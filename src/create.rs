use std::fs::{self, File};
use sha2::{Sha256, Digest};
use tar::Builder;
use zstd::stream::Encoder;
use std::path::Path;
use std::env;

use crate::package::Metadata;

pub fn create_package(package_name: &str, compression_level: i32) -> Result<(), Box<dyn std::error::Error>> {
    let info_dir = Path::new("info");
    let metadata_path = info_dir.join("desc.toml");

    // Checking if the 'info' directory exists
    if !info_dir.is_dir() {
        return Err(" The 'info/' directory was not found.".into());
    }

    // Checking if the file 'desc.toml' exists inside 'info/'
    if !metadata_path.exists() {
        return Err("The 'desc.toml' file was not found inside 'info/'.".into());
    }

    // -- Reading and validation of Metadata --
    println!("=> Reading and validating metadata from 'info/desc.toml'…");

    let metadata_content = fs::read_to_string(&metadata_path)
	.map_err(|e| format!("Couldn't read '{}': {}", metadata_path.display(), e))?;
    let metadata: Metadata = toml::from_str(&metadata_content)
	.map_err(|e| format!("Syntax error in '{}': {}", metadata_path.display(), e))?;

    let expected_name = format!("{}-{}-{}", metadata.pkgname, metadata.version, metadata.build);
    if package_name != expected_name {
	return Err(format!(
	    "The inserted package name ('{}') is not the same as described in desc.toml ('{}').",
	    package_name, expected_name
	).into());
    }
    println!("=> Metadata successfully validated.");

    // -- Creation of compressed package .mtz --
    let archive_name = format!("{}.mtz", package_name);
    println!("=> Creating compressed package '{}' (level {})…", archive_name, compression_level);

    let original_dir = env::current_dir()?;
    env::set_current_dir(info_dir)?;
    
    let file = File::create(original_dir.join(&archive_name))?;
    // Encode it with Zstd
    let encoder = Encoder::new(file, compression_level)?;
    // And pass it to tar::Builder
    let mut builder = Builder::new(encoder);
    builder.append_dir_all(".", ".")?;
    let encoder = builder.into_inner()?;

    // As we finish builder, we should also finish the encoder
    // let encoder = builder.into_inner()?;
    encoder.finish()?;

    // Let's go back to the original directory
    env::set_current_dir(&original_dir)?;

    println!("=> Package successfully created at '{}'.", original_dir.display());

    // -- SHA256 Checksum calculation --
    println!("=> Calculating SHA256 checksum…");
    let mut package_file = File::open(&archive_name)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut package_file, &mut hasher)?;
    let hash = hasher.finalize();

    let checksum_name = format!("{}.sha256", archive_name);
    fs::write(&checksum_name, format!("{:x} {}\n", hash, format!("{}", &archive_name)))?;

    println!("=> Checksum saved at '{}'", checksum_name);

    Ok(())
}
