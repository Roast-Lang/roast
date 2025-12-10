//! Kitchen - Project and Environment Manager for Roast
//!
//! Kitchen is the all-in-one tool for managing Roast projects:
//! - Virtual environments (like Python venv)
//! - Project scaffolding (like cargo new)
//! - Dependency management (like cargo/uv)
//! - Build system
//! - Package publishing
//! - GPU detection and configuration

pub mod config;
pub mod project;
pub mod venv;
pub mod deps;
pub mod resolver;
pub mod build;
pub mod registry;
pub mod cache;
pub mod lock;
pub mod gpu;
pub mod scripts;
pub mod workspace;
pub mod pypi;

pub use config::{KitchenConfig, ProjectConfig};
pub use project::Project;
pub use venv::VirtualEnv;
pub use deps::{Dependency, DependencySpec};
pub use resolver::Resolver;
pub use build::Builder;
pub use registry::Registry;
pub use cache::Cache;
pub use lock::Lockfile;
pub use gpu::GpuInfo;
pub use workspace::Workspace;
pub use pypi::{PythonEnv, PyPIClient, PythonPackage};

/// Kitchen version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default registry URL.
pub const DEFAULT_REGISTRY: &str = "https://registry.roast-lang.org";

/// Kitchen result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Kitchen error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Project error: {0}")]
    Project(String),
    
    #[error("Dependency error: {0}")]
    Dependency(String),
    
    #[error("Resolution error: {0}")]
    Resolution(String),
    
    #[error("Build error: {0}")]
    Build(String),
    
    #[error("Registry error: {0}")]
    Registry(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Virtual environment error: {0}")]
    VirtualEnv(String),
    
    #[error("GPU error: {0}")]
    Gpu(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("HTTP error: {0}")]
    Http(String),
    
    #[error("Semver error: {0}")]
    Semver(#[from] semver::Error),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Python error: {0}")]
    Python(String),
}

