use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Metadata {
    pub maintainer: String,
    pub pkgname: String,
    pub version: String,
    pub build: String,
    pub license: String,
    pub desc: String,
    pub url: String,
    #[serde(default)]
    pub deps: Vec<String>,
}
