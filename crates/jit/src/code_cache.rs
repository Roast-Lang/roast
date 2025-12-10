//! Code cache for JIT-compiled code.
//!
//! Manages compiled code lifetime, invalidation, and memory pressure.

use crate::{JitResult, JitError};
use crate::baseline::BaselineCode;
use crate::optimizing::OptimizedCode;
use crate::tiers::CompilationTier;
use rustc_hash::FxHashMap;
use parking_lot::RwLock;

/// Entry in the code cache.
#[derive(Clone)]
pub struct CacheEntry {
    /// Function ID.
    pub func_id: u32,
    /// Current compilation tier.
    pub tier: CompilationTier,
    /// Baseline compiled code (if available).
    pub baseline: Option<BaselineCode>,
    /// Optimized code (if available).
    pub optimized: Option<OptimizedCode>,
    /// Number of times the code was executed.
    pub executions: u64,
    /// Last access timestamp.
    pub last_access: std::time::Instant,
    /// Size in bytes.
    pub size_bytes: usize,
    /// Whether this entry is pinned (won't be evicted).
    pub pinned: bool,
}

impl CacheEntry {
    pub fn new_baseline(func_id: u32, code: BaselineCode) -> Self {
        let size = code.code.len();
        Self {
            func_id,
            tier: CompilationTier::Baseline,
            baseline: Some(code),
            optimized: None,
            executions: 0,
            last_access: std::time::Instant::now(),
            size_bytes: size,
            pinned: false,
        }
    }
    
    pub fn new_optimized(func_id: u32, code: OptimizedCode) -> Self {
        let size = code.code.len();
        Self {
            func_id,
            tier: CompilationTier::Optimizing,
            baseline: None,
            optimized: Some(code),
            executions: 0,
            last_access: std::time::Instant::now(),
            size_bytes: size,
            pinned: false,
        }
    }
    
    /// Upgrades with optimized code while keeping baseline for deopt.
    pub fn upgrade(&mut self, optimized: OptimizedCode) {
        self.size_bytes += optimized.code.len();
        self.optimized = Some(optimized);
        self.tier = CompilationTier::Optimizing;
    }
    
    /// Gets the active code.
    pub fn active_code(&self) -> Option<&[u8]> {
        match self.tier {
            CompilationTier::Interpreter => None,
            CompilationTier::Baseline => self.baseline.as_ref().map(|b| b.code.as_slice()),
            CompilationTier::Optimizing => self.optimized.as_ref().map(|o| o.code.as_slice()),
        }
    }
    
    /// Records an execution.
    pub fn record_execution(&mut self) {
        self.executions += 1;
        self.last_access = std::time::Instant::now();
    }
}

/// Configuration for the code cache.
#[derive(Clone, Debug)]
pub struct CacheConfig {
    /// Maximum cache size in bytes.
    pub max_size: usize,
    /// Target utilization (0.0 - 1.0).
    pub target_utilization: f64,
    /// Enable LRU eviction.
    pub enable_eviction: bool,
    /// Minimum time before eviction (seconds).
    pub min_lifetime_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 64 * 1024 * 1024, // 64 MB
            target_utilization: 0.8,
            enable_eviction: true,
            min_lifetime_secs: 60,
        }
    }
}

/// The JIT code cache.
pub struct CodeCache {
    config: CacheConfig,
    entries: RwLock<FxHashMap<u32, CacheEntry>>,
    /// Current size in bytes.
    current_size: RwLock<usize>,
    /// Statistics.
    stats: RwLock<CacheStats>,
}

