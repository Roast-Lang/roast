//! Lock file support for deterministic dependency resolution.
//!
//! The lock file (roast.lock) ensures reproducible builds by recording
//! exact versions and checksums of all resolved dependencies.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// Lock file structure - serialized as TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    /// Lock file version for forward compatibility.
    pub version: u32,
    /// Map of package name to locked dependency info.
    pub packages: BTreeMap<String, LockedPackage>,
}

/// A locked package with exact version and integrity hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    /// Exact resolved version.
    pub version: String,
    /// Source of the package.
    pub source: LockedSource,
    /// Integrity checksum (SHA256).
    pub checksum: Option<String>,
    /// Direct dependencies of this package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

/// Source of a locked package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LockedSource {
    #[serde(rename = "registry")]
    Registry { registry: String },
    #[serde(rename = "git")]
    Git { url: String, rev: String },
    #[serde(rename = "path")]
    Path { path: String },
}

impl Lockfile {
    /// Current lock file format version.
    pub const CURRENT_VERSION: u32 = 1;
    
    /// Default lock file name.
    pub const FILENAME: &'static str = "roast.lock";

    /// Create a new empty lock file.
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            packages: BTreeMap::new(),
        }
    }

    /// Load lock file from path.
    pub fn load(path: &Path) -> Result<Self, LockfileError> {
        let content = fs::read_to_string(path)
            .map_err(|e| LockfileError::Io(e))?;
        let lockfile: Lockfile = toml::from_str(&content)
            .map_err(|e| LockfileError::Parse(e.to_string()))?;
        
        // Version check
        if lockfile.version > Self::CURRENT_VERSION {
            return Err(LockfileError::UnsupportedVersion(lockfile.version));
        }
        
        Ok(lockfile)
    }

    /// Load lock file from current directory if it exists.
    pub fn load_if_exists() -> Result<Option<Self>, LockfileError> {
        let path = Path::new(Self::FILENAME);
        if path.exists() {
            Ok(Some(Self::load(path)?))
        } else {
            Ok(None)
        }
    }

    /// Save lock file to path.
    pub fn save(&self, path: &Path) -> Result<(), LockfileError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| LockfileError::Serialize(e.to_string()))?;
        fs::write(path, content)
            .map_err(|e| LockfileError::Io(e))?;
        Ok(())
    }

    /// Save lock file to current directory.
    pub fn save_default(&self) -> Result<(), LockfileError> {
        self.save(Path::new(Self::FILENAME))
    }

    /// Add or update a package in the lock file.
    pub fn add_package(&mut self, name: String, pkg: LockedPackage) {
        self.packages.insert(name, pkg);
    }

    /// Get a locked package by name.
    pub fn get(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.get(name)
    }

    /// Check if lock file contains a package.
    pub fn contains(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// Number of locked packages.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Check if lock file is empty.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock file errors.
#[derive(Debug)]
pub enum LockfileError {
    Io(io::Error),
    Parse(String),
    Serialize(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for LockfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockfileError::Io(e) => write!(f, "lock file I/O error: {}", e),
            LockfileError::Parse(e) => write!(f, "lock file parse error: {}", e),
            LockfileError::Serialize(e) => write!(f, "lock file serialize error: {}", e),
            LockfileError::UnsupportedVersion(v) => {
                write!(f, "unsupported lock file version: {} (max: {})", 
                       v, Lockfile::CURRENT_VERSION)
            }
        }
    }
}

impl std::error::Error for LockfileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_roundtrip() {
        let mut lockfile = Lockfile::new();
        lockfile.add_package("example".to_string(), LockedPackage {
            version: "1.2.3".to_string(),
            source: LockedSource::Registry {
                registry: "https://registry.roast-lang.org".to_string(),
            },
            checksum: Some("sha256:abc123".to_string()),
            dependencies: vec!["dep1".to_string()],
        });

        let toml = toml::to_string_pretty(&lockfile).unwrap();
        let parsed: Lockfile = toml::from_str(&toml).unwrap();

        assert_eq!(parsed.version, Lockfile::CURRENT_VERSION);
        assert_eq!(parsed.packages.len(), 1);
        assert!(parsed.contains("example"));
    }
}
