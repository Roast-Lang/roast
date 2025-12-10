//! Package and metadata caching.

use std::path::{Path, PathBuf};
use std::fs;
use std::time::{Duration, SystemTime};
// use serde::{Deserialize, Serialize};
use crate::deps::PackageMetadata;
use crate::{Error, Result};

/// Package cache.
pub struct Cache {
    /// Cache directory.
    cache_dir: PathBuf,
    
    /// Metadata cache TTL.
    metadata_ttl: Duration,
}

impl Cache {
    /// Create a new cache.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            metadata_ttl: Duration::from_secs(3600), // 1 hour
        }
    }
    
    /// Set metadata TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.metadata_ttl = ttl;
        self
    }
    
    /// Initialize cache directories.
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(self.packages_dir())?;
        fs::create_dir_all(self.metadata_dir())?;
        fs::create_dir_all(self.git_dir())?;
        Ok(())
    }
    
    /// Get cache size in bytes.
    pub fn size(&self) -> Result<u64> {
        let mut total = 0;
        
        for entry in walkdir::WalkDir::new(&self.cache_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
        
        Ok(total)
    }
    
    /// Clear the cache.
    pub fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)?;
        }
        self.init()
    }
    
    /// Get packages directory.
    pub fn packages_dir(&self) -> PathBuf {
        self.cache_dir.join("packages")
    }
    
    /// Get metadata directory.
    pub fn metadata_dir(&self) -> PathBuf {
        self.cache_dir.join("metadata")
    }
    
    /// Get git directory.
    pub fn git_dir(&self) -> PathBuf {
        self.cache_dir.join("git")
    }
    
    /// Get cached package metadata.
    pub fn get_metadata(&self, name: &str) -> Result<Option<PackageMetadata>> {
        let path = self.metadata_dir().join(format!("{}.json", name));
        
        if !path.exists() {
            return Ok(None);
        }
        
        // Check TTL
        let modified = fs::metadata(&path)?.modified()?;
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::MAX);
        
        if age > self.metadata_ttl {
            // Expired
            return Ok(None);
        }
        
        let content = fs::read_to_string(&path)?;
        let metadata: PackageMetadata = serde_json::from_str(&content)?;
        
        Ok(Some(metadata))
    }
    
    /// Cache package metadata.
    pub fn set_metadata(&self, metadata: &PackageMetadata) -> Result<()> {
        let path = self.metadata_dir().join(format!("{}.json", metadata.name));
        
        fs::create_dir_all(self.metadata_dir())?;
        
        let content = serde_json::to_string_pretty(metadata)?;
        fs::write(&path, content)?;
        
        Ok(())
    }
    
    /// Get cached package.
    pub fn get_package(&self, name: &str, version: &str) -> Result<Option<PathBuf>> {
        let path = self.package_path(name, version);
        
        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }
    
    /// Store a package in cache.
    pub fn store_package(&self, name: &str, version: &str, data: &[u8]) -> Result<PathBuf> {
        let path = self.package_path(name, version);
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(&path, data)?;
        
        // Extract if it's a tarball
        let extract_dir = path.with_extension("");
        if !extract_dir.exists() {
            self.extract_package(&path, &extract_dir)?;
        }
        
        Ok(extract_dir)
    }
    
    /// Get package path.
    fn package_path(&self, name: &str, version: &str) -> PathBuf {
        self.packages_dir()
            .join(name)
            .join(format!("{}-{}.tar.gz", name, version))
    }
    
    /// Extract package tarball.
    fn extract_package(&self, tarball: &Path, dest: &Path) -> Result<()> {
        use flate2::read::GzDecoder;
        use tar::Archive;
        
        let file = fs::File::open(tarball)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        
        fs::create_dir_all(dest)?;
        archive.unpack(dest)?;
        
        Ok(())
    }
    
    /// Get or clone a git repository.
    pub fn get_git_repo(&self, url: &str, rev: Option<&str>) -> Result<PathBuf> {
        use sha2::{Sha256, Digest};
        
        // Hash the URL for directory name
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let hash = hex::encode(&hasher.finalize()[..8]);
        
        let repo_dir = self.git_dir().join(&hash);
        
        if !repo_dir.exists() {
            // Clone the repository
            let status = std::process::Command::new("git")
                .args(["clone", "--depth", "1", url, &repo_dir.to_string_lossy()])
                .status()?;
            
            if !status.success() {
                return Err(Error::Cache(format!(
                    "Failed to clone git repository: {}", url
                )));
            }
        }
        
        // Checkout specific revision if specified
        if let Some(rev) = rev {
            let status = std::process::Command::new("git")
                .current_dir(&repo_dir)
                .args(["checkout", rev])
                .status()?;
            
            if !status.success() {
                return Err(Error::Cache(format!(
                    "Failed to checkout revision: {}", rev
                )));
            }
        }
        
        Ok(repo_dir)
    }
    
    /// Garbage collect old cache entries.
    pub fn gc(&self, max_age: Duration) -> Result<GcResult> {
        let mut result = GcResult::default();
        let now = SystemTime::now();
        
        // Clean metadata
        if let Ok(entries) = fs::read_dir(self.metadata_dir()) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
                    if now.duration_since(modified).unwrap_or(Duration::MAX) > max_age {
                        result.freed_bytes += fs::metadata(&path)?.len();
                        fs::remove_file(&path)?;
                        result.removed_files += 1;
                    }
                }
            }
        }
        
        // Clean old packages
        // (Only clean if not in lock file - would need lock file info)
        
        Ok(result)
    }
}

/// Garbage collection result.
#[derive(Debug, Default)]
pub struct GcResult {
    /// Number of files removed.
    pub removed_files: usize,
    
    /// Bytes freed.
    pub freed_bytes: u64,
}

impl GcResult {
    /// Human-readable freed size.
    pub fn freed_size_human(&self) -> String {
        if self.freed_bytes >= 1024 * 1024 * 1024 {
            format!("{:.2} GB", self.freed_bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if self.freed_bytes >= 1024 * 1024 {
            format!("{:.2} MB", self.freed_bytes as f64 / (1024.0 * 1024.0))
        } else if self.freed_bytes >= 1024 {
            format!("{:.2} KB", self.freed_bytes as f64 / 1024.0)
        } else {
            format!("{} bytes", self.freed_bytes)
        }
    }
}

