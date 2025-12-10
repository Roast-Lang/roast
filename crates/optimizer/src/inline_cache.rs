//! Inline Caching for Dynamic Dispatch
//!
//! This module implements inline caching (IC) to speed up method calls on
//! dynamically-typed objects. Instead of performing full method lookup on
//! every call, we cache the lookup result and only revalidate when needed.
//!
//! # Cache Types
//!
//! - **Monomorphic IC**: One cached type/method pair (most common case)
//! - **Polymorphic IC**: Multiple type/method pairs (2-4 variants)
//! - **Megamorphic**: Fallback to hash table lookup (>4 variants)
//!
//! # Implementation
//!
//! Each call site has an associated inline cache that stores:
//! - The type of the receiver from the last call
//! - The resolved method for that type
//! - A shape/hidden class identifier for fast comparison
//!
//! # Benefits
//!
//! - Eliminates method lookup overhead (typically 10-50x faster)
//! - Enables speculative optimization (type prediction)
//! - Reduces megamorphic call overhead

use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Type shape identifier (like V8's hidden classes).
/// This uniquely identifies the shape/layout of an object type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ShapeId(pub u64);

impl ShapeId {
    pub const UNKNOWN: ShapeId = ShapeId(0);
    
    /// Creates a new unique shape ID.
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        ShapeId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A cached method reference.
#[derive(Clone, Debug)]
pub struct CachedMethod {
    /// The method's bytecode offset or function pointer.
    pub target: MethodTarget,
    /// The type shape this was resolved for.
    pub shape: ShapeId,
    /// Number of hits on this cache entry.
    pub hits: u64,
}

/// Target of a cached method.
#[derive(Clone, Copy, Debug)]
pub enum MethodTarget {
    /// Bytecode offset in the current module.
    BytecodeOffset(u32),
    /// Index in the function table.
    FunctionIndex(u32),
    /// Native function pointer (for FFI).
    Native(usize),
    /// Unresolved (cache miss).
    Unresolved,
}

/// State of an inline cache.
#[derive(Clone, Debug)]
pub enum ICState {
    /// Uninitialized - no calls yet.
    Uninitialized,
    /// Monomorphic - seen exactly one type.
    Monomorphic(CachedMethod),
    /// Polymorphic - seen 2-4 types.
    Polymorphic(Vec<CachedMethod>),
    /// Megamorphic - too many types, use hash table.
    Megamorphic,
}

impl Default for ICState {
    fn default() -> Self {
        ICState::Uninitialized
    }
}

/// An inline cache for a single call site.
#[derive(Clone, Debug, Default)]
pub struct InlineCache {
    /// Current state of the cache.
    pub state: ICState,
    /// Method name being called.
    pub method_name: String,
    /// Total number of calls through this cache.
    pub total_calls: u64,
    /// Number of cache misses.
    pub misses: u64,
}

/// Maximum number of variants in polymorphic IC.
pub const MAX_POLYMORPHIC_VARIANTS: usize = 4;

impl InlineCache {
    pub fn new(method_name: String) -> Self {
        Self {
            state: ICState::Uninitialized,
            method_name,
            total_calls: 0,
            misses: 0,
        }
    }
    
    /// Looks up a method in the cache.
    /// Returns the cached method if found, None on cache miss.
    pub fn lookup(&mut self, shape: ShapeId) -> Option<&CachedMethod> {
        self.total_calls += 1;
        
        match &mut self.state {
            ICState::Uninitialized => {
                self.misses += 1;
                None
            }
            ICState::Monomorphic(cached) => {
                if cached.shape == shape {
                    cached.hits += 1;
                    Some(cached)
                } else {
                    self.misses += 1;
                    None
                }
            }
            ICState::Polymorphic(variants) => {
                for cached in variants.iter_mut() {
                    if cached.shape == shape {
                        cached.hits += 1;
                        return Some(cached);
                    }
                }
                self.misses += 1;
                None
            }
            ICState::Megamorphic => {
                self.misses += 1;
                None
            }
        }
    }
    
    /// Updates the cache with a newly resolved method.
    pub fn update(&mut self, shape: ShapeId, target: MethodTarget) {
        let new_cached = CachedMethod {
            target,
            shape,
            hits: 1,
        };
        
        match &mut self.state {
            ICState::Uninitialized => {
                self.state = ICState::Monomorphic(new_cached);
            }
            ICState::Monomorphic(existing) => {
                if existing.shape != shape {
                    // Transition to polymorphic
                    let existing = existing.clone();
                    self.state = ICState::Polymorphic(vec![existing, new_cached]);
                }
            }
            ICState::Polymorphic(variants) => {
                // Check if shape already exists
                for cached in variants.iter_mut() {
                    if cached.shape == shape {
                        cached.target = target;
                        return;
                    }
                }
                
                // Add new variant
                if variants.len() < MAX_POLYMORPHIC_VARIANTS {
                    variants.push(new_cached);
                } else {
                    // Transition to megamorphic
                    self.state = ICState::Megamorphic;
                }
            }
            ICState::Megamorphic => {
                // Stay megamorphic
            }
        }
    }
    
    /// Returns the hit rate for this cache.
    pub fn hit_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            1.0 - (self.misses as f64 / self.total_calls as f64)
        }
    }
    
    /// Checks if this cache is effective.
    pub fn is_effective(&self) -> bool {
        // Consider effective if >80% hit rate
        self.hit_rate() > 0.8
    }
    
    /// Returns whether this is a monomorphic call site.
    pub fn is_monomorphic(&self) -> bool {
        matches!(self.state, ICState::Monomorphic(_))
    }
    
    /// Resets the cache to uninitialized state.
    pub fn reset(&mut self) {
        self.state = ICState::Uninitialized;
        self.total_calls = 0;
        self.misses = 0;
    }
}

