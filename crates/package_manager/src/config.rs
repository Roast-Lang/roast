//! Package configuration (roast.toml).

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
}

/// Build configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub opt_level: Option<u32>,
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

