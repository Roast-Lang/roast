//! Configuration management for Kitchen.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use crate::{Error, Result};

/// Global Kitchen configuration (~/.kitchen/config.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitchenConfig {
    /// Default registry URL.
    #[serde(default = "default_registry")]
    pub registry: String,

    /// Cache directory.
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,

    /// Default Python interpreter for compatibility.
    pub python_path: Option<PathBuf>,

    /// Parallel downloads.
    #[serde(default = "default_parallel")]
    pub parallel_downloads: usize,

    /// Offline mode.
    #[serde(default)]
    pub offline: bool,

    /// GPU settings.
    #[serde(default)]
    pub gpu: GpuConfig,

    /// Authentication tokens.
    #[serde(default)]
    pub tokens: HashMap<String, String>,
}

fn default_registry() -> String {
    crate::DEFAULT_REGISTRY.to_string()
}

fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("kitchen")
}

fn default_parallel() -> usize {
    4
}

impl Default for KitchenConfig {
    fn default() -> Self {
        Self {
            registry: default_registry(),
            cache_dir: default_cache_dir(),
            python_path: None,
            parallel_downloads: default_parallel(),
            offline: false,
            gpu: GpuConfig::default(),
            tokens: HashMap::new(),
        }
    }
}

impl KitchenConfig {
    /// Load global configuration.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Save global configuration.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        fs::write(&config_path, content)?;

        Ok(())
    }

    /// Get config file path.
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("kitchen")
            .join("config.toml")
    }

    /// Get kitchen home directory.
    pub fn kitchen_home() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("kitchen")
    }

    /// Set authentication token for a registry.
    pub fn set_token(&mut self, registry: &str, token: &str) {
        self.tokens.insert(registry.to_string(), token.to_string());
    }

    /// Get authentication token for a registry.
    pub fn get_token(&self, registry: &str) -> Option<&str> {
        self.tokens.get(registry).map(|s| s.as_str())
    }

    /// Get authentication token securely (checks env var first).
    /// 
    /// Priority:
    /// 1. ROAST_REGISTRY_TOKEN environment variable  
    /// 2. Registry-specific env var: ROAST_REGISTRY_TOKEN_{REGISTRY_HOST}
    /// 3. Config file (with deprecation warning)
    pub fn get_token_secure(&self, registry: &str) -> Option<String> {
        // 1. Check global environment variable
        if let Ok(token) = std::env::var("ROAST_REGISTRY_TOKEN") {
            return Some(token);
        }
        
        // 2. Check registry-specific environment variable
        // Convert registry URL to env var name: https://registry.roast-lang.org -> ROAST_REGISTRY_TOKEN_REGISTRY_ROAST_LANG_ORG
        let env_key = registry
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .replace(['.', '-', '/'], "_")
            .to_uppercase();
        let specific_env = format!("ROAST_REGISTRY_TOKEN_{}", env_key);
        if let Ok(token) = std::env::var(&specific_env) {
            return Some(token);
        }
        
        // 3. Fall back to config file with deprecation warning
        if let Some(token) = self.tokens.get(registry) {
            eprintln!("⚠️  Warning: Using plaintext token from config file for {}", registry);
            eprintln!("   Consider using environment variable ROAST_REGISTRY_TOKEN instead");
            eprintln!("   Or run 'kitchen login' to securely store credentials");
            return Some(token.clone());
        }
        
        None
    }

    /// Set authentication token (stores in config for now, will support keyring later).
    pub fn set_token_secure(&mut self, registry: &str, token: &str) -> Result<()> {
        // TODO: In future, use keyring crate for secure storage
        // For now, store in config with a note about where it's stored
        self.tokens.insert(registry.to_string(), token.to_string());
        eprintln!("ℹ️  Token stored in config file. Future versions will use system keychain.");
        self.save()
    }
}

/// GPU configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuConfig {
    /// Enable GPU by default.
    #[serde(default)]
    pub enabled: bool,

    /// Preferred GPU backend.
    pub backend: Option<GpuBackend>,

    /// Preferred device index.
    pub device: Option<usize>,
}

/// GPU backend type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GpuBackend {
    Cuda,
    OpenCL,
    Metal,
    Vulkan,
    Cpu,
}

/// Project configuration (roast.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Package metadata.
    pub package: PackageConfig,

    /// Dependencies.
    #[serde(default)]
    pub dependencies: HashMap<String, DependencyConfig>,

    /// Development dependencies.
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, DependencyConfig>,

    /// Build dependencies.
    #[serde(default, rename = "build-dependencies")]
    pub build_dependencies: HashMap<String, DependencyConfig>,

    /// Optional dependencies (features).
    #[serde(default, rename = "optional-dependencies")]
    pub optional_dependencies: HashMap<String, Vec<String>>,

    /// Feature flags.
    #[serde(default)]
    pub features: HashMap<String, Vec<String>>,

    /// Default features.
    #[serde(default, rename = "default-features")]
    pub default_features: Vec<String>,

    /// Scripts.
    #[serde(default)]
    pub scripts: HashMap<String, String>,

    /// Build configuration.
    #[serde(default)]
    pub build: BuildConfig,

    /// Workspace configuration (for mono-repos).
    pub workspace: Option<WorkspaceConfig>,

    /// Profile configurations.
    #[serde(default)]
    pub profile: ProfilesConfig,
}

