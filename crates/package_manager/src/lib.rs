//! Package manager for Roast (roastpkg).

pub mod config;
pub mod resolver;
pub mod registry;
pub mod lockfile;

pub use config::{PackageConfig, RegistryConfig, RegistryAuth, GlobalConfig, CredentialEntry};
pub use registry::{Registry, LocalRegistry, PrivateRegistry, RegistryManager, PackageMetadata, VersionInfo, RegistryError};
pub use lockfile::{Lockfile, LockedPackage, LockedSource, LockfileError};

