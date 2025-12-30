//! Incremental build support for Roast.
//!
//! Tracks file modifications and only recompiles changed files.
//! Supports:
//! - File modification time tracking
//! - Content hashing for precise change detection
//! - Dependency graph tracking
//! - Cache invalidation strategies
//! - Parallel rebuild scheduling

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Read};
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
    /// Version of the cache format.
    pub version: u32,
}

/// Current cache version.
pub const CACHE_VERSION: u32 = 2;

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
    /// Reverse dependencies (files that import this file).
    pub dependents: Vec<PathBuf>,
    /// Compilation artifacts hash.
    pub artifact_hash: Option<u64>,
}

/// Change type for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeType {
    /// File is new.
    Added,
    /// File content changed.
    Modified,
    /// File was deleted.
    Deleted,
    /// File unchanged.
    Unchanged,
    /// Dependency changed.
    DependencyChanged,
}

/// Incremental build plan.
#[derive(Debug, Clone, Default)]
pub struct BuildPlan {
    /// Files that need to be compiled (in dependency order).
    pub to_compile: Vec<PathBuf>,
    /// Files that are unchanged.
    pub unchanged: Vec<PathBuf>,
    /// Files that were deleted.
    pub deleted: Vec<PathBuf>,
    /// Total files in project.
    pub total_files: usize,
    /// Compilation layers (for parallel compilation).
    pub layers: Vec<Vec<PathBuf>>,
}

impl BuildPlan {
    /// Calculate percentage of files that can be skipped.
    pub fn skip_percentage(&self) -> f64 {
        if self.total_files == 0 {
            return 0.0;
        }
        (self.unchanged.len() as f64 / self.total_files as f64) * 100.0
    }
    
    /// Check if any files need compilation.
    pub fn needs_compilation(&self) -> bool {
        !self.to_compile.is_empty()
    }
}

