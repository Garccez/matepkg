use std::collections::HashSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tar::Archive;
use zstd::stream::Decoder;
use version_compare::{Version, Cmp};

use crate::package::Metadata;
use crate::install::install_package;

// -- auxiliary function 1: analyzes a new .mtz package --
/// Reads the metadata and manifest of a new package file without extracting it
fn analyze_new_package(package_path: &Path) -> Result<(Metadata, HashSet<PathBuf>), Box<dyn std::error::Error>> {
    // TODO: Add checksum validation?
    println!("=> Analyzing metadata and manifest of '{}'...", package_path.display());

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

// -- auxiliary function 2: analyzes an already installed package --
/// Finds and reads the metadata and manifest of an already installed package.
fn find_and_analyze_installed_package(pkgname: &str) -> Result<(Metadata, HashSet<PathBuf>, String), Box<dyn std::error::Error>> {
    let desc_dir = Path::new("/var/lib/matepkg/desc/");
    let mut found_package: Option<PathBuf> = None;

    for entry in fs::read_dir(desc_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.file_name().unwrap_or_default().to_string_lossy().starts_with(pkgname) {
            if found_package.is_some() {
                return Err("Multiple versions of the same package found in the database. Solve manually.".into());
            }
            found_package = Some(path);
        }
    }

    match found_package {
        Some(desc_path) => {
            let metadata: Metadata = toml::from_str(&fs::read_to_string(&desc_path)?)?;
            let canonical_name = format!("{}-{}-{}", metadata.pkgname, metadata.version, metadata.build);
            
            let list_path = PathBuf::from(format!("/var/lib/matepkg/list/{}.list", canonical_name));
            let manifest: HashSet<PathBuf> = fs::read_to_string(list_path)?.lines().map(PathBuf::from).collect();
            
            Ok((metadata, manifest, canonical_name))
        }
        None => Err("No installed version of the package found. Try installing it.".into()),
    }
}

// -- main function: upgrades (actually) --
pub fn upgrade_package(new_package_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // analyze the new package.
    let (new_metadata, new_manifest) = analyze_new_package(new_package_path)?;
    println!("=> New package: {}, version {}, build {}", new_metadata.pkgname, new_metadata.version, new_metadata.build);

    // find and analyze the old package.
    let (old_metadata, old_manifest, old_canonical_name) = find_and_analyze_installed_package(&new_metadata.pkgname)?;
    println!("=> Installed version: {}, version {}, build {}", old_metadata.pkgname, old_metadata.version, old_metadata.build);

    // compare versions.
    let new_ver = Version::from(&new_metadata.version).ok_or("New package version invalid.")?;
    let old_ver = Version::from(&old_metadata.version).ok_or("Installed package version invalid.")?;

    match new_ver.compare(old_ver) {
        Cmp::Lt | Cmp::Eq if new_metadata.build <= old_metadata.build => {
            return Err("The provided version is not an upgrade (it is older or the same). Use --allow-downgrade to force the upgrade.".into());
        }
        _ => println!("=> Version validated. Continuing upgrade."),
    }

    // calculate the difference of files
    let obsolete_files: Vec<_> = old_manifest.difference(&new_manifest).collect();
    println!("=> {} obsolete files to remove.", obsolete_files.len());

    // do the transaction
    // installing the new package.
    println!("==> [1/3] Installing the new version…");
    install_package(new_package_path)?;

    // remove the files that have become obsolete.
    println!("==> [2/3] Removing obsolte version of the old version…");
    let mut dirs_to_check: HashSet<PathBuf> = HashSet::new(); // to remove empty dirs
    for file_path in obsolete_files {
        let full_path = Path::new("/").join(file_path);
        if full_path.is_file() || full_path.is_symlink() {
            fs::remove_file(&full_path)?;
            if let Some(parent) = full_path.parent() {
                dirs_to_check.insert(parent.to_path_buf());
            }
        }
    }
    // to remove empty dirs²
    for dir in dirs_to_check {
        if dir.read_dir()?.next().is_none() { // checks if it's empty
            let _ = fs::remove_dir(dir);
        }
    }

    // clean the old matadata and manifest from the database.
    println!("--> [3/3] Cleaning old registry files from the database…");
    fs::remove_file(format!("/var/lib/matepkg/list/{}.list", old_canonical_name))?;
    fs::remove_file(format!("/var/lib/matepkg/desc/{}.toml", old_canonical_name))?;

    println!("\n=> '{}' upgraded successfully!", new_metadata.pkgname);
    Ok(())
}
