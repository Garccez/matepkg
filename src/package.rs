use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Metadata {
    pub maintainer: String,
    pub pkgname: String,
    pub version: String,
    pub build: String,
    pub license: String,
    pub desc: String,
    pub url: String,
    #[serde(default)]
    pub deps: Vec<String>,
}

impl Metadata {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("maintainer", &self.maintainer),
            ("pkgname", &self.pkgname),
            ("version", &self.version),
            ("build", &self.build),
            ("license", &self.license),
            ("desc", &self.desc),
            ("url", &self.url),
        ] {
            if value.trim().is_empty() {
                return Err(format!("metadata field '{}' cannot be empty", name));
            }
        }
        if self.pkgname.contains(['-', '/', '\\'])
            || self.version.contains(['/', '\\'])
            || self.build.contains(['/', '\\'])
        {
            return Err("pkgname, version and build contain an invalid path character".into());
        }
        Ok(())
    }

    pub fn canonical_name(&self) -> String {
        format!("{}-{}-{}", self.pkgname, self.version, self.build)
    }
}

pub fn db_root() -> PathBuf {
    std::env::var_os("MATEPKG_DB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| install_root().join("var/lib/matepkg"))
}

pub fn install_root() -> PathBuf {
    std::env::var_os("MATEPKG_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn list_dir() -> PathBuf {
    db_root().join("list")
}
pub fn desc_dir() -> PathBuf {
    db_root().join("desc")
}
pub fn sync_dir() -> PathBuf {
    db_root().join("sync")
}
pub fn cache_dir() -> PathBuf {
    db_root().join("cache")
}
pub fn hooks_dir() -> PathBuf {
    db_root().join("hooks")
}

pub fn run_hook(path: &Path, phase: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if install_root() != Path::new("/") {
        eprintln!(
            "[WARNING] Skipping {}: package hooks are only run when MATEPKG_ROOT is '/'.",
            phase
        );
        return Ok(());
    }
    if !path.exists() {
        return Ok(());
    }
    let mut command = Command::new("bash");
    command
        .arg("-c")
        .arg(format!("source \"$1\"; {} \"${{@:2}}\"", phase));
    command.arg("matepkg-hook").arg(path);
    command.args(args);
    let status = command.status()?;
    if !status.success() {
        eprintln!("[WARNING] Hook '{}' returned an error.", phase);
    }
    Ok(())
}

pub fn safe_archive_path(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Err(format!(
            "absolute archive path is not allowed: {}",
            path.display()
        ));
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(format!("unsafe archive path: {}", path.display()));
        }
    }
    Ok(())
}

pub fn installed_packages() -> Result<Vec<(Metadata, PathBuf)>, Box<dyn std::error::Error>> {
    let dir = desc_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() {
            let metadata: Metadata = toml::from_str(&fs::read_to_string(&path)?)?;
            packages.push((metadata, path));
        }
    }
    Ok(packages)
}

pub fn sync_packages(
    repo: Option<&str>,
) -> Result<Vec<(Metadata, PathBuf)>, Box<dyn std::error::Error>> {
    let root = sync_dir();
    let repos: Vec<PathBuf> = match repo {
        Some(name) => vec![root.join(name)],
        None => {
            if root.exists() {
                fs::read_dir(&root)?
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    let mut packages = Vec::new();
    for dir in repos {
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let metadata: Metadata = toml::from_str(&fs::read_to_string(&path)?)?;
                packages.push((metadata, path));
            }
        }
    }
    Ok(packages)
}

pub fn repository_priority() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if let Some(value) = std::env::var_os("MATEPKG_REPOS") {
        return Ok(value
            .to_string_lossy()
            .split(':')
            .map(str::to_owned)
            .filter(|v| !v.is_empty())
            .collect());
    }
    let root = sync_dir();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<_> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

pub fn satisfies(installed: &str, constraint: &str) -> bool {
    let constraint = constraint.trim();
    let (operator, required) = ["<=", ">=", "!=", "=", "<", ">"]
        .iter()
        .find_map(|op| constraint.strip_prefix(op).map(|v| (*op, v)))
        .unwrap_or(("=", constraint));
    let (Some(left), Some(right)) = (
        version_compare::Version::from(installed),
        version_compare::Version::from(required.trim()),
    ) else {
        return false;
    };
    match operator {
        "=" => left.compare(right) == version_compare::Cmp::Eq,
        "!=" => left.compare(right) != version_compare::Cmp::Eq,
        "<" => left.compare(right) == version_compare::Cmp::Lt,
        "<=" => matches!(
            left.compare(right),
            version_compare::Cmp::Lt | version_compare::Cmp::Eq
        ),
        ">" => left.compare(right) == version_compare::Cmp::Gt,
        ">=" => matches!(
            left.compare(right),
            version_compare::Cmp::Gt | version_compare::Cmp::Eq
        ),
        _ => false,
    }
}

pub fn split_dependency(value: &str) -> (&str, &str) {
    let Some(index) = value.find(['<', '>', '=', '!']) else {
        return (value.trim(), "");
    };
    (value[..index].trim(), value[index..].trim())
}

#[cfg(test)]
mod tests {
    #[test]
    fn compares_version_constraints() {
        assert!(super::satisfies("2.38", ">=2.38"));
        assert!(super::satisfies("2.39", ">2.38"));
        assert!(super::satisfies("2.35", "<2.38"));
        assert!(!super::satisfies("2.35", ">=2.38"));
        assert!(super::satisfies("2.38", "=2.38"));
        assert!(super::satisfies("2.38", "<=2.38"));
        assert!(super::satisfies("2.38", "!=2.39"));
        assert!(!super::satisfies("invalid", ">=2.38"));
    }

    #[test]
    fn validates_metadata_and_canonical_name() {
        let metadata = super::Metadata {
            maintainer: "test".into(),
            pkgname: "demo".into(),
            version: "1.0".into(),
            build: "1".into(),
            license: "MIT".into(),
            desc: "demo package".into(),
            url: "https://example.invalid".into(),
            deps: vec![],
        };
        assert!(metadata.validate().is_ok());
        assert_eq!(metadata.canonical_name(), "demo-1.0-1");
        let mut invalid = metadata.clone();
        invalid.pkgname = "bad-name".into();
        assert!(invalid.validate().is_err());
        invalid.pkgname = "demo".into();
        invalid.desc.clear();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(super::safe_archive_path(std::path::Path::new("usr/bin/mate")).is_ok());
        assert!(super::safe_archive_path(std::path::Path::new("../etc/passwd")).is_err());
        assert!(super::safe_archive_path(std::path::Path::new("/etc/passwd")).is_err());
    }
}
