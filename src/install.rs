use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Archive;
use zstd::stream::Decoder;

use crate::package::{Metadata, desc_dir, list_dir, safe_archive_path};

pub fn install_package(package_path_arg: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // -- Checking for file extension --
    let package_path = if package_path_arg.extension().and_then(|s| s.to_str()) == Some("mtz") {
        package_path_arg.to_path_buf()
    } else {
        let mut path_str = package_path_arg.as_os_str().to_owned();
        path_str.push(".mtz");
        PathBuf::from(path_str)
    };

    // -- Integrity and existence validation --
    println!("=> Checking package '{}'…", package_path.display());

    if !package_path.exists() {
        return Err(format!("Package file not found at '{}'", package_path.display()).into());
    }

    let checksum_path = package_path.with_extension("mtz.sha256");
    if !checksum_path.exists() {
        return Err(format!(
            "Corresponding checksum file not found at '{}'",
            checksum_path.display()
        )
        .into());
    }

    // Checks checksum
    let expected_checksum = fs::read_to_string(&checksum_path)?
        .split_whitespace()
        .next()
        .ok_or("Badly formatted checksum file.")?
        .to_lowercase();

    let mut file_for_hash = File::open(&package_path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file_for_hash, &mut hasher)?;
    let calculated_hash = format!("{:x}", hasher.finalize());

    if expected_checksum != calculated_hash {
        return Err(
            "Checksum checking failed! The package may have been corrupted or altered.".into(),
        );
    }
    println!("=> Checksum OK.");

    // -- Verification, extraction and generation of manifest --
    println!("=> Verifying package content before extraction…");

    let firstopen_package_file = File::open(&package_path)?;
    let firstopen_decoder = Decoder::new(firstopen_package_file)?;
    let mut firstopen_archive = Archive::new(firstopen_decoder);

    let entries: Vec<_> = firstopen_archive
        .entries()?
        .collect::<Result<Vec<_>, _>>()?;

    let has_desc_toml = entries.iter().any(|entry| {
        if let Ok(path) = entry.path() {
            return path == Path::new("desc.toml");
        }
        false
    });

    if !has_desc_toml {
        return Err(
            "Invalid package: 'desc.toml' file was not found. No changes were made to the system."
                .into(),
        );
    }

    for entry in &entries {
        let path = entry.path()?;
        safe_archive_path(&path)?;
    }
    let mut metadata_content = String::new();
    let metadata_file = File::open(&package_path)?;
    let metadata_decoder = Decoder::new(metadata_file)?;
    let mut metadata_archive = Archive::new(metadata_decoder);
    for entry in metadata_archive.entries()? {
        let mut entry = entry?;
        if entry.path()? == Path::new("desc.toml") {
            std::io::Read::read_to_string(&mut entry, &mut metadata_content)?;
            break;
        }
    }
    let metadata: Metadata = toml::from_str(&metadata_content)?;
    metadata
        .validate()
        .map_err(|e| format!("Invalid metadata: {}", e))?;
    for dependency in &metadata.deps {
        let dependency_name = dependency
            .split(['=', '<', '>', ' '])
            .next()
            .unwrap_or(dependency);
        let satisfied = desc_dir().exists()
            && fs::read_dir(desc_dir())?
                .filter_map(Result::ok)
                .any(|entry| {
                    fs::read_to_string(entry.path())
                        .ok()
                        .and_then(|content| toml::from_str::<Metadata>(&content).ok())
                        .map(|installed| installed.pkgname == dependency_name)
                        .unwrap_or(false)
                });
        if !satisfied {
            return Err(format!("dependency '{}' is not installed", dependency).into());
        }
    }

    println!("=> Content verified. 'desc.toml' was found.");

    println!("=> Extracting file to root '/'…");

    let package_file = File::open(&package_path)?;
    let decoder = Decoder::new(package_file)?;
    let mut archive = Archive::new(decoder);

    // Stores the list of all files to the manifest and for cleaning.
    let mut extracted_paths: Vec<PathBuf> = Vec::new();

    // Iterates over the package, stores the path of every file and extracts it.
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();

        if path.to_string_lossy() == "./" {
            continue;
        }

        let target = PathBuf::from("/").join(&path);
        if (target.is_file() || target.is_symlink())
            && path != Path::new("desc.toml")
            && path != Path::new("pos.sh")
            && !path_owned_by_installed_package(&path)?
        {
            return Err(
                format!("refusing to overwrite existing file '{}'", target.display()).into(),
            );
        }
        entry.unpack_in("/")?;
        extracted_paths.push(path);
    }

    if extracted_paths.is_empty() {
        return Err("Package seems empty. Nothing was done.".into());
    }
    println!("=> {} files successfully extracted.", extracted_paths.len());

    // -- Post-extraction management --

    // Reads metadata to get canonical package name
    let temp_desc_path = PathBuf::from("/desc.toml");
    let metadata_content = fs::read_to_string(&temp_desc_path)?;
    let metadata: Metadata = toml::from_str(&metadata_content)?;
    let canonical_name = metadata.canonical_name();

    // Stores files manifest at the "database" for a future removal.
    let db_list_dir = list_dir();
    fs::create_dir_all(&db_list_dir)?;
    let list_content = extracted_paths
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        db_list_dir.join(format!("{}.list", canonical_name)),
        list_content,
    )?;
    println!("=> Files manifest stored at the database.");

    // Moves the description file to the "database".
    let db_desc_dir = desc_dir();
    fs::create_dir_all(&db_desc_dir)?;
    let final_desc_path = db_desc_dir.join(format!("{}.toml", canonical_name));
    fs::rename(&temp_desc_path, &final_desc_path)?;
    println!("=> Metadata moved to '{}'", final_desc_path.display());

    // -- Script execution --
    let pos_install_script = PathBuf::from("/pos.sh");
    if pos_install_script.exists() {
        println!("=> Running post-install script (pos.sh)…");
        let status = Command::new("bash").arg(&pos_install_script).status()?;
        if !status.success() {
            // Maybe we should revert the installation here in the future?
            eprintln!("[WARNING] The post-install script returned an error.");
        }
    }

    // -- Final cleaning --
    println!("=> Cleaning control files from the root…");
    // Removes the control files that were extracted to the root.
    // Using the list `extracted_paths` to guarantee that only what was extracted is deleted.
    if pos_install_script.exists() {
        fs::remove_file(pos_install_script)?;
    }

    println!("\nPackage '{}' successfully installed!", canonical_name);
    Ok(())
}

fn path_owned_by_installed_package(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    if !list_dir().exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(list_dir())? {
        let content = fs::read_to_string(entry?.path())?;
        if content.lines().any(|line| Path::new(line) == path) {
            return Ok(true);
        }
    }
    Ok(false)
}
