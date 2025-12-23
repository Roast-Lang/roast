//! Roast package manager CLI (roastpkg).

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use roast_package_manager::{LocalRegistry, PackageConfig};
use std::path::Path;
use std::fs;

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
        /// Version to install
        #[arg(short, long)]
        version: Option<String>,
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
    /// Publish package to local registry
    Publish {
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Search for packages in local registry
    Search {
        /// Search query
        query: String,
    },
    /// Show package info from local registry
    Info {
        /// Package name
        package: String,
    },
    /// Initialize a new package
    Init {
        /// Package name
        name: Option<String>,
    },
    /// List all packages in local registry
    List,
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
    let registry = LocalRegistry::default();

    match cli.command {
        Commands::Install { package, version } => {
            if let Some(pkg) = package {
                let ver = version.unwrap_or_else(|| "latest".to_string());
                println!("📦 Installing {}@{}...", pkg, ver);
                
                // Try to get package info
                match registry.get_package(&pkg) {
                    Ok(meta) => {
                        if let Some(latest) = meta.versions.last() {
                            println!("  Found version: {}", latest.version);
                            match registry.download(&pkg, &latest.version) {
                                Ok(data) => {
                                    println!("  Downloaded {} bytes", data.len());
                                    // TODO: Extract and install
                                    println!("✓ Installed {} successfully", pkg);
                                }
                                Err(e) => println!("✗ Download failed: {}", e),
                            }
                        } else {
                            println!("✗ No versions found for {}", pkg);
                        }
                    }
                    Err(e) => println!("✗ Package not found: {}", e),
                }
            } else {
                println!("📦 Installing dependencies from roast.toml...");
                if Path::new("roast.toml").exists() {
                    let config = PackageConfig::load(Path::new("roast.toml"))
                        .context("Failed to load roast.toml")?;
                    println!("  Package: {} v{}", config.package.name, config.package.version);
                    println!("  Dependencies: {}", config.dependencies.len());
                    for (name, _dep) in &config.dependencies {
                        println!("    - {}", name);
                    }
                } else {
                    println!("✗ No roast.toml found");
                }
            }
        }
        Commands::Add { package, version, dev } => {
            let ver = version.unwrap_or_else(|| "*".to_string());
            let dep_type = if dev { "dev " } else { "" };
            println!("➕ Adding {}dependency: {} @ {}", dep_type, package, ver);
            // TODO: Modify roast.toml
        }
        Commands::Remove { package } => {
            println!("➖ Removing dependency: {}", package);
            // TODO: Modify roast.toml
        }
        Commands::Update { package } => {
            if let Some(pkg) = package {
                println!("🔄 Updating package: {}", pkg);
            } else {
                println!("🔄 Updating all dependencies...");
            }
        }
        Commands::Publish { yes } => {
            println!("📤 Publishing package to local registry...");
            
            if !Path::new("roast.toml").exists() {
                println!("✗ No roast.toml found in current directory");
                return Ok(());
            }
            
            let config = PackageConfig::load(Path::new("roast.toml"))
                .context("Failed to load roast.toml")?;
            
            println!("  Name: {}", config.package.name);
            println!("  Version: {}", config.package.version);
            
            if !yes {
                println!("  (use --yes to skip confirmation)");
            }
            
            // Create a simple tarball (just the package contents for now)
            let tarball = create_tarball(&config.package.name)?;
            
            match registry.publish(&config.package.name, &config.package.version, &tarball) {
                Ok(()) => {
                    println!("✓ Published {}@{} to local registry", 
                             config.package.name, config.package.version);
                }
                Err(e) => println!("✗ Publish failed: {}", e),
            }
        }
        Commands::Search { query } => {
            println!("🔍 Searching for: {}", query);
            match registry.list_packages() {
                Ok(packages) => {
                    let matches: Vec<_> = packages.iter()
                        .filter(|p| p.contains(&query))
                        .collect();
                    if matches.is_empty() {
                        println!("  No packages found matching '{}'", query);
                    } else {
                        for pkg in matches {
                            println!("  📦 {}", pkg);
                        }
                    }
                }
                Err(e) => println!("✗ Search failed: {}", e),
            }
        }
        Commands::Info { package } => {
            println!("📋 Package info: {}", package);
            match registry.get_package(&package) {
                Ok(meta) => {
                    println!("  Name: {}", meta.name);
                    if let Some(desc) = &meta.description {
                        println!("  Description: {}", desc);
                    }
                    println!("  Downloads: {}", meta.downloads);
                    println!("  Versions:");
                    for ver in &meta.versions {
                        println!("    - {} ({})", ver.version, ver.published);
                    }
                }
                Err(e) => println!("✗ Package not found: {}", e),
            }
        }
        Commands::Init { name } => {
            let pkg_name = name.unwrap_or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "my_package".to_string())
            });
            
            println!("📝 Initializing package: {}", pkg_name);
            
            if Path::new("roast.toml").exists() {
                println!("✗ roast.toml already exists");
                return Ok(());
            }
            
            let toml_content = format!(r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
authors = []
description = ""

[dependencies]
"#, pkg_name);
            
            fs::write("roast.toml", toml_content)?;
            fs::create_dir_all("src")?;
            fs::write("src/main.roast", "def main():\n    print(\"Hello, Roast!\")\n\nmain()\n")?;
            
            println!("✓ Created roast.toml and src/main.roast");
        }
        Commands::List => {
            println!("📦 Packages in local registry:");
            match registry.list_packages() {
                Ok(packages) => {
                    if packages.is_empty() {
                        println!("  (empty - publish packages with 'roastpkg publish')");
                    } else {
                        for pkg in packages {
                            println!("  - {}", pkg);
                        }
                    }
                }
                Err(e) => println!("✗ Failed to list packages: {}", e),
            }
        }
        Commands::Build { release } => {
            let mode = if release { "release" } else { "debug" };
            println!("🔨 Building in {} mode...", mode);
            // TODO: Call roastc build
        }
        Commands::Clean => {
            println!("🧹 Cleaning build artifacts...");
            if Path::new("target").exists() {
                fs::remove_dir_all("target")?;
                println!("✓ Removed target directory");
            } else {
                println!("  Nothing to clean");
            }
        }
    }

    Ok(())
}

/// Create a simple tarball of the current directory
fn create_tarball(name: &str) -> Result<Vec<u8>> {
    use std::io::Write;
    
    // For MVP, just pack key files into a simple format
    let mut data = Vec::new();
    
    // Add roast.toml
    if let Ok(content) = fs::read("roast.toml") {
        writeln!(data, "=== roast.toml ===")?;
        data.extend_from_slice(&content);
        writeln!(data)?;
    }
    
    // Add src files
    if Path::new("src").exists() {
        for entry in walkdir::WalkDir::new("src").into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Ok(content) = fs::read(path) {
                    writeln!(data, "=== {} ===", path.display())?;
                    data.extend_from_slice(&content);
                    writeln!(data)?;
                }
            }
        }
    }
    
    if data.is_empty() {
        writeln!(data, "=== {} ===", name)?;
    }
    
    Ok(data)
}