impl BuildCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            last_build: None,
            config_hash: None,
            version: CACHE_VERSION,
        }
    }

    /// Load cache from file.
    pub fn load(cache_path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(cache_path)?;
        let cache: Self = serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        // Check version compatibility
        if cache.version != CACHE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cache version mismatch: expected {}, got {}", CACHE_VERSION, cache.version)
            ));
        }
        
        Ok(cache)
    }
    
    /// Load cache or create new if not exists/invalid.
    pub fn load_or_new(cache_path: &Path) -> Self {
        Self::load(cache_path).unwrap_or_else(|_| Self::new())
    }

    /// Save cache to file.
    pub fn save(&self, cache_path: &Path) -> io::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(cache_path, content)
    }
    
    /// Compute content hash for a file.
    pub fn compute_content_hash(path: &Path) -> io::Result<u64> {
        let mut file = fs::File::open(path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        Ok(hash_bytes(&contents))
    }
    
    /// Detect what changed for a file.
    pub fn detect_change(&self, path: &Path) -> ChangeType {
        let entry = match self.files.get(path) {
            Some(e) => e,
            None => return ChangeType::Added,
        };
        
        // Check if file exists
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return ChangeType::Deleted,
        };
        
        let current_mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        let current_size = metadata.len();
        
        // Quick check: size and mtime unchanged
        if current_mtime == entry.mtime && current_size == entry.size {
            return ChangeType::Unchanged;
        }
        
        // If we have content hash, verify with that
        if let Some(cached_hash) = entry.content_hash {
            if let Ok(current_hash) = Self::compute_content_hash(path) {
                if current_hash == cached_hash {
                    return ChangeType::Unchanged;
                }
            }
        }
        
        ChangeType::Modified
    }

    /// Check if a file needs recompilation.
    pub fn needs_rebuild(&self, path: &Path) -> bool {
        match self.detect_change(path) {
            ChangeType::Unchanged => {
                // Check if output still exists
                if let Some(entry) = self.files.get(path) {
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
                }
                false
            }
            _ => true,
        }
    }
    
    /// Create a build plan for a set of source files.
    pub fn create_build_plan(&self, sources: &[PathBuf]) -> BuildPlan {
        let mut plan = BuildPlan::default();
        plan.total_files = sources.len();
        
        let mut needs_rebuild: HashSet<PathBuf> = HashSet::new();
        let mut deleted: HashSet<PathBuf> = HashSet::new();
        
        // First pass: identify directly changed files
        for source in sources {
            match self.detect_change(source) {
                ChangeType::Added | ChangeType::Modified => {
                    needs_rebuild.insert(source.clone());
                }
                ChangeType::Deleted => {
                    deleted.insert(source.clone());
                }
                ChangeType::Unchanged => {
                    // Check if output exists
                    if let Some(entry) = self.files.get(source) {
                        if let Some(ref output) = entry.output_path {
                            if !output.exists() {
                                needs_rebuild.insert(source.clone());
                            }
                        }
                    }
                }
                ChangeType::DependencyChanged => {
                    needs_rebuild.insert(source.clone());
                }
            }
        }
        
        // Second pass: propagate to dependents
        let mut to_check: VecDeque<PathBuf> = needs_rebuild.iter().cloned().collect();
        while let Some(path) = to_check.pop_front() {
            if let Some(entry) = self.files.get(&path) {
                for dependent in &entry.dependents {
                    if !needs_rebuild.contains(dependent) && sources.contains(dependent) {
                        needs_rebuild.insert(dependent.clone());
                        to_check.push_back(dependent.clone());
                    }
                }
            }
        }
        
        // Categorize files
        for source in sources {
            if deleted.contains(source) {
                plan.deleted.push(source.clone());
            } else if needs_rebuild.contains(source) {
                plan.to_compile.push(source.clone());
            } else {
                plan.unchanged.push(source.clone());
            }
        }
        
        // Build compilation layers for parallel execution
        plan.layers = self.build_layers(&plan.to_compile);
        
        plan
    }
    
    /// Build compilation layers for parallel execution.
    /// Files in the same layer have no inter-dependencies.
    fn build_layers(&self, files: &[PathBuf]) -> Vec<Vec<PathBuf>> {
        let file_set: HashSet<_> = files.iter().collect();
        let mut layers: Vec<Vec<PathBuf>> = Vec::new();
        let mut assigned: HashSet<PathBuf> = HashSet::new();
        
        while assigned.len() < files.len() {
            let mut current_layer = Vec::new();
            
            for file in files {
                if assigned.contains(file) {
                    continue;
                }
                
                // Check if all dependencies are either:
                // 1. Not in the to-compile set
                // 2. Already assigned to previous layers
                let deps_satisfied = if let Some(entry) = self.files.get(file) {
                    entry.dependencies.iter().all(|dep| {
                        !file_set.contains(dep) || assigned.contains(dep)
                    })
                } else {
                    true // New file, no known deps
                };
                
                if deps_satisfied {
                    current_layer.push(file.clone());
                }
            }
            
            if current_layer.is_empty() {
                // Cycle detected or all remaining files have unmet deps
                // Add remaining files to avoid infinite loop
                for file in files {
                    if !assigned.contains(file) {
                        current_layer.push(file.clone());
                    }
                }
            }
            
            // Add files to assigned set
            for file in &current_layer {
                assigned.insert(file.clone());
            }
            
            layers.push(current_layer);
        }
        
        layers
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
        
        let content_hash = Self::compute_content_hash(path).ok();

        // Update reverse dependencies
        for dep in &deps {
            if let Some(dep_entry) = self.files.get_mut(dep) {
                if !dep_entry.dependents.contains(&path.to_path_buf()) {
                    dep_entry.dependents.push(path.to_path_buf());
                }
            }
        }

        self.files.insert(path.to_path_buf(), FileEntry {
            mtime,
            size: metadata.len(),
            content_hash,
            output_path: output,
            dependencies: deps,
            dependents: Vec::new(),
            artifact_hash: None,
        });

        self.last_build = Some(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        Ok(())
    }
    
    /// Update configuration hash.
    pub fn update_config(&mut self, config_str: &str) {
        let new_hash = hash_str(config_str);
        if self.config_hash != Some(new_hash) {
            // Config changed, invalidate everything
            self.clear();
            self.config_hash = Some(new_hash);
        }
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
    
    /// Get statistics about the cache.
    pub fn stats(&self) -> CacheStats {
        let mut total_deps = 0;
        let mut total_dependents = 0;
        
        for entry in self.files.values() {
            total_deps += entry.dependencies.len();
            total_dependents += entry.dependents.len();
        }
        
        CacheStats {
            total_files: self.files.len(),
            total_dependencies: total_deps,
            total_dependents: total_dependents,
            last_build: self.last_build,
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_files: usize,
    pub total_dependencies: usize,
    pub total_dependents: usize,
    pub last_build: Option<u64>,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Build Cache: {} files, {} deps tracked",
            self.total_files,
            self.total_dependencies
        )?;
        if let Some(ts) = self.last_build {
            write!(f, ", last build: {}", ts)?;
        }
        Ok(())
    }
}

/// Get default cache path for a project.
pub fn default_cache_path(project_root: &Path) -> PathBuf {
    project_root.join(".roast").join("build_cache.json")
}

/// Simple hash function for strings (FNV-1a).
pub fn hash_str(s: &str) -> u64 {
    hash_bytes(s.as_bytes())
}

/// Simple hash function for bytes (FNV-1a).
pub fn hash_bytes(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash
}

/// Combine multiple hashes.
pub fn combine_hashes(hashes: &[u64]) -> u64 {
    let mut combined: u64 = 0xcbf29ce484222325;
    for hash in hashes {
        combined ^= hash;
        combined = combined.wrapping_mul(0x100000001b3);
    }
    combined
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
    
    #[test]
    fn test_hash_bytes() {
        let h1 = hash_bytes(b"hello world");
        let h2 = hash_bytes(b"hello world");
        let h3 = hash_bytes(b"different");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
    
    #[test]
    fn test_build_plan() {
        let cache = BuildCache::new();
        let sources = vec![
            PathBuf::from("a.roast"),
            PathBuf::from("b.roast"),
            PathBuf::from("c.roast"),
        ];
        
        let plan = cache.create_build_plan(&sources);
        
        // All files are new, so all should be compiled
        assert_eq!(plan.to_compile.len(), 3);
        assert!(plan.unchanged.is_empty());
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = BuildCache::new();
        let stats = cache.stats();
        assert_eq!(stats.total_files, 0);
    }
    
    #[test]
    fn test_combine_hashes() {
        let h1 = hash_str("a");
        let h2 = hash_str("b");
        let combined1 = combine_hashes(&[h1, h2]);
        let combined2 = combine_hashes(&[h1, h2]);
        let combined3 = combine_hashes(&[h2, h1]);
        
        assert_eq!(combined1, combined2);
        // Order matters
        assert_ne!(combined1, combined3);
    }
}
