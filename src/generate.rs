use std::fs;
use std::io::Write;

pub fn generate_metadata(name_version_build: Option<String>) -> std::io::Result<()> {
    let binding = name_version_build.expect("Either a name, version or build was not provided.");
    let parts: Vec<&str> = binding.split('-').collect();
    // Logic to separate name, version and build
    let (name, version, build) = (parts.get(0).unwrap_or(&"").to_string(), parts.get(1).unwrap_or(&"").to_string(), parts.get(2).unwrap_or(&"").to_string());

    let content = format!(r#"# Metadata file for Mate packages.
maintainer = ""
pkgname = "{}"
version = "{}"
build = "{}"
license = ""
desc = ""
url = ""
# Package dependencies (optional)
deps = []"#, name, version, build);

    fs::create_dir_all("info")?;
    let mut file = fs::File::create("info/desc.toml")?;
    file.write_all(content.as_bytes())?;

    println!("=> 'info/desc.toml' successfully generated!");
    Ok(())
}
