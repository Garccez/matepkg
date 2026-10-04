use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tar::Archive;
use zstd::stream::Decoder;

use crate::package::{
    Metadata, desc_dir, hooks_dir, install_root, installed_packages, list_dir, run_hook,
    safe_archive_path, satisfies, split_dependency,
};

pub fn install_package(package_path_arg: &Path) -> Result<String, Box<dyn std::error::Error>> {
    install_package_impl(package_path_arg, true)
}

pub(crate) fn install_package_without_hooks(
    package_path_arg: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    install_package_impl(package_path_arg, false)
}

fn install_package_impl(
    package_path_arg: &Path,
    run_install_hooks: bool,
) -> Result<String, Box<dyn std::error::Error>> {
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
    let installed = installed_packages()?;
    for dependency in &metadata.deps {
        let (dependency_name, constraint) = split_dependency(dependency);
        let dependency_name = dependency_name.trim();
        let satisfied = installed.iter().any(|(installed, _)| {
            installed.pkgname == dependency_name
                && (constraint.is_empty() || satisfies(&installed.version, constraint))
        });
        if !satisfied {
            return Err(format!("dependency '{}' is not installed", dependency).into());
        }
    }

    let mut hooks_content = None;
    let hooks_file = File::open(&package_path)?;
    let hooks_decoder = Decoder::new(hooks_file)?;
    let mut hooks_archive = Archive::new(hooks_decoder);
    for entry in hooks_archive.entries()? {
        let mut entry = entry?;
        if entry.path()? == Path::new("hooks.sh") {
            let mut content = String::new();
            std::io::Read::read_to_string(&mut entry, &mut content)?;
            hooks_content = Some(content);
            break;
        }
    }
    let hook_path = hooks_dir().join(format!("{}.sh", metadata.canonical_name()));
    if let Some(content) = &hooks_content {
        fs::create_dir_all(hooks_dir())?;
        fs::write(&hook_path, content)?;
        if run_install_hooks {
            run_hook(&hook_path, "pre_install", &[])?;
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

        let target = install_root().join(&path);
        if (target.is_file() || target.is_symlink())
            && path != Path::new("desc.toml")
            && path != Path::new("hooks.sh")
            && !path_owned_by_installed_package(&path)?
        {
            let rollback_failures = rollback_extracted(&extracted_paths);
            let _ = fs::remove_file(install_root().join("desc.toml"));
            let _ = fs::remove_file(&hook_path);
            let rollback_status = rollback_status(&rollback_failures);
            return Err(
                format!(
                    "refusing to overwrite existing file '{}'; {}; package hooks may have had partial side effects",
                    target.display(), rollback_status
                ).into(),
            );
        }
        if path == Path::new("hooks.sh") {
            continue;
        }
        if let Err(error) = entry.unpack_in(install_root()) {
            let rollback_failures = rollback_extracted(&extracted_paths);
            if install_root().join("desc.toml").exists() {
                let _ = fs::remove_file(install_root().join("desc.toml"));
            }
            if hook_path.exists() {
                let _ = fs::remove_file(&hook_path);
            }
            let rollback_status = rollback_status(&rollback_failures);
            return Err(format!(
                "package extraction failed after {} entries: {}; {}; package hooks may have had partial side effects",
                extracted_paths.len(), error, rollback_status
            ).into());
        }
        extracted_paths.push(path);
    }

    if extracted_paths.is_empty() {
        return Err("Package seems empty. Nothing was done.".into());
    }
    let (file_count, directory_count) =
        extracted_paths
            .iter()
            .fold((0, 0), |(files, directories), path| {
                if install_root().join(path).is_dir() {
                    (files, directories + 1)
                } else {
                    (files + 1, directories)
                }
            });
    println!(
        "=> {} files and {} directories successfully extracted.",
        file_count, directory_count
    );

    // -- Post-extraction management --

    // Reads metadata to get canonical package name
    let temp_desc_path = install_root().join("desc.toml");
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
    if let Err(error) = fs::write(
        db_list_dir.join(format!("{}.list", canonical_name)),
        list_content,
    ) {
        let rollback_failures = rollback_extracted(&extracted_paths);
        let _ = fs::remove_file(&temp_desc_path);
        let _ = fs::remove_file(&hook_path);
        let rollback_status = rollback_status(&rollback_failures);
        return Err(format!(
            "failed to register package; {}; package hooks may have had partial side effects: {error}",
            rollback_status
        ).into());
    }
    println!("=> Files manifest stored at the database.");

    // Moves the description file to the "database".
    let db_desc_dir = desc_dir();
    fs::create_dir_all(&db_desc_dir)?;
    let final_desc_path = db_desc_dir.join(format!("{}.toml", canonical_name));
    fs::rename(&temp_desc_path, &final_desc_path)?;
    println!("=> Metadata moved to '{}'", final_desc_path.display());

    if run_install_hooks {
        run_hook(&hook_path, "post_install", &[])?;
    }

    println!("\nPackage '{}' successfully installed!", canonical_name);
    Ok(canonical_name)
}

fn rollback_extracted(paths: &[PathBuf]) -> Vec<PathBuf> {
    rollback_extracted_at(&install_root(), paths)
}

fn rollback_extracted_at(root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut failures = Vec::new();
    for path in paths.iter().rev() {
        let full_path = root.join(path);
        if full_path.is_file() || full_path.is_symlink() {
            if fs::remove_file(&full_path).is_err() {
                failures.push(path.clone());
            }
        } else if full_path.is_dir() && fs::remove_dir(&full_path).is_err() {
            failures.push(path.clone());
        }
    }
    failures
}

fn rollback_status(failures: &[PathBuf]) -> String {
    if failures.is_empty() {
        "file rollback completed".to_owned()
    } else {
        format!(
            "ROLLBACK INCOMPLETE for: {}",
            failures
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
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

#[cfg(test)]
mod tests {
    use super::rollback_status;
    use std::path::PathBuf;

    #[test]
    fn reports_successful_and_incomplete_rollbacks() {
        assert_eq!(rollback_status(&[]), "file rollback completed");
        assert_eq!(
            rollback_status(&[
                PathBuf::from("usr/bin/demo"),
                PathBuf::from("etc/demo.conf")
            ]),
            "ROLLBACK INCOMPLETE for: usr/bin/demo, etc/demo.conf"
        );
    }

    #[test]
    fn rollback_removes_files_from_alternate_root() {
        let root = std::env::temp_dir().join(format!("matepkg-rollback-{}", std::process::id()));
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::write(root.join("usr/bin/demo"), b"demo").unwrap();
        let failures = super::rollback_extracted_at(&root, &[PathBuf::from("usr/bin/demo")]);
        assert!(failures.is_empty());
        assert!(!root.join("usr/bin/demo").exists());

        let _ = std::fs::remove_dir_all(root);
    }
}
