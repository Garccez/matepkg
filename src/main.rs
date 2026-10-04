use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod create;
mod fetch;
mod generate;
mod install;
mod package;
mod remove;
mod repo;
mod resolve;
mod upgrade;

use crate::create::create_package;
use crate::fetch::fetch;
use crate::generate::generate_metadata;
use crate::install::install_package;
use crate::remove::remove_package;
use crate::repo::add_repo;
use crate::resolve::resolve;
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
    RepoAdd {
        repo_name: String,
        path: PathBuf,
    },
    Remove {
        #[arg(required = true)]
        packages: Vec<String>,
    },
    Search {
        query: String,
        #[arg(short = 'o', long = "one-line")]
        one_line: bool,
        #[arg(short = 'i', long = "installed")]
        installed: bool,
    },
    List {
        #[arg(long)]
        available: bool,
    },
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
            if requires_root() {
                eprintln!(
                    "\n=> [ERROR] This operation requires root privileges. Run this again with sudo or with root privileges."
                );
                std::process::exit(1);
            }
            let mut transaction_packages = Vec::new();
            for pkg_path_str in packages {
                println!("=> Installing package: {}", pkg_path_str);
                let pkg_path = PathBuf::from(&pkg_path_str);
                let direct = pkg_path.extension().and_then(|e| e.to_str()) == Some("mtz")
                    || pkg_path.is_file()
                    || PathBuf::from(format!("{}.mtz", pkg_path.display())).is_file();
                if direct {
                    match install_package(&pkg_path) {
                        Ok(name) => transaction_packages.push(name),
                        Err(e) => {
                            report_transaction_failure(e, &transaction_packages);
                            std::process::exit(1);
                        }
                    }
                } else {
                    match resolve(&pkg_path_str).and_then(|packages| {
                        for package in packages {
                            let fetched = fetch(&package)?;
                            transaction_packages.push(install_package(&fetched)?);
                        }
                        Ok::<(), Box<dyn std::error::Error>>(())
                    }) {
                        Ok(()) => {}
                        Err(e) => {
                            report_transaction_failure(e, &transaction_packages);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        Commands::RepoAdd { repo_name, path } => {
            if let Err(e) = add_repo(&repo_name, &path) {
                eprintln!("=> [ERROR] Repository sync failed: {}", e);
                std::process::exit(1);
            }
            println!("=> Repository '{}' synchronized.", repo_name);
        }
        Commands::Remove { packages } => {
            if requires_root() {
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
        Commands::Search {
            query,
            one_line,
            installed,
        } => {
            if let Err(e) = search_packages(&query, one_line, installed) {
                eprintln!("=> [ERROR] {}", e);
                std::process::exit(1);
            }
        }
        Commands::List { available } => {
            if let Err(e) = list_packages(available) {
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
            if requires_root() {
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

fn requires_root() -> bool {
    crate::package::install_root() == Path::new("/") && !is_root()
}

fn report_transaction_failure(error: Box<dyn std::error::Error>, installed: &[String]) {
    eprintln!("\n=> [ERROR] Package installation failed: {}", error);
    if installed.is_empty() {
        eprintln!("=> No previous package from this transaction required rollback.");
        return;
    }
    eprintln!("=> Rolling back packages installed by this transaction...");
    let mut failed = Vec::new();
    for package in installed.iter().rev() {
        if let Err(error) = remove_package(package) {
            failed.push(format!("{} ({})", package, error));
        }
    }
    if failed.is_empty() {
        eprintln!("=> Rollback completed for files and database records.");
    } else {
        eprintln!(
            "=> ROLLBACK INCOMPLETE. Packages with unknown state: {}",
            failed.join(", ")
        );
    }
    eprintln!(
        "=> Hooks from the failed package may have had partial side effects; review manually."
    );
}

fn search_packages(
    query: &str,
    one_line: bool,
    installed_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let query = query.to_lowercase();
    let installed = crate::package::installed_packages()?;
    let mut results = installed
        .iter()
        .map(|(m, _)| (m.clone(), true))
        .collect::<Vec<_>>();
    if !installed_only {
        for (metadata, _) in crate::package::sync_packages(None)? {
            if !installed
                .iter()
                .any(|(i, _)| i.pkgname == metadata.pkgname && i.version == metadata.version)
            {
                results.push((metadata, false));
            }
        }
    }
    for (metadata, is_installed) in results {
        let text =
            format!("{} {} {}", metadata.pkgname, metadata.desc, metadata.url).to_lowercase();
        if text.contains(&query) {
            if one_line {
                println!(
                    "[{}] {} - {}",
                    if is_installed {
                        "installed"
                    } else {
                        "available"
                    },
                    metadata.canonical_name(),
                    metadata.desc
                );
            } else {
                println!(
                    "[{}] {}\n  {}\n  {}\n",
                    if is_installed {
                        "installed"
                    } else {
                        "available"
                    },
                    metadata.canonical_name(),
                    metadata.desc,
                    metadata.url
                );
            }
        }
    }
    Ok(())
}

fn list_packages(available: bool) -> Result<(), Box<dyn std::error::Error>> {
    let packages = if available {
        crate::package::sync_packages(None)?
    } else {
        crate::package::installed_packages()?
    };
    for (metadata, _) in packages {
        println!("{}", metadata.canonical_name());
    }
    Ok(())
}

fn info_package(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some((metadata, _)) = crate::package::installed_packages()?
        .into_iter()
        .find(|(metadata, _)| metadata.pkgname == name || metadata.canonical_name() == name)
    {
        println!("[installed]\n{:#?}", metadata);
        return Ok(());
    }
    let package = crate::package::sync_packages(None)?
        .into_iter()
        .find(|(metadata, _)| metadata.pkgname == name || metadata.canonical_name() == name)
        .ok_or_else(|| format!("package '{}' was not found", name))?;
    println!("[available]\n{:#?}", package.0);
    Ok(())
}
