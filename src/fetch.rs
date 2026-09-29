use crate::package::cache_dir;
use crate::repo::repo_source;
use crate::resolve::PackageRef;
use std::fs;
use std::path::PathBuf;

pub fn fetch(package: &PackageRef) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo_dir = crate::package::sync_dir().join(&package.repo);
    let source = repo_source(&repo_dir, &package.filename)?;
    let cache = cache_dir();
    fs::create_dir_all(&cache)?;
    let target = cache.join(&package.filename);
    fs::copy(&source, &target)?;
    let checksum = source.with_extension("mtz.sha256");
    fs::copy(&checksum, target.with_extension("mtz.sha256"))?;
    Ok(target)
}