/// Manager for all inline caches in a function/module.
#[derive(Clone, Debug, Default)]
pub struct InlineCacheManager {
    /// Caches indexed by call site ID.
    caches: FxHashMap<u32, InlineCache>,
    /// Next available call site ID.
    next_id: u32,
    /// Global statistics.
    stats: ICStats,
}

/// Global inline cache statistics.
#[derive(Clone, Debug, Default)]
pub struct ICStats {
    pub total_caches: usize,
    pub monomorphic_caches: usize,
    pub polymorphic_caches: usize,
    pub megamorphic_caches: usize,
    pub total_calls: u64,
    pub total_misses: u64,
}

impl InlineCacheManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Creates a new inline cache for a call site.
    pub fn create_cache(&mut self, method_name: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.caches.insert(id, InlineCache::new(method_name));
        self.stats.total_caches += 1;
        id
    }
    
    /// Gets a cache by ID.
    pub fn get_cache(&self, id: u32) -> Option<&InlineCache> {
        self.caches.get(&id)
    }
    
    /// Gets a mutable cache by ID.
    pub fn get_cache_mut(&mut self, id: u32) -> Option<&mut InlineCache> {
        self.caches.get_mut(&id)
    }
    
    /// Performs a lookup through the cache.
    pub fn lookup(&mut self, cache_id: u32, shape: ShapeId) -> Option<MethodTarget> {
        if let Some(cache) = self.caches.get_mut(&cache_id) {
            cache.lookup(shape).map(|c| c.target.clone())
        } else {
            None
        }
    }
    
    /// Updates a cache with a resolved method.
    pub fn update(&mut self, cache_id: u32, shape: ShapeId, target: MethodTarget) {
        if let Some(cache) = self.caches.get_mut(&cache_id) {
            cache.update(shape, target);
        }
    }
    
    /// Collects statistics about all caches.
    pub fn collect_stats(&mut self) -> ICStats {
        let mut stats = ICStats::default();
        stats.total_caches = self.caches.len();
        
        for cache in self.caches.values() {
            match &cache.state {
                ICState::Monomorphic(_) => stats.monomorphic_caches += 1,
                ICState::Polymorphic(_) => stats.polymorphic_caches += 1,
                ICState::Megamorphic => stats.megamorphic_caches += 1,
                ICState::Uninitialized => {}
            }
            stats.total_calls += cache.total_calls;
            stats.total_misses += cache.misses;
        }
        
        self.stats = stats.clone();
        stats
    }
    
    /// Returns the overall hit rate.
    pub fn hit_rate(&self) -> f64 {
        if self.stats.total_calls == 0 {
            0.0
        } else {
            1.0 - (self.stats.total_misses as f64 / self.stats.total_calls as f64)
        }
    }
    
    /// Finds monomorphic call sites that could be devirtualized.
    pub fn find_devirtualization_candidates(&self) -> Vec<(u32, &CachedMethod)> {
        let mut candidates = Vec::new();
        
        for (&id, cache) in &self.caches {
            if let ICState::Monomorphic(cached) = &cache.state {
                if cache.total_calls > 10 && cache.hit_rate() > 0.95 {
                    candidates.push((id, cached));
                }
            }
        }
        
        candidates
    }
    
    /// Resets all caches (e.g., for reoptimization).
    pub fn reset_all(&mut self) {
        for cache in self.caches.values_mut() {
            cache.reset();
        }
    }
    
    /// Iterates over all caches with their IDs.
    pub fn iter_caches(&self) -> impl Iterator<Item = (&u32, &InlineCache)> {
        self.caches.iter()
    }
}