/// Package metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
    /// Package name.
    pub name: String,

    /// Package version.
    pub version: String,

    /// Roast edition.
    #[serde(default = "default_edition")]
    pub edition: String,

    /// Authors.
    #[serde(default)]
    pub authors: Vec<String>,

    /// Description.
    pub description: Option<String>,

    /// License.
    pub license: Option<String>,

    /// Repository URL.
    pub repository: Option<String>,

    /// Homepage URL.
    pub homepage: Option<String>,

    /// Documentation URL.
    pub documentation: Option<String>,

    /// Keywords.
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Categories.
    #[serde(default)]
    pub categories: Vec<String>,

    /// Readme file.
    pub readme: Option<String>,

    /// Entry point.
    #[serde(default = "default_entry")]
    pub entry: String,

    /// Minimum Roast version.
    #[serde(rename = "roast-version")]
    pub roast_version: Option<String>,

    /// Include patterns.
    #[serde(default)]
    pub include: Vec<String>,

    /// Exclude patterns.
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_edition() -> String {
    "2024".to_string()
}

fn default_entry() -> String {
    "src/main.roast".to_string()
}

/// Dependency configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyConfig {
    /// Simple version string.
    Simple(String),

    /// Detailed configuration.
    Detailed(DetailedDependency),
}

/// Detailed dependency configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    /// Version requirement.
    pub version: Option<String>,

    /// Git repository.
    pub git: Option<String>,

    /// Git branch.
    pub branch: Option<String>,

    /// Git tag.
    pub tag: Option<String>,

    /// Git revision.
    pub rev: Option<String>,

    /// Local path.
    pub path: Option<String>,

    /// Registry URL.
    pub registry: Option<String>,

    /// Optional dependency.
    #[serde(default)]
    pub optional: bool,

    /// Features to enable.
    #[serde(default)]
    pub features: Vec<String>,

    /// Default features.
    #[serde(default = "default_true")]
    pub default_features: bool,
}

fn default_true() -> bool {
    true
}

/// Build configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Target directory (default: "cooked").
    #[serde(default = "default_target")]
    pub target_dir: String,

    /// Build script.
    pub build_script: Option<String>,

    /// Extra include paths.
    #[serde(default)]
    pub include_paths: Vec<String>,

    /// Extra library paths.
    #[serde(default)]
    pub library_paths: Vec<String>,

    /// GPU configuration for build.
    #[serde(default)]
    pub gpu: GpuBuildConfig,
}

fn default_target() -> String {
    "cooked".to_string()
}

/// GPU build configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuBuildConfig {
    /// Enable GPU compilation.
    #[serde(default)]
    pub enabled: bool,

    /// CUDA architectures.
    #[serde(default)]
    pub cuda_archs: Vec<String>,

    /// OpenCL platforms.
    #[serde(default)]
    pub opencl_platforms: Vec<String>,
}

/// Workspace configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Member packages.
    pub members: Vec<String>,

    /// Excluded packages.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Default members.
    #[serde(default, rename = "default-members")]
    pub default_members: Vec<String>,

    /// Shared dependencies.
    #[serde(default)]
    pub dependencies: HashMap<String, DependencyConfig>,
}

/// Build profiles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfilesConfig {
    /// Debug profile.
    #[serde(default)]
    pub dev: ProfileConfig,

    /// Release profile.
    #[serde(default)]
    pub release: ProfileConfig,

    /// Test profile.
    #[serde(default)]
    pub test: ProfileConfig,

    /// Benchmark profile.
    #[serde(default)]
    pub bench: ProfileConfig,
}

/// Build profile configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Optimization level (0-3, s, z).
    #[serde(default)]
    pub opt_level: OptLevel,

    /// Enable debug info.
    #[serde(default)]
    pub debug: bool,

    /// Enable LTO.
    #[serde(default)]
    pub lto: bool,

    /// Panic strategy.
    #[serde(default)]
    pub panic: PanicStrategy,

    /// Codegen units.
    pub codegen_units: Option<u32>,

    /// Enable incremental compilation.
    #[serde(default = "default_true")]
    pub incremental: bool,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            opt_level: OptLevel::O0,
            debug: true,
            lto: false,
            panic: PanicStrategy::Unwind,
            codegen_units: None,
            incremental: true,
        }
    }
}

/// Optimization level.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum OptLevel {
    #[default]
    #[serde(rename = "0")]
    O0,
    #[serde(rename = "1")]
    O1,
    #[serde(rename = "2")]
    O2,
    #[serde(rename = "3")]
    O3,
    #[serde(rename = "s")]
    Os,
    #[serde(rename = "z")]
    Oz,
}

/// Panic strategy.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanicStrategy {
    #[default]
    Unwind,
    Abort,
}

impl ProjectConfig {
    /// Load from roast.toml.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Save to roast.toml.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Find roast.toml in current or parent directories.
    pub fn find() -> Result<PathBuf> {
        let mut current = std::env::current_dir()?;

        loop {
            let config_path = current.join("roast.toml");
            if config_path.exists() {
                return Ok(config_path);
            }

            if !current.pop() {
                return Err(Error::NotFound("roast.toml not found".to_string()));
            }
        }
    }
}
