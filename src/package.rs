use serde::Deserialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize, Debug)]
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
        .unwrap_or_else(|| PathBuf::from("/var/lib/matepkg"))
}

pub fn list_dir() -> PathBuf {
    db_root().join("list")
}
pub fn desc_dir() -> PathBuf {
    db_root().join("desc")
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