/// Cache statistics.
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
    /// Number of entries evicted.
    pub evictions: u64,
    /// Number of entries invalidated.
    pub invalidations: u64,
    /// Total compilations.
    pub compilations: u64,
    /// Total bytes compiled.
    pub bytes_compiled: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl CodeCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            entries: RwLock::new(FxHashMap::default()),
            current_size: RwLock::new(0),
            stats: RwLock::new(CacheStats::default()),
        }
    }
    
    /// Gets an entry from the cache.
    pub fn get(&self, func_id: u32) -> Option<CacheEntry> {
        let entries = self.entries.read();
        if let Some(entry) = entries.get(&func_id) {
            self.stats.write().hits += 1;
            Some(entry.clone())
        } else {
            self.stats.write().misses += 1;
            None
        }
    }
    
    /// Gets the current tier for a function.
    pub fn get_tier(&self, func_id: u32) -> CompilationTier {
        self.entries.read()
            .get(&func_id)
            .map(|e| e.tier)
            .unwrap_or(CompilationTier::Interpreter)
    }
    
    /// Inserts baseline compiled code.
    pub fn insert_baseline(&self, func_id: u32, code: BaselineCode) -> JitResult<()> {
        let size = code.code.len();
        
        // Check if we need to evict
        self.ensure_space(size)?;
        
        let entry = CacheEntry::new_baseline(func_id, code);
        
        {
            let mut entries = self.entries.write();
            if let Some(existing) = entries.get(&func_id) {
                *self.current_size.write() -= existing.size_bytes;
            }
            entries.insert(func_id, entry);
        }
        
        *self.current_size.write() += size;
        
        let mut stats = self.stats.write();
        stats.compilations += 1;
        stats.bytes_compiled += size as u64;
        
        Ok(())
    }
    
    /// Inserts or upgrades with optimized code.
    pub fn insert_optimized(&self, func_id: u32, code: OptimizedCode) -> JitResult<()> {
        let size = code.code.len();
        
        self.ensure_space(size)?;
        
        {
            let mut entries = self.entries.write();
            if let Some(entry) = entries.get_mut(&func_id) {
                entry.upgrade(code);
            } else {
                entries.insert(func_id, CacheEntry::new_optimized(func_id, code));
            }
        }
        
        *self.current_size.write() += size;
        
        let mut stats = self.stats.write();
        stats.compilations += 1;
        stats.bytes_compiled += size as u64;
        
        Ok(())
    }
    
    /// Invalidates compiled code for a function.
    pub fn invalidate(&self, func_id: u32) {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.remove(&func_id) {
            *self.current_size.write() -= entry.size_bytes;
            self.stats.write().invalidations += 1;
        }
    }
    
    /// Invalidates all entries.
    pub fn invalidate_all(&self) {
        let mut entries = self.entries.write();
        self.stats.write().invalidations += entries.len() as u64;
        entries.clear();
        *self.current_size.write() = 0;
    }
    
    /// Pins an entry to prevent eviction.
    pub fn pin(&self, func_id: u32) {
        if let Some(entry) = self.entries.write().get_mut(&func_id) {
            entry.pinned = true;
        }
    }
    
    /// Unpins an entry.
    pub fn unpin(&self, func_id: u32) {
        if let Some(entry) = self.entries.write().get_mut(&func_id) {
            entry.pinned = false;
        }
    }
    
    /// Records an execution of cached code.
    pub fn record_execution(&self, func_id: u32) {
        if let Some(entry) = self.entries.write().get_mut(&func_id) {
            entry.record_execution();
        }
    }
    
    /// Gets cache statistics.
    pub fn stats(&self) -> CacheStats {
        self.stats.read().clone()
    }
    
    /// Gets current cache size.
    pub fn size(&self) -> usize {
        *self.current_size.read()
    }
    
    /// Gets number of entries.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }
    
    /// Checks if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }
    
    /// Ensures there's space for new code.
    fn ensure_space(&self, needed: usize) -> JitResult<()> {
        if !self.config.enable_eviction {
            let current = *self.current_size.read();
            if current + needed > self.config.max_size {
                return Err(JitError::CacheFull);
            }
            return Ok(());
        }
        
        let target = (self.config.max_size as f64 * self.config.target_utilization) as usize;
        
        while *self.current_size.read() + needed > target {
            if !self.evict_one()? {
                break;
            }
        }
        
        Ok(())
    }
    
    /// Evicts one entry based on LRU policy.
    fn evict_one(&self) -> JitResult<bool> {
        let now = std::time::Instant::now();
        let min_age = std::time::Duration::from_secs(self.config.min_lifetime_secs);
        
        let mut entries = self.entries.write();
        
        // Find LRU entry that's not pinned and old enough
        let victim = entries
            .iter()
            .filter(|(_, e)| !e.pinned && now.duration_since(e.last_access) >= min_age)
            .min_by_key(|(_, e)| e.last_access)
            .map(|(id, _)| *id);
        
        if let Some(func_id) = victim {
            if let Some(entry) = entries.remove(&func_id) {
                *self.current_size.write() -= entry.size_bytes;
                self.stats.write().evictions += 1;
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Performs garbage collection on the cache.
    pub fn gc(&self) -> usize {
        let target = (self.config.max_size as f64 * self.config.target_utilization) as usize;
        let mut evicted = 0;
        
        while *self.current_size.read() > target {
            match self.evict_one() {
                Ok(true) => evicted += 1,
                _ => break,
            }
        }
        
        evicted
    }
}

impl Default for CodeCache {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

/// Handle for deoptimization.
pub struct DeoptHandle {
    func_id: u32,
    deopt_point: usize,
}

impl DeoptHandle {
    pub fn new(func_id: u32, deopt_point: usize) -> Self {
        Self { func_id, deopt_point }
    }
    
    /// Triggers deoptimization back to baseline.
    pub fn deopt(&self, cache: &CodeCache) -> JitResult<()> {
        let mut entries = cache.entries.write();
        
        if let Some(entry) = entries.get_mut(&self.func_id) {
            // If we have baseline code, fall back to it
            if entry.baseline.is_some() {
                entry.tier = CompilationTier::Baseline;
                entry.optimized = None;
                // Recalculate size
                if let Some(ref baseline) = entry.baseline {
                    entry.size_bytes = baseline.code.len();
                }
            } else {
                // Otherwise, drop to interpreter
                entry.tier = CompilationTier::Interpreter;
                entry.baseline = None;
                entry.optimized = None;
                entry.size_bytes = 0;
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_basic() {
        let cache = CodeCache::default();
        
        let code = BaselineCode {
            func_id: 1,
            code: vec![0u8; 100],
            entry_offset: 0,
            frame_size: 0,
            call_sites: Vec::new(),
        };
        
        cache.insert_baseline(1, code).unwrap();
        
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.size(), 100);
        
        let entry = cache.get(1).unwrap();
        assert_eq!(entry.tier, CompilationTier::Baseline);
    }
    
    #[test]
    fn test_cache_eviction() {
        let config = CacheConfig {
            max_size: 500,
            target_utilization: 0.8,
            enable_eviction: true,
            min_lifetime_secs: 0,
        };
        
        let cache = CodeCache::new(config);
        
        // Insert entries until we hit the limit
        for i in 0..10 {
            let code = BaselineCode {
                func_id: i,
                code: vec![0u8; 100],
                entry_offset: 0,
                frame_size: 0,
                call_sites: Vec::new(),
            };
            cache.insert_baseline(i, code).unwrap();
        }
        
        // Should have evicted some entries
        assert!(cache.len() < 10);
    }
    
    #[test]
    fn test_cache_invalidation() {
        let cache = CodeCache::default();
        
        let code = BaselineCode {
            func_id: 1,
            code: vec![0u8; 100],
            entry_offset: 0,
            frame_size: 0,
            call_sites: Vec::new(),
        };
        
        cache.insert_baseline(1, code).unwrap();
        assert_eq!(cache.len(), 1);
        
        cache.invalidate(1);
        assert_eq!(cache.len(), 0);
        assert!(cache.get(1).is_none());
    }
}
