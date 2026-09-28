use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod create;
mod generate;
mod install;
mod package;
mod remove;
mod upgrade;

use crate::create::create_package;
use crate::generate::generate_metadata;
use crate::install::install_package;
use crate::remove::remove_package;
use crate::upgrade::upgrade_package;

#[derive(Parser, Debug)]
#[command(
    name = "mate",
    version = "0.1.0",
    about = "Simple Linux package manager in Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Generate {
        #[arg(name = "NAME-VERSION-BUILD", required = true)]
        name_version_build: Option<String>,
    },
    Create {
        package_name: String,
        #[arg(short = 'l', long = "level", default_value_t = 3)]
        level: i32,
    },
    Install {
        #[arg(required = true)]
        packages: Vec<String>,
    },
    Remove {
        #[arg(required = true)]
        packages: Vec<String>,
    },
    Search {
        query: String,
        #[arg(short = 'o', long = "one-line")]
        one_line: bool,
    },
    List,
    Info {
        package: String,
    },
    Upgrade {
        #[arg(required = true)]
        packages: Vec<String>,
    },
}
fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { name_version_build } => {
            println!(
                "=> Generating metadata for: {:?}",
                name_version_build.clone().unwrap_or_default()
            );
            if let Err(e) = generate_metadata(name_version_build) {
                eprintln!("=> [ERROR] Error while generating metadata: {}", e);
            }
        }
        Commands::Create {
            package_name,
            level,
        } => {
            println!("=> Making package: {}", package_name);
            if !(0..=21).contains(&level) {
                eprintln!("=> [ERROR] Compression level must be a number from 0 to 21.");
                std::process::exit(1);
            }

            if let Err(e) = create_package(&package_name, level) {
                eprintln!("\n=> [ERROR] Package creation failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Install { packages } => {
            if !is_root() {
                eprintln!(
                    "\n=> [ERROR] This operation requires root privileges. Run this again with sudo or with root privileges."
                );
                std::process::exit(1);
            }
            for pkg_path_str in packages {
                println!("=> Installing package: {}", pkg_path_str);
                let pkg_path = PathBuf::from(pkg_path_str);
                if let Err(e) = install_package(&pkg_path) {
                    eprintln!("\n=> [ERROR] Package installation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Remove { packages } => {
            if !is_root() {
                eprintln!(
                    "\n=> [ERROR] This operation requires root privileges. Run this again with sudo or with root privileges."
                );
                std::process::exit(1);
            }
            for pkg_name in packages {
                println!("=> Removing package: {}", pkg_name);
                if let Err(e) = remove_package(&pkg_name) {
                    eprintln!("\n=> [ERROR] Package removal failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Search { query, one_line } => {
            if let Err(e) = search_packages(&query, one_line) {
                eprintln!("=> [ERROR] {}", e);
                std::process::exit(1);
            }
        }
        Commands::List => {
            if let Err(e) = list_packages() {
                eprintln!("=> [ERROR] {}", e);
                std::process::exit(1);
            }
        }
        Commands::Info { package } => {
            if let Err(e) = info_package(&package) {
                eprintln!("=> [ERROR] {}", e);
                std::process::exit(1);
            }
        }
        Commands::Upgrade { packages } => {
            if !is_root() {
                eprintln!(
                    "\n=> [ERROR] This operation requires root privileges. Run this again with sudo or with root privileges."
                );
                std::process::exit(1);
            }

            println!("=> Upgrading packages: {:#?}", packages);
            for pkg_name in packages {
                println!("\n=> Starting upgrade of package: {}", pkg_name);
                let pkg_path = PathBuf::from(pkg_name);
                if let Err(e) = upgrade_package(&pkg_path) {
                    eprintln!("\n=> [ERROR] Package upgrade failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
        .unwrap_or(false)
}

fn search_packages(query: &str, one_line: bool) -> Result<(), Box<dyn std::error::Error>> {
    let query = query.to_lowercase();
    for (metadata, _) in crate::package::installed_packages()? {
        let text =
            format!("{} {} {}", metadata.pkgname, metadata.desc, metadata.url).to_lowercase();
        if text.contains(&query) {
            if one_line {
                println!("{} - {}", metadata.canonical_name(), metadata.desc);
            } else {
                println!(
                    "{}\n  {}\n  {}\n",
                    metadata.canonical_name(),
                    metadata.desc,
                    metadata.url
                );
            }
        }
    }
    Ok(())
}

fn list_packages() -> Result<(), Box<dyn std::error::Error>> {
    for (metadata, _) in crate::package::installed_packages()? {
        println!("{}", metadata.canonical_name());
    }
    Ok(())
}

fn info_package(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let package = crate::package::installed_packages()?
        .into_iter()
        .find(|(metadata, _)| metadata.pkgname == name || metadata.canonical_name() == name)
        .ok_or_else(|| format!("package '{}' is not installed", name))?;
    println!("{:#?}", package.0);
    Ok(())
}
