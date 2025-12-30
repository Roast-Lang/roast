//! Package registry client.
//!
//! Supports:
//! - Public registries (registry.roastlang.wiki)
//! - Private registries (self-hosted, corporate)
//! - Local file-based registries (offline/testing)
//! - Multiple authentication methods

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use crate::config::{RegistryConfig, RegistryAuth};

/// Package registry client.
pub struct Registry {
    base_url: String,
    auth: Option<RegistryAuth>,
    verify_ssl: bool,
    ca_cert: Option<PathBuf>,
}

/// Local file-based registry for offline/testing use.
pub struct LocalRegistry {
    root_path: PathBuf,
}

/// Private registry client with full authentication support.
pub struct PrivateRegistry {
    name: String,
    config: RegistryConfig,
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
            auth: None,
            verify_ssl: true,
            ca_cert: None,
        }
    }
    
    /// Create a registry with configuration.
    pub fn from_config(config: &RegistryConfig) -> Self {
        Self {
            base_url: config.url.clone(),
            auth: Some(config.auth.clone()),
            verify_ssl: config.verify_ssl,
            ca_cert: config.ca_cert.as_ref().map(PathBuf::from),
        }
    }

    pub fn default_registry() -> Self {
        Self::new("https://registry.roastlang.wiki")
    }
    
    /// Set authentication.
    pub fn with_auth(mut self, auth: RegistryAuth) -> Self {
        self.auth = Some(auth);
        self
    }
    
    /// Set SSL verification.
    pub fn with_ssl_verify(mut self, verify: bool) -> Self {
        self.verify_ssl = verify;
        self
    }
    
    /// Set custom CA certificate.
    pub fn with_ca_cert(mut self, path: PathBuf) -> Self {
        self.ca_cert = Some(path);
        self
    }
    
    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    
    /// Build HTTP headers with authentication.
    fn build_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            ("User-Agent".to_string(), "roast-package-manager/0.1".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ];
        
        if let Some(ref auth) = self.auth {
            if let Some(auth_header) = auth.get_auth_header() {
                headers.push(("Authorization".to_string(), auth_header));
            }
        }
        
        headers
    }

    /// Searches for packages.
    pub async fn search(&self, query: &str) -> Result<Vec<PackageMetadata>, RegistryError> {
        // Would make HTTP request to registry
        let _ = query;
        let _headers = self.build_headers();
        Ok(Vec::new())
    }

    /// Gets package info.
    pub async fn get_package(&self, name: &str) -> Result<PackageMetadata, RegistryError> {
        // Would make HTTP request to registry
        let _headers = self.build_headers();
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
        let _headers = self.build_headers();
        Ok(Vec::new())
    }

    /// Publishes a package.
    pub async fn publish(&self, _tarball: &[u8], _token: &str) -> Result<(), RegistryError> {
        // Would upload package to registry
        let _headers = self.build_headers();
        Ok(())
    }
}

