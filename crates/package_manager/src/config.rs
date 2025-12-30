//! Package configuration (roast.toml).

use base64::prelude::*;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Package configuration from roast.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
    pub package: PackageInfo,
    #[serde(default)]
    pub dependencies: IndexMap<String, Dependency>,
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: IndexMap<String, Dependency>,
    #[serde(default)]
    pub features: IndexMap<String, Vec<String>>,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub registries: IndexMap<String, RegistryConfig>,
}

/// Package information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub edition: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
}

/// Dependency specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Simple(String),
    Detailed(DependencyDetails),
}

/// Detailed dependency specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyDetails {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    /// Private registry name for this dependency.
    #[serde(default)]
    pub registry: Option<String>,
}

/// Build configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub opt_level: Option<u32>,
}

/// Private registry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry URL (e.g., https://registry.mycompany.com).
    pub url: String,
    /// Authentication method.
    #[serde(default)]
    pub auth: RegistryAuth,
    /// Whether to use this registry for all dependencies by default.
    #[serde(default)]
    pub default: bool,
    /// Priority when searching for packages (lower = higher priority).
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Whether to allow publishing to this registry.
    #[serde(default)]
    pub publish: bool,
    /// Custom CA certificate path for self-signed certs.
    #[serde(default)]
    pub ca_cert: Option<String>,
    /// Whether to verify TLS certificates.
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
}

fn default_priority() -> u32 { 100 }
fn default_true() -> bool { true }

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            auth: RegistryAuth::None,
            default: false,
            priority: 100,
            publish: false,
            ca_cert: None,
            verify_ssl: true,
        }
    }
}

/// Authentication method for a registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RegistryAuth {
    /// No authentication.
    None,
    /// Token-based authentication.
    Token {
        /// The token value or environment variable name.
        token: String,
        /// If true, `token` is the name of an env var containing the token.
        #[serde(default)]
        from_env: bool,
    },
    /// Basic authentication.
    Basic {
        username: String,
        /// Password or env var name.
        password: String,
        #[serde(default)]
        password_from_env: bool,
    },
    /// OAuth2 authentication.
    OAuth2 {
        client_id: String,
        client_secret: String,
        token_url: String,
        #[serde(default)]
        scopes: Vec<String>,
    },
    /// AWS credentials (for S3-backed registries).
    Aws {
        #[serde(default)]
        profile: Option<String>,
        #[serde(default)]
        region: Option<String>,
    },
}

impl Default for RegistryAuth {
    fn default() -> Self {
        RegistryAuth::None
    }
}

impl RegistryAuth {
    /// Get the authorization header value.
    pub fn get_auth_header(&self) -> Option<String> {
        match self {
            RegistryAuth::None => None,
            RegistryAuth::Token { token, from_env } => {
                let actual_token = if *from_env {
                    std::env::var(token).ok()?
                } else {
                    token.clone()
                };
                Some(format!("Bearer {}", actual_token))
            }
            RegistryAuth::Basic { username, password, password_from_env } => {
                let actual_password = if *password_from_env {
                    std::env::var(password).ok()?
                } else {
                    password.clone()
                };
                let credentials = format!("{}:{}", username, actual_password);
                let encoded = BASE64_STANDARD.encode(credentials.as_bytes());
                Some(format!("Basic {}", encoded))
            }
            RegistryAuth::OAuth2 { .. } => {
                // OAuth2 would need token exchange first
                None
            }
            RegistryAuth::Aws { .. } => {
                // AWS would use AWS SDK for signing
                None
            }
        }
    }
}

/// Global registry configuration (from ~/.roast/config.toml).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Global registry settings.
    #[serde(default)]
    pub registries: IndexMap<String, RegistryConfig>,
    /// Default registry for publishing.
    #[serde(default)]
    pub default_publish_registry: Option<String>,
    /// Credential storage.
    #[serde(default)]
    pub credentials: IndexMap<String, CredentialEntry>,
}

/// Stored credential entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialEntry {
    pub token: String,
    /// Expiry timestamp (Unix epoch).
    pub expires_at: Option<u64>,
}

impl GlobalConfig {
    /// Load global config from default location.
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::default_path();
        if !config_path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&config_path)
    }
    
    /// Load from specific path.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        toml::from_str(&content)
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }
    
    /// Save to default location.
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(&Self::default_path())
    }
    
    /// Save to specific path.
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::Io(e.to_string()))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        std::fs::write(path, content)
            .map_err(|e| ConfigError::Io(e.to_string()))
    }
    
    /// Default config path.
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".roast")
            .join("config.toml")
    }
    
    /// Store a credential.
    pub fn store_credential(&mut self, registry: &str, token: &str, expires_at: Option<u64>) {
        self.credentials.insert(registry.to_string(), CredentialEntry {
            token: token.to_string(),
            expires_at,
        });
    }
    
    /// Get a credential.
    pub fn get_credential(&self, registry: &str) -> Option<&CredentialEntry> {
        self.credentials.get(registry).filter(|c| {
            // Check if not expired
            if let Some(expires) = c.expires_at {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                return expires > now;
            }
            true
        })
    }
}

impl PackageConfig {
    /// Loads package config from a file.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        toml::from_str(&content)
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Saves package config to a file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        std::fs::write(path, content)
            .map_err(|e| ConfigError::Io(e.to_string()))
    }
    
    /// Get registry URL for a dependency.
    pub fn get_registry_url(&self, dep: &DependencyDetails) -> Option<String> {
        if let Some(ref registry_name) = dep.registry {
            self.registries.get(registry_name).map(|r| r.url.clone())
        } else {
            None
        }
    }
}

/// Configuration error.
#[derive(Debug)]
pub enum ConfigError {
    Io(String),
    Parse(String),
    Serialize(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {}", e),
            ConfigError::Parse(e) => write!(f, "parse error: {}", e),
            ConfigError::Serialize(e) => write!(f, "serialize error: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

