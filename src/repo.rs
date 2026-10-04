use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use zstd::stream::Decoder;

use crate::package::{Metadata, db_root, safe_archive_path};

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct SyncPackage {
    #[serde(flatten)]
    pub metadata: Metadata,
    pub sha256: String,
    pub filename: String,
}

pub fn analyze_new_package(path: &Path) -> Result<(Metadata, String), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let decoder = Decoder::new(file)?;
    let mut archive = Archive::new(decoder);
    let mut metadata = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        safe_archive_path(&entry_path)?;
        if entry_path == Path::new("desc.toml") {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            metadata = Some(toml::from_str(&content)?);
        }
    }
    let metadata: Metadata = metadata.ok_or("desc.toml not found")?;
    metadata
        .validate()
        .map_err(|e| format!("Invalid metadata: {e}"))?;
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok((metadata, format!("{:x}", hasher.finalize())))
}

pub fn add_repo(name: &str, source: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<PathBuf> = if source.is_dir() {
        fs::read_dir(source)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("mtz"))
            .collect()
    } else if source.extension().and_then(|e| e.to_str()) == Some("mtz") {
        vec![source.to_path_buf()]
    } else {
        return Err("repository source must be a .mtz file or directory".into());
    };
    let sync = db_root().join("sync").join(name);
    fs::create_dir_all(&sync)?;
    let source = if source.is_dir() {
        source.canonicalize()?
    } else {
        source
            .parent()
            .ok_or("package has no parent directory")?
            .canonicalize()?
    };
    fs::write(sync.join(".source"), source.to_string_lossy().as_bytes())?;
    for package in files {
        let (metadata, calculated) = analyze_new_package(&package)?;
        let checksum = package.with_extension("mtz.sha256");
        let expected = fs::read_to_string(checksum)?
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        if expected != calculated {
            return Err(format!("checksum mismatch for {}", package.display()).into());
        }
        let record = SyncPackage {
            metadata: metadata.clone(),
            sha256: calculated,
            filename: package
                .file_name()
                .ok_or("invalid package filename")?
                .to_string_lossy()
                .into_owned(),
        };
        fs::write(
            sync.join(format!("{}.toml", metadata.canonical_name())),
            toml::to_string_pretty(&record)?,
        )?;
    }
    Ok(())
}

pub fn repo_source(repo_dir: &Path, filename: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(repo_dir.join(".source"))?;
    Ok(PathBuf::from(source.trim()).join(filename))
}

#[cfg(test)]
mod tests {
    use super::{SyncPackage, analyze_new_package};
    use crate::package::Metadata;
    use std::fs::{self, File};
    use tar::Builder;
    use zstd::stream::Encoder;

    fn metadata() -> Metadata {
        Metadata {
            maintainer: "tester".into(),
            pkgname: "demo".into(),
            version: "1.0".into(),
            build: "1".into(),
            license: "MIT".into(),
            desc: "demo".into(),
            url: "https://example.invalid".into(),
            deps: vec!["base>=1.0".into()],
        }
    }

    #[test]
    fn analyzes_package_metadata_and_checksum() {
        let dir = std::env::temp_dir().join(format!("matepkg-repo-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let package = dir.join("demo-1.0-1.mtz");
        let file = File::create(&package).unwrap();
        let encoder = Encoder::new(file, 1).unwrap();
        let mut builder = Builder::new(encoder);
        let content = toml::to_string(&metadata()).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_path("desc.toml").unwrap();
        header.set_size(content.len() as u64);
        header.set_cksum();
        builder.append(&header, content.as_bytes()).unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let (found, checksum) = analyze_new_package(&package).unwrap();
        assert_eq!(found.canonical_name(), "demo-1.0-1");
        assert_eq!(checksum.len(), 64);

        let record = SyncPackage {
            metadata: found,
            sha256: checksum,
            filename: "demo-1.0-1.mtz".into(),
        };
        let decoded: SyncPackage = toml::from_str(&toml::to_string(&record).unwrap()).unwrap();
        assert_eq!(decoded.filename, "demo-1.0-1.mtz");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_repository_source_file() {
        let dir = std::env::temp_dir().join(format!("matepkg-source-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".source"), "/srv/packages\n").unwrap();
        assert_eq!(
            super::repo_source(&dir, "demo-1.0-1.mtz").unwrap(),
            std::path::PathBuf::from("/srv/packages/demo-1.0-1.mtz")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_archive_without_metadata() {
        let dir = std::env::temp_dir().join(format!("matepkg-invalid-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let package = dir.join("invalid.mtz");
        let file = File::create(&package).unwrap();
        let encoder = Encoder::new(file, 1).unwrap();
        let mut builder = Builder::new(encoder);
        let content = b"not metadata";
        let mut header = tar::Header::new_gnu();
        header.set_path("README").unwrap();
        header.set_size(content.len() as u64);
        header.set_cksum();
        builder.append(&header, content.as_slice()).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        assert!(analyze_new_package(&package).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
