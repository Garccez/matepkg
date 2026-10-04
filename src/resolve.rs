use crate::package::{
    Metadata, installed_packages, repository_priority, satisfies, split_dependency, sync_packages,
};
use std::collections::{HashMap, HashSet};
use version_compare::Version;

#[derive(Clone, Debug)]
pub struct PackageRef {
    pub metadata: Metadata,
    pub repo: String,
    pub filename: String,
}

pub fn resolve(pkgname: &str) -> Result<Vec<PackageRef>, Box<dyn std::error::Error>> {
    let available = sync_packages(None)?;
    let priorities = repository_priority()?;
    let mut candidates: HashMap<String, Vec<PackageRef>> = HashMap::new();
    for (metadata, path) in available {
        let repo = path
            .parent()
            .and_then(|p| p.file_name())
            .ok_or("invalid sync path")?
            .to_string_lossy()
            .into_owned();
        let source = std::fs::read_to_string(path)?;
        let record: crate::repo::SyncPackage = toml::from_str(&source)?;
        candidates
            .entry(metadata.pkgname.clone())
            .or_default()
            .push(PackageRef {
                metadata,
                repo,
                filename: record.filename,
            });
    }
    let by_name: HashMap<_, _> = candidates
        .into_iter()
        .map(|(name, entries)| select_candidate(&name, entries, &priorities))
        .collect::<Result<_, _>>()?;
    let installed = installed_packages()?;
    let installed_by_name: HashMap<_, _> = installed
        .into_iter()
        .map(|(m, _)| (m.pkgname.clone(), m))
        .collect();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut result = Vec::new();
    fn visit(
        name: &str,
        by_name: &HashMap<String, PackageRef>,
        installed: &HashMap<String, Metadata>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        result: &mut Vec<PackageRef>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if visited.contains(name) {
            return Ok(());
        }
        if visiting.contains(name) {
            return Err(format!("dependency cycle detected at '{}'", name).into());
        }
        if installed.contains_key(name) {
            visited.insert(name.to_string());
            return Ok(());
        }
        let package = by_name
            .get(name)
            .ok_or_else(|| format!("package '{}' is not available in sync db", name))?;
        visiting.insert(name.to_string());
        for dep in &package.metadata.deps {
            let (dep_name, constraint) = split_dependency(dep);
            if let Some(installed) = installed.get(dep_name) {
                if !constraint.is_empty() && !satisfies(&installed.version, constraint) {
                    return Err(
                        format!("installed '{}' does not satisfy '{}'", dep, dep_name).into(),
                    );
                }
            } else {
                if !constraint.is_empty() {
                    let candidate = by_name.get(dep_name).ok_or_else(|| {
                        format!("dependency '{}' is not available in sync db", dep_name)
                    })?;
                    if !satisfies(&candidate.metadata.version, constraint) {
                        return Err(format!(
                            "available '{}' does not satisfy '{}'",
                            candidate.metadata.canonical_name(),
                            dep
                        )
                        .into());
                    }
                }
                visit(dep_name, by_name, installed, visiting, visited, result)?;
            }
        }
        visiting.remove(name);
        visited.insert(name.to_string());
        result.push(package.clone());
        Ok(())
    }

    visit(
        pkgname,
        &by_name,
        &installed_by_name,
        &mut visiting,
        &mut visited,
        &mut result,
    )?;
    Ok(result)
}

fn select_candidate(
    name: &str,
    mut entries: Vec<PackageRef>,
    priorities: &[String],
) -> Result<(String, PackageRef), Box<dyn std::error::Error>> {
    if entries.is_empty() {
        return Err(format!("no available versions for '{}'", name).into());
    }
    entries.sort_by(|a, b| {
        let ar = priorities
            .iter()
            .position(|repo| repo == &a.repo)
            .unwrap_or(usize::MAX);
        let br = priorities
            .iter()
            .position(|repo| repo == &b.repo)
            .unwrap_or(usize::MAX);
        ar.cmp(&br).then_with(|| {
            match (
                Version::from(&a.metadata.version),
                Version::from(&b.metadata.version),
            ) {
                (Some(av), Some(bv)) => match av.compare(bv) {
                    version_compare::Cmp::Gt => std::cmp::Ordering::Less,
                    version_compare::Cmp::Lt => std::cmp::Ordering::Greater,
                    _ => b.metadata.build.cmp(&a.metadata.build),
                },
                _ => std::cmp::Ordering::Equal,
            }
        })
    });
    Ok((name.to_owned(), entries.remove(0)))
}

#[cfg(test)]
mod tests {
    use super::{PackageRef, select_candidate};
    use crate::package::{Metadata, split_dependency};

    fn package(repo: &str, version: &str) -> PackageRef {
        PackageRef {
            metadata: Metadata {
                maintainer: "t".into(),
                pkgname: "demo".into(),
                version: version.into(),
                build: "1".into(),
                license: "MIT".into(),
                desc: "d".into(),
                url: "https://example.invalid".into(),
                deps: vec![],
            },
            repo: repo.into(),
            filename: format!("demo-{version}-1.mtz"),
        }
    }

    #[test]
    fn parses_dependency_constraints() {
        assert_eq!(split_dependency("glibc>=2.38"), ("glibc", ">=2.38"));
        assert_eq!(split_dependency("openssl"), ("openssl", ""));
        assert_eq!(split_dependency(" zlib < 2.0"), ("zlib", "< 2.0"));
    }

    #[test]
    fn chooses_priority_repo_before_newer_lower_priority_version() {
        let (_, selected) = select_candidate(
            "demo",
            vec![package("core", "1.0"), package("testing", "9.0")],
            &["core".into(), "testing".into()],
        )
        .unwrap();
        assert_eq!(selected.repo, "core");
    }

    #[test]
    fn chooses_highest_version_inside_same_repo() {
        let (_, selected) = select_candidate(
            "demo",
            vec![package("core", "1.0"), package("core", "2.0")],
            &["core".into()],
        )
        .unwrap();
        assert_eq!(selected.metadata.version, "2.0");
    }
}
