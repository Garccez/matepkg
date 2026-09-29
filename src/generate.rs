use std::fs;
use std::io::Write;

fn parse_name_version_build(value: &str) -> Option<(&str, &str, &str)> {
    let mut parts = value.rsplitn(3, '-');
    let build = parts.next()?;
    let version = parts.next()?;
    let name = parts.next()?;
    if name.is_empty() || version.is_empty() || build.is_empty() {
        None
    } else {
        Some((name, version, build))
    }
}

pub fn generate_metadata(name_version_build: Option<String>) -> std::io::Result<()> {
    let binding = name_version_build.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "use NAME-VERSION-BUILD")
    })?;
    let (name, version, build) = if let Some(parts) = parse_name_version_build(&binding) {
        parts
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "use NAME-VERSION-BUILD",
        ));
    };

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

#[cfg(test)]
mod tests {
    use super::parse_name_version_build;

    #[test]
    fn parses_package_names_containing_dashes() {
        assert_eq!(
            parse_name_version_build("my-tool-1.2.3-4"),
            Some(("my-tool", "1.2.3", "4"))
        );
    }

    #[test]
    fn rejects_incomplete_package_names() {
        assert_eq!(parse_name_version_build("my-tool"), None);
        assert_eq!(parse_name_version_build("my-tool-1.0-"), None);
    }
}
