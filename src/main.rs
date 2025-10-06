use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod package;
mod generate;
mod create;
mod install;
mod remove;

use crate::generate::generate_metadata;
use crate::create::create_package;
use crate::install::install_package;
use crate::remove::remove_package;

#[derive(Parser, Debug)]
#[command(name = "mate", version = "0.1.0", about = "Simple Linux package manager in Rust")]
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
    Upgrade {
	#[arg(required = true)]
	packages: Vec<String>,
    },
}
fn main() {
    let cli = Cli::parse();

    match cli.command {
	Commands::Generate { name_version_build } => {
	    println!("=> Generating metadata for: {:?}", name_version_build.clone().unwrap_or_default());
	    if let Err(e) = generate_metadata(name_version_build) {
		eprintln!("=> [ERROR] Error while generating metadata: {}", e);
	    }
	}
	Commands::Create { package_name, level } => {
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
	    if std::env::var("USER").unwrap_or_default() != "root" {
		eprintln!("\n=> [ERROR] This operation requires root privileges. Run this again with sudo or with root privileges.");
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
	    if std::env::var("USER").unwrap_or_default() != "root" {
		eprintln!("\n=> [ERROR] This operation requires root privileges. Run this again with sudo or with root privileges.");
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
            println!("Buscando por '{}', one-line: {}", query, one_line);
            // TODO: Chamar a função de busca
        }
        Commands::Upgrade { packages } => {
            println!("Atualizando pacotes: {:?}", packages);
             // TODO: Chamar a função de upgrade
        }
    }
}
