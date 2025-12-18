//! Incremental build support for Roast.
//!
//! Tracks file modifications and only recompiles changed files.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

/// Build cache for incremental compilation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildCache {
    /// Map of source file path to metadata.
    pub files: HashMap<PathBuf, FileEntry>,
    /// Last build timestamp.
    pub last_build: Option<u64>,
    /// Build configuration hash.
    pub config_hash: Option<u64>,
}

/// Cached file entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// File modification time (Unix timestamp).
    pub mtime: u64,
    /// File size in bytes.
    pub size: u64,
    /// Content hash (for more precise change detection).
    pub content_hash: Option<u64>,
    /// Compiled output path.
    pub output_path: Option<PathBuf>,
    /// Dependencies (imports).
    pub dependencies: Vec<PathBuf>,
}

impl BuildCache {
    /// Load cache from file.
    pub fn load(cache_path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(cache_path)?;
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Save cache to file.
    pub fn save(&self, cache_path: &Path) -> io::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(cache_path, content)
    }

    /// Check if a file needs recompilation.
    pub fn needs_rebuild(&self, path: &Path) -> bool {
        let entry = match self.files.get(path) {
            Some(e) => e,
            None => return true, // New file
        };

        // Check if file has changed
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return true, // File doesn't exist
        };

        let current_mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let current_size = metadata.len();

        // Changed if mtime or size differs
        if current_mtime != entry.mtime || current_size != entry.size {
            return true;
        }

        // Check if output still exists
        if let Some(ref output) = entry.output_path {
            if !output.exists() {
                return true;
            }
        }

        // Check dependencies
        for dep in &entry.dependencies {
            if self.needs_rebuild(dep) {
                return true;
            }
        }

        false
    }

    /// Update entry after successful compilation.
    pub fn update(&mut self, path: &Path, output: Option<PathBuf>, deps: Vec<PathBuf>) -> io::Result<()> {
        let metadata = fs::metadata(path)?;

        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.files.insert(path.to_path_buf(), FileEntry {
            mtime,
            size: metadata.len(),
            content_hash: None, // TODO: compute hash
            output_path: output,
            dependencies: deps,
        });

        self.last_build = Some(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        Ok(())
    }

    /// Get files that need recompilation.
    pub fn files_to_rebuild(&self, sources: &[PathBuf]) -> Vec<PathBuf> {
        sources
            .iter()
            .filter(|p| self.needs_rebuild(p))
            .cloned()
            .collect()
    }

    /// Clear cache.
    pub fn clear(&mut self) {
        self.files.clear();
        self.last_build = None;
        self.config_hash = None;
    }

    /// Remove stale entries (files that no longer exist).
    pub fn prune(&mut self) {
        self.files.retain(|path, _| path.exists());
    }
}

/// Get default cache path for a project.
pub fn default_cache_path(project_root: &Path) -> PathBuf {
    project_root.join(".roast").join("build_cache.json")
}

/// Simple hash function for strings (FNV-1a).
pub fn hash_str(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_empty_cache_needs_rebuild() {
        let cache = BuildCache::default();
        assert!(cache.needs_rebuild(Path::new("nonexistent.roast")));
    }

    #[test]
    fn test_hash_str() {
        let h1 = hash_str("hello");
        let h2 = hash_str("hello");
        let h3 = hash_str("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
}