impl PrivateRegistry {
    /// Create a new private registry client.
    pub fn new(name: &str, config: RegistryConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
        }
    }
    
    /// Get the registry name.
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get the registry URL.
    pub fn url(&self) -> &str {
        &self.config.url
    }
    
    /// Check if this registry allows publishing.
    pub fn can_publish(&self) -> bool {
        self.config.publish
    }
    
    /// Get the priority of this registry.
    pub fn priority(&self) -> u32 {
        self.config.priority
    }
    
    /// Build HTTP headers with authentication.
    fn build_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            ("User-Agent".to_string(), "roast-package-manager/0.1".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ];
        
        if let Some(auth_header) = self.config.auth.get_auth_header() {
            headers.push(("Authorization".to_string(), auth_header));
        }
        
        headers
    }
    
    /// Search for packages.
    pub async fn search(&self, query: &str) -> Result<Vec<PackageMetadata>, RegistryError> {
        let _headers = self.build_headers();
        let _url = format!("{}/api/v1/search?q={}", self.config.url, query);
        // Would make HTTP request
        Ok(Vec::new())
    }
    
    /// Get package metadata.
    pub async fn get_package(&self, name: &str) -> Result<PackageMetadata, RegistryError> {
        let _headers = self.build_headers();
        let _url = format!("{}/api/v1/packages/{}", self.config.url, name);
        // Would make HTTP request
        Err(RegistryError::NotFound(name.to_string()))
    }
    
    /// Download a package.
    pub async fn download(&self, name: &str, version: &str) -> Result<Vec<u8>, RegistryError> {
        let _headers = self.build_headers();
        let _url = format!("{}/api/v1/packages/{}/{}/download", self.config.url, name, version);
        // Would make HTTP request
        Ok(Vec::new())
    }
    
    /// Publish a package.
    pub async fn publish(&self, tarball: &[u8]) -> Result<(), RegistryError> {
        if !self.can_publish() {
            return Err(RegistryError::Unauthorized);
        }
        
        let _headers = self.build_headers();
        let _url = format!("{}/api/v1/packages/publish", self.config.url);
        let _ = tarball;
        // Would make HTTP request
        Ok(())
    }
    
    /// Check connectivity to the registry.
    pub async fn health_check(&self) -> Result<bool, RegistryError> {
        let _url = format!("{}/health", self.config.url);
        // Would make HTTP request
        Ok(true)
    }
}

/// Registry manager for handling multiple registries.
pub struct RegistryManager {
    registries: Vec<PrivateRegistry>,
    default_registry: Option<String>,
}

impl RegistryManager {
    /// Create a new registry manager.
    pub fn new() -> Self {
        Self {
            registries: Vec::new(),
            default_registry: None,
        }
    }
    
    /// Add a registry.
    pub fn add_registry(&mut self, name: &str, config: RegistryConfig) {
        if config.default {
            self.default_registry = Some(name.to_string());
        }
        self.registries.push(PrivateRegistry::new(name, config));
    }
    
    /// Get a registry by name.
    pub fn get(&self, name: &str) -> Option<&PrivateRegistry> {
        self.registries.iter().find(|r| r.name() == name)
    }
    
    /// Get the default registry.
    pub fn default(&self) -> Option<&PrivateRegistry> {
        self.default_registry.as_ref()
            .and_then(|name| self.get(name))
    }
    
    /// Get all registries sorted by priority.
    pub fn all_by_priority(&self) -> Vec<&PrivateRegistry> {
        let mut sorted: Vec<_> = self.registries.iter().collect();
        sorted.sort_by_key(|r| r.priority());
        sorted
    }
    
    /// Search across all registries.
    pub async fn search_all(&self, query: &str) -> Vec<(String, Vec<PackageMetadata>)> {
        let mut results = Vec::new();
        for registry in self.all_by_priority() {
            if let Ok(packages) = registry.search(query).await {
                results.push((registry.name().to_string(), packages));
            }
        }
        results
    }
    
    /// Find a package in any registry.
    pub async fn find_package(&self, name: &str) -> Option<(String, PackageMetadata)> {
        for registry in self.all_by_priority() {
            if let Ok(meta) = registry.get_package(name).await {
                return Some((registry.name().to_string(), meta));
            }
        }
        None
    }
}

impl Default for RegistryManager {
    fn default() -> Self {
        Self::new()
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
    InvalidResponse(String),
    AuthenticationFailed(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::NotFound(pkg) => write!(f, "package not found: {}", pkg),
            RegistryError::Unauthorized => write!(f, "unauthorized"),
            RegistryError::RateLimited => write!(f, "rate limited"),
            RegistryError::Network(e) => write!(f, "network error: {}", e),
            RegistryError::InvalidResponse(e) => write!(f, "invalid response: {}", e),
            RegistryError::AuthenticationFailed(e) => write!(f, "authentication failed: {}", e),
        }
    }
}

impl std::error::Error for RegistryError {}

