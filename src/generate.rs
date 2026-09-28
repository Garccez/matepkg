use std::fs;
use std::io::Write;

pub fn generate_metadata(name_version_build: Option<String>) -> std::io::Result<()> {
    let binding = name_version_build.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "use NAME-VERSION-BUILD")
    })?;
    let mut parts = binding.rsplitn(3, '-');
    let build = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if name.is_empty() || version.is_empty() || build.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "use NAME-VERSION-BUILD",
        ));
    }

    let content = format!(
        r#"# Metadata file for Mate packages.
maintainer = ""
pkgname = "{}"
version = "{}"
build = "{}"
license = ""
desc = ""
url = ""
# Package dependencies (optional)
deps = []"#,
        name, version, build
    );

    fs::create_dir_all("info")?;
    let mut file = fs::File::create("info/desc.toml")?;
    file.write_all(content.as_bytes())?;

    println!("=> 'info/desc.toml' successfully generated!");
    Ok(())
}
