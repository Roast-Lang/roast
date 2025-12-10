//! Package registry client.

use serde::{Deserialize, Serialize};

/// Package registry client.
pub struct Registry {
    base_url: String,
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

