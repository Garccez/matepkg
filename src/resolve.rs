use crate::package::{Metadata, installed_packages, satisfies, split_dependency, sync_packages};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct PackageRef {
    pub metadata: Metadata,
    pub repo: String,
    pub filename: String,
}

pub fn resolve(pkgname: &str) -> Result<Vec<PackageRef>, Box<dyn std::error::Error>> {
    let available = sync_packages(None)?;
    let mut by_name = HashMap::new();
    for (metadata, path) in available {
        let repo = path
            .parent()
            .and_then(|p| p.file_name())
            .ok_or("invalid sync path")?
            .to_string_lossy()
            .into_owned();
        let source = std::fs::read_to_string(path)?;
        let record: crate::repo::SyncPackage = toml::from_str(&source)?;
        let package_name = metadata.pkgname.clone();
        if by_name
            .insert(
                metadata.pkgname.clone(),
                PackageRef {
                    metadata,
                    repo,
                    filename: record.filename,
                },
            )
            .is_some()
        {
            return Err(format!(
                "multiple available versions for '{}'; resolve manually",
                package_name
            )
            .into());
        }
    }
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

#[cfg(test)]
mod tests {
    use crate::package::split_dependency;

    #[test]
    fn parses_dependency_constraints() {
        assert_eq!(split_dependency("glibc>=2.38"), ("glibc", ">=2.38"));
        assert_eq!(split_dependency("openssl"), ("openssl", ""));
        assert_eq!(split_dependency(" zlib < 2.0"), ("zlib", "< 2.0"));
    }
}
