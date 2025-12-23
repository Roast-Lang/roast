//! Package registry client.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

/// Package registry client.
pub struct Registry {
    base_url: String,
}

/// Local file-based registry for offline/testing use.
pub struct LocalRegistry {
    root_path: PathBuf,
}

/// Package metadata from registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub description: Option<String>,
    pub versions: Vec<VersionInfo>,
    pub downloads: u64,
}

/// Version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub published: String,
    pub checksum: String,
}

impl Registry {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
        }
    }

    pub fn default_registry() -> Self {
        Self::new("https://registry.roast-lang.org")
    }

    /// Searches for packages.
    pub async fn search(&self, query: &str) -> Result<Vec<PackageMetadata>, RegistryError> {
        // Would make HTTP request to registry
        let _ = query;
        Ok(Vec::new())
    }

    /// Gets package info.
    pub async fn get_package(&self, name: &str) -> Result<PackageMetadata, RegistryError> {
        // Would make HTTP request to registry
        Err(RegistryError::NotFound(name.to_string()))
    }

    /// Downloads a package version.
    pub async fn download(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<u8>, RegistryError> {
        // Would download package tarball
        let _ = (name, version);
        Ok(Vec::new())
    }

    /// Publishes a package.
    pub async fn publish(&self, _tarball: &[u8], _token: &str) -> Result<(), RegistryError> {
        // Would upload package to registry
        Ok(())
    }
}

impl LocalRegistry {
    /// Create a new local registry at the given path.
    pub fn new(root_path: &Path) -> Self {
        Self {
            root_path: root_path.to_path_buf(),
        }
    }

    /// Default local registry path (~/.roast/registry).
    pub fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::new(&home.join(".roast").join("registry"))
    }

    /// Initialize the registry directory structure.
    pub fn init(&self) -> Result<(), RegistryError> {
        fs::create_dir_all(&self.root_path)
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        fs::create_dir_all(self.root_path.join("packages"))
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        fs::create_dir_all(self.root_path.join("index"))
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        Ok(())
    }

    /// List all packages in the local registry.
    pub fn list_packages(&self) -> Result<Vec<String>, RegistryError> {
        let index_path = self.root_path.join("index");
        if !index_path.exists() {
            return Ok(Vec::new());
        }
        
        let mut packages = Vec::new();
        for entry in fs::read_dir(&index_path)
            .map_err(|e| RegistryError::Network(e.to_string()))? 
        {
            if let Ok(entry) = entry {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        packages.push(name.trim_end_matches(".json").to_string());
                    }
                }
            }
        }
        Ok(packages)
    }

    /// Get package metadata.
    pub fn get_package(&self, name: &str) -> Result<PackageMetadata, RegistryError> {
        let index_file = self.root_path.join("index").join(format!("{}.json", name));
        if !index_file.exists() {
            return Err(RegistryError::NotFound(name.to_string()));
        }
        
        let content = fs::read_to_string(&index_file)
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        serde_json::from_str(&content)
            .map_err(|e| RegistryError::Network(e.to_string()))
    }

    /// Publish a package to the local registry.
    pub fn publish(&self, name: &str, version: &str, tarball: &[u8]) -> Result<(), RegistryError> {
        self.init()?;
        
        // Save tarball
        let pkg_dir = self.root_path.join("packages").join(name);
        fs::create_dir_all(&pkg_dir)
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        let tarball_path = pkg_dir.join(format!("{}-{}.tar.gz", name, version));
        fs::write(&tarball_path, tarball)
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        
        // Update index
        let index_file = self.root_path.join("index").join(format!("{}.json", name));
        let mut metadata = if index_file.exists() {
            let content = fs::read_to_string(&index_file)
                .map_err(|e| RegistryError::Network(e.to_string()))?;
            serde_json::from_str::<PackageMetadata>(&content)
                .unwrap_or_else(|_| PackageMetadata {
                    name: name.to_string(),
                    description: None,
                    versions: Vec::new(),
                    downloads: 0,
                })
        } else {
            PackageMetadata {
                name: name.to_string(),
                description: None,
                versions: Vec::new(),
                downloads: 0,
            }
        };
        
        // Add new version with SHA256 checksum (secure, replaces MD5)
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(tarball);
        let checksum = hex::encode(hasher.finalize());
        metadata.versions.push(VersionInfo {
            version: version.to_string(),
            published: chrono::Utc::now().to_rfc3339(),
            checksum,
        });
        
        // Save metadata
        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        fs::write(&index_file, json)
            .map_err(|e| RegistryError::Network(e.to_string()))?;
        
        Ok(())
    }

    /// Download a package version.
    pub fn download(&self, name: &str, version: &str) -> Result<Vec<u8>, RegistryError> {
        let tarball_path = self.root_path
            .join("packages")
            .join(name)
            .join(format!("{}-{}.tar.gz", name, version));
        
        if !tarball_path.exists() {
            return Err(RegistryError::NotFound(format!("{}@{}", name, version)));
        }
        
        fs::read(&tarball_path)
            .map_err(|e| RegistryError::Network(e.to_string()))
    }
}

/// Registry error.
#[derive(Debug)]
pub enum RegistryError {
    NotFound(String),
    Unauthorized,
    RateLimited,
    Network(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::NotFound(pkg) => write!(f, "package not found: {}", pkg),
            RegistryError::Unauthorized => write!(f, "unauthorized"),
            RegistryError::RateLimited => write!(f, "rate limited"),
            RegistryError::Network(e) => write!(f, "network error: {}", e),
        }
    }
}

impl std::error::Error for RegistryError {}