/// Polymorphic inline cache with dispatch table.
/// Used for very hot polymorphic call sites.
#[derive(Clone, Debug, Default)]
pub struct DispatchTable {
    /// Mapping from shape to method target.
    entries: FxHashMap<ShapeId, MethodTarget>,
    /// Default target for unknown shapes (if any).
    default_target: Option<MethodTarget>,
}

impl DispatchTable {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Sets a default target for unknown shapes.
    pub fn set_default(&mut self, target: MethodTarget) {
        self.default_target = Some(target);
    }
    
    /// Adds an entry to the dispatch table.
    pub fn add(&mut self, shape: ShapeId, target: MethodTarget) {
        self.entries.insert(shape, target);
    }
    
    /// Looks up a target in the dispatch table.
    pub fn lookup(&self, shape: ShapeId) -> Option<&MethodTarget> {
        self.entries.get(&shape).or(self.default_target.as_ref())
    }
    
    /// Gets the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    /// Checks if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Shape transition record for tracking object shape changes.
#[derive(Clone, Debug)]
pub struct ShapeTransition {
    pub from: ShapeId,
    pub to: ShapeId,
    pub property: String,
}

/// Shape manager for tracking object shapes (hidden classes).
#[derive(Clone, Debug, Default)]
pub struct ShapeManager {
    /// Mapping from shape to its property names.
    shapes: FxHashMap<ShapeId, Vec<String>>,
    /// Transitions between shapes.
    transitions: FxHashMap<(ShapeId, String), ShapeId>,
    /// Root shape (empty object).
    root: ShapeId,
}

impl ShapeManager {
    pub fn new() -> Self {
        let root = ShapeId::new();
        let mut shapes = FxHashMap::default();
        shapes.insert(root, Vec::new());
        
        Self {
            shapes,
            transitions: FxHashMap::default(),
            root,
        }
    }
    
    /// Gets the root shape (empty object).
    pub fn root(&self) -> ShapeId {
        self.root
    }
    
    /// Transitions from one shape to another when adding a property.
    pub fn transition(&mut self, from: ShapeId, property: &str) -> ShapeId {
        let key = (from, property.to_string());
        
        if let Some(&to) = self.transitions.get(&key) {
            return to;
        }
        
        // Create new shape
        let to = ShapeId::new();
        let mut props = self.shapes.get(&from).cloned().unwrap_or_default();
        props.push(property.to_string());
        self.shapes.insert(to, props);
        self.transitions.insert(key, to);
        
        to
    }
    
    /// Gets the properties for a shape.
    pub fn properties(&self, shape: ShapeId) -> Option<&[String]> {
        self.shapes.get(&shape).map(|v| v.as_slice())
    }
    
    /// Gets the property index for a shape.
    pub fn property_index(&self, shape: ShapeId, property: &str) -> Option<usize> {
        self.shapes.get(&shape)?
            .iter()
            .position(|p| p == property)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_monomorphic_cache() {
        let mut cache = InlineCache::new("foo".to_string());
        let shape = ShapeId::new();
        
        // First call - miss
        assert!(cache.lookup(shape).is_none());
        
        // Update cache
        cache.update(shape, MethodTarget::BytecodeOffset(100));
        
        // Second call - hit
        assert!(cache.lookup(shape).is_some());
        
        // Check state
        assert!(cache.is_monomorphic());
    }
    
    #[test]
    fn test_polymorphic_transition() {
        let mut cache = InlineCache::new("bar".to_string());
        let shape1 = ShapeId::new();
        let shape2 = ShapeId::new();
        
        cache.update(shape1, MethodTarget::BytecodeOffset(100));
        cache.update(shape2, MethodTarget::BytecodeOffset(200));
        
        // Should be polymorphic now
        assert!(matches!(cache.state, ICState::Polymorphic(_)));
    }
    
    #[test]
    fn test_shape_transitions() {
        let mut mgr = ShapeManager::new();
        
        let empty = mgr.root();
        let with_x = mgr.transition(empty, "x");
        let with_xy = mgr.transition(with_x, "y");
        
        assert_eq!(mgr.property_index(with_xy, "x"), Some(0));
        assert_eq!(mgr.property_index(with_xy, "y"), Some(1));
    }
}
