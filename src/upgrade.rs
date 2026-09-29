use std::collections::HashSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tar::Archive;
use version_compare::{Cmp, Version};
use zstd::stream::Decoder;

use crate::install::install_package_without_hooks;
use crate::package::{Metadata, desc_dir, hooks_dir, install_root, list_dir, run_hook};

fn is_upgrade(new: &Metadata, old: &Metadata) -> Result<bool, Box<dyn std::error::Error>> {
    let new_version = Version::from(&new.version).ok_or("New package version invalid.")?;
    let old_version = Version::from(&old.version).ok_or("Installed package version invalid.")?;
    Ok(match new_version.compare(old_version) {
        Cmp::Gt => true,
        Cmp::Eq => new.build > old.build,
        Cmp::Lt | Cmp::Ne | Cmp::Le | Cmp::Ge => false,
    })
}

// -- Auxiliary function 1: analyzes a new .mtz package --
/// Reads the metadata and manifest of a new package file without extracting it
fn analyze_new_package(
    package_path: &Path,
) -> Result<(Metadata, HashSet<PathBuf>), Box<dyn std::error::Error>> {
    let checksum_path = package_path.with_extension("mtz.sha256");
    if !checksum_path.exists() {
        return Err(format!("checksum file not found at '{}'", checksum_path.display()).into());
    }
    println!(
        "=> Analyzing metadata and manifest of '{}'...",
        package_path.display()
    );

    let package_file = File::open(package_path)?;
    let decoder = Decoder::new(package_file)?;
    let mut archive = Archive::new(decoder);

    let mut metadata: Option<Metadata> = None;
    let mut manifest = HashSet::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();

        if path == Path::new("desc.toml") {
            let mut content = String::new();
            std::io::Read::read_to_string(&mut entry, &mut content)?;
            metadata = Some(toml::from_str(&content)?);
        }

        if path.to_string_lossy() != "." {
            manifest.insert(path);
        }
    }

    match metadata {
        Some(md) => Ok((md, manifest)),
        None => Err("Invalid package: 'desc.toml' not found.".into()),
    }
}

// -- Auxiliary function 2: analyzes an already installed package --
/// Finds and reads the metadata and manifest of an already installed package.
fn find_and_analyze_installed_package(
    pkgname: &str,
) -> Result<(Metadata, HashSet<PathBuf>, String), Box<dyn std::error::Error>> {
    let desc_dir = desc_dir();
    let mut found_package: Option<PathBuf> = None;

    for entry in fs::read_dir(desc_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with(pkgname)
        {
            if found_package.is_some() {
                return Err(
                    "Multiple versions of the same package found in the database. Solve manually."
                        .into(),
                );
            }
            found_package = Some(path);
        }
    }

    match found_package {
        Some(desc_path) => {
            let metadata: Metadata = toml::from_str(&fs::read_to_string(&desc_path)?)?;
            let canonical_name = metadata.canonical_name();

            let list_path = list_dir().join(format!("{}.list", canonical_name));
            let manifest: HashSet<PathBuf> = fs::read_to_string(list_path)?
                .lines()
                .map(PathBuf::from)
                .collect();

            Ok((metadata, manifest, canonical_name))
        }
        None => Err("No installed version of the package found. Try installing it.".into()),
    }
}

// -- Main function: upgrades (actually) --
pub fn upgrade_package(new_package_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Analyze the new package.
    let (new_metadata, new_manifest) = analyze_new_package(new_package_path)?;
    println!(
        "=> New package: {}, version {}, build {}",
        new_metadata.pkgname, new_metadata.version, new_metadata.build
    );

    // Find and analyze the old package.
    let (old_metadata, old_manifest, old_canonical_name) =
        find_and_analyze_installed_package(&new_metadata.pkgname)?;
    println!(
        "=> Installed version: {}, version {}, build {}",
        old_metadata.pkgname, old_metadata.version, old_metadata.build
    );

    // Compare versions.
    if !is_upgrade(&new_metadata, &old_metadata)? {
        return Err("The provided version is not an upgrade.".into());
    }
    println!("=> Version validated. Continuing upgrade.");
    let old_hook = hooks_dir().join(format!("{}.sh", old_canonical_name));
    run_hook(&old_hook, "pre_upgrade", &[&old_metadata.version])?;

    // Calculate the difference of files
    let obsolete_files: Vec<_> = old_manifest.difference(&new_manifest).collect();
    println!("=> {} obsolete files to remove.", obsolete_files.len());

    // Do the transaction
    // Installing the new package.
    println!("==> [1/3] Installing the new version…");
    install_package_without_hooks(new_package_path)?;

    // Remove the files that have become obsolete.
    println!("==> [2/3] Removing obsolete files of the old version…");
    let mut dirs_to_check: HashSet<PathBuf> = HashSet::new(); // to remove empty dirs
    for file_path in obsolete_files {
        let full_path = install_root().join(file_path);
        if full_path.is_file() || full_path.is_symlink() {
            fs::remove_file(&full_path)?;
            if let Some(parent) = full_path.parent() {
                dirs_to_check.insert(parent.to_path_buf());
            }
        }
    }
    // Remove empty dirs²
    for dir in dirs_to_check {
        if dir.read_dir()?.next().is_none() {
            // Checks if it's empty
            let _ = fs::remove_dir(dir);
        }
    }

    // Clean the old matadata and manifest from the database.
    println!("--> [3/3] Cleaning old registry files from the database…");
    fs::remove_file(list_dir().join(format!("{}.list", old_canonical_name)))?;
    fs::remove_file(desc_dir().join(format!("{}.toml", old_canonical_name)))?;
    if old_hook.exists() {
        fs::remove_file(old_hook)?;
    }
    let new_hook = hooks_dir().join(format!("{}.sh", new_metadata.canonical_name()));
    run_hook(&new_hook, "post_upgrade", &[&old_metadata.version])?;

    println!("\n=> '{}' upgraded successfully!", new_metadata.pkgname);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_upgrade;
    use crate::package::Metadata;

    fn metadata(version: &str, build: &str) -> Metadata {
        Metadata {
            maintainer: "test".into(),
            pkgname: "demo".into(),
            version: version.into(),
            build: build.into(),
            license: "MIT".into(),
            desc: "demo".into(),
            url: "https://example.invalid".into(),
            deps: Vec::new(),
        }
    }

    #[test]
    fn rejects_version_downgrade() {
        assert!(!is_upgrade(&metadata("1.0", "1"), &metadata("2.0", "1")).unwrap());
    }

    #[test]
    fn accepts_newer_version_and_build() {
        assert!(is_upgrade(&metadata("2.0", "1"), &metadata("1.0", "9")).unwrap());
        assert!(is_upgrade(&metadata("1.0", "2"), &metadata("1.0", "1")).unwrap());
        assert!(!is_upgrade(&metadata("1.0", "1"), &metadata("1.0", "1")).unwrap());
    }
}
