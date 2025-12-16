use std::fs;
use std::path::PathBuf;

pub fn remove_package(package_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // -- Finding package manifest path --
    println!("=> Preparing to remove '{}'…", package_name);
    let list_path = PathBuf::from(format!("/var/lib/matepkg/list/{}.list", package_name));
    let desc_path = PathBuf::from(format!("/var/lib/matepkg/desc/{}.toml", package_name));

    if !list_path.exists() || !desc_path.exists() {
        return Err(format!("Package '{}' doesn't seem to be installed (manifest files not found).", package_name).into());
    }

    // -- Reading Manifest --
    println!("=> Reading package's manifest…");
    let list_content = fs::read_to_string(&list_path)?;
    
    let paths_to_remove: Vec<PathBuf> = list_content.lines().map(PathBuf::from).collect();

    if paths_to_remove.is_empty() {
        eprintln!("[WARNING] Package's manifest is empty.");
    }

    // -- Removal: First Pass (files) ----
    println!("=> Removing files…");
    let mut file_count = 0;
    for path in &paths_to_remove {
        let full_path = PathBuf::from("/").join(path);
        if full_path.is_file() || full_path.is_symlink() {
            match fs::remove_file(&full_path) {
                Ok(_) => {
                    // Optionally print every removed file
                    // println!("   Removed: {}", full_path.display());
                    file_count += 1;
                }
                Err(e) => eprintln!("[WARNING] It was not possible to remove the file '{}': {}", full_path.display(), e),
            }
        }
    }
    println!("=> {} files removed.", file_count);

    // -- Removal: Second Pass (directories) --
    println!("=> Removing empty directories…");
    let mut dir_count = 0;
    // Iterating in reverse order to remove subdirectories first.
    for path in paths_to_remove.iter().rev() {
        let full_path = PathBuf::from("/").join(path);
        if full_path.is_dir() {
            if fs::remove_dir(&full_path).is_ok() {
                // Optionally print every removed directory
                // println!("   Removed: {}", full_path.display());
                dir_count += 1;
            }
        }
    }
    println!("=> {} directories removed.", dir_count);

    // ---- Removal: Database cleaning ----
    println!("=> Cleaning database logs…");
    fs::remove_file(&list_path)?;
    fs::remove_file(&desc_path)?;
    println!("=> Logs removed.");

    println!("\n=> Package '{}' successfully removed!", package_name);
    Ok(())
}
