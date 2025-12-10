//! Roast package manager CLI (roastpkg).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "roastpkg")]
#[command(about = "Roast package manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install dependencies
    Install {
        /// Package to install (optional, defaults to roast.toml deps)
        package: Option<String>,
    },
    /// Add a dependency
    Add {
        /// Package name
        package: String,
        /// Version requirement
        #[arg(short, long)]
        version: Option<String>,
        /// Add as dev dependency
        #[arg(long)]
        dev: bool,
    },
    /// Remove a dependency
    Remove {
        /// Package name
        package: String,
    },
    /// Update dependencies
    Update {
        /// Specific package to update
        package: Option<String>,
    },
    /// Publish package to registry
    Publish {
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Search for packages
    Search {
        /// Search query
        query: String,
    },
    /// Show package info
    Info {
        /// Package name
        package: String,
    },
    /// Initialize a new package
    Init {
        /// Package name
        name: Option<String>,
    },
    /// Build the package
    Build {
        /// Release mode
        #[arg(long)]
        release: bool,
    },
    /// Clean build artifacts
    Clean,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { package } => {
            if let Some(pkg) = package {
                println!("Installing package: {}", pkg);
            } else {
                println!("Installing dependencies from roast.toml...");
            }
        }
        Commands::Add { package, version, dev } => {
            let ver = version.unwrap_or_else(|| "*".to_string());
            let dep_type = if dev { "dev " } else { "" };
            println!("Adding {}dependency: {} @ {}", dep_type, package, ver);
        }
        Commands::Remove { package } => {
            println!("Removing dependency: {}", package);
        }
        Commands::Update { package } => {
            if let Some(pkg) = package {
                println!("Updating package: {}", pkg);
            } else {
                println!("Updating all dependencies...");
            }
        }
        Commands::Publish { yes } => {
            if !yes {
                println!("Publishing package (would confirm)...");
            } else {
                println!("Publishing package...");
            }
        }
        Commands::Search { query } => {
            println!("Searching for: {}", query);
        }
        Commands::Info { package } => {
            println!("Package info for: {}", package);
        }
        Commands::Init { name } => {
            let pkg_name = name.unwrap_or_else(|| "my_package".to_string());
            println!("Initializing package: {}", pkg_name);
        }
        Commands::Build { release } => {
            let mode = if release { "release" } else { "debug" };
            println!("Building in {} mode...", mode);
        }
        Commands::Clean => {
            println!("Cleaning build artifacts...");
        }
    }

    Ok(())
}

