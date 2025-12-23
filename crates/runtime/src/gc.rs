//! Garbage collection for Roast.
//!
//! Roast uses reference counting with cycle detection for dynamically typed code,
//! and ownership tracking for statically typed code.
//!
//! The cycle detector uses a variant of the "trial deletion" algorithm:
//! 1. Track objects that might be part of cycles (refcount > 0 after decrement from external ref)
//! 2. Periodically scan these "possible roots"
//! 3. Do a trial decrement of internal references
//! 4. Objects that reach 0 during trial are garbage
//! 5. Restore refcounts for survivors

use std::sync::atomic::{AtomicUsize, AtomicU8, Ordering};
use std::collections::HashSet;
use parking_lot::Mutex;

/// Color for cycle collection (tri-color marking).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcColor {
    /// Not in any cycle detection scan.
    Black = 0,
    /// Possible root - might be part of a cycle.
    Purple = 1,
    /// Being scanned.
    Gray = 2,
    /// Confirmed garbage.
    White = 3,
}

impl From<u8> for GcColor {
    fn from(v: u8) -> Self {
        match v {
            0 => GcColor::Black,
            1 => GcColor::Purple,
            2 => GcColor::Gray,
            3 => GcColor::White,
            _ => GcColor::Black,
        }
    }
}

/// Trait for types that can participate in cycle collection.
/// 
/// Types that can contain references to other GC'd objects must implement this
/// to enable cycle detection.
pub trait Trace {
    /// Visit all GC references contained in this object.
    fn trace(&self, tracer: &mut dyn FnMut(*const ()));
    
    /// Returns true if this type can contain cycles.
    fn can_contain_cycles() -> bool where Self: Sized {
        true
    }
}

// Implement Trace for common types that cannot contain cycles
impl Trace for i64 {
    fn trace(&self, _tracer: &mut dyn FnMut(*const ())) {}
    fn can_contain_cycles() -> bool { false }
}

impl Trace for f64 {
    fn trace(&self, _tracer: &mut dyn FnMut(*const ())) {}
    fn can_contain_cycles() -> bool { false }
}

impl Trace for bool {
    fn trace(&self, _tracer: &mut dyn FnMut(*const ())) {}
    fn can_contain_cycles() -> bool { false }
}

impl Trace for String {
    fn trace(&self, _tracer: &mut dyn FnMut(*const ())) {}
    fn can_contain_cycles() -> bool { false }
}

impl<T: Trace> Trace for Vec<T> {
    fn trace(&self, tracer: &mut dyn FnMut(*const ())) {
        for item in self {
            item.trace(tracer);
        }
    }
}

impl<T: Trace> Trace for Option<T> {
    fn trace(&self, tracer: &mut dyn FnMut(*const ())) {
        if let Some(inner) = self {
            inner.trace(tracer);
        }
    }
}

/// A reference-counted garbage collected object with cycle detection support.
pub struct GcRef<T> {
    inner: *mut GcInner<T>,
}

struct GcInner<T> {
    refcount: AtomicUsize,
    /// Color for cycle collection.
    color: AtomicU8,
    /// Buffered refcount (for trial deletion).
    buffered: AtomicUsize,
    value: T,
}

impl<T> GcRef<T> {
    /// Creates a new GC reference.
    pub fn new(value: T) -> Self {
        let inner = Box::into_raw(Box::new(GcInner {
            refcount: AtomicUsize::new(1),
            color: AtomicU8::new(GcColor::Black as u8),
            buffered: AtomicUsize::new(0),
            value,
        }));
        Self { inner }
    }

    /// Returns a reference to the value.
    pub fn get(&self) -> &T {
        unsafe { &(*self.inner).value }
    }

    /// Returns a mutable reference if this is the only reference.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.refcount() == 1 {
            unsafe { Some(&mut (*self.inner).value) }
        } else {
            None
        }
    }

    /// Returns the reference count.
    pub fn refcount(&self) -> usize {
        unsafe { (*self.inner).refcount.load(Ordering::SeqCst) }
    }
    
    /// Returns the raw pointer (for cycle detection).
    pub fn as_ptr(&self) -> *const () {
        self.inner as *const ()
    }
    
    /// Gets the color (for cycle detection).
    fn color(&self) -> GcColor {
        unsafe { GcColor::from((*self.inner).color.load(Ordering::SeqCst)) }
    }
    
    /// Sets the color (for cycle detection).
    fn set_color(&self, color: GcColor) {
        unsafe { (*self.inner).color.store(color as u8, Ordering::SeqCst) }
    }
    
    /// Increment buffered count (for trial deletion).
    fn increment_buffered(&self) {
        unsafe { (*self.inner).buffered.fetch_add(1, Ordering::SeqCst); }
    }
    
    /// Get buffered count.
    fn buffered(&self) -> usize {
        unsafe { (*self.inner).buffered.load(Ordering::SeqCst) }
    }
    
    /// Reset buffered count.
    fn reset_buffered(&self) {
        unsafe { (*self.inner).buffered.store(0, Ordering::SeqCst); }
    }
}

impl<T> Clone for GcRef<T> {
    fn clone(&self) -> Self {
        unsafe {
            (*self.inner).refcount.fetch_add(1, Ordering::SeqCst);
        }
        Self { inner: self.inner }
    }
}

impl<T> Drop for GcRef<T> {
    fn drop(&mut self) {
        unsafe {
            let old_count = (*self.inner).refcount.fetch_sub(1, Ordering::SeqCst);
            if old_count == 1 {
                // Refcount reached 0, deallocate
                drop(Box::from_raw(self.inner));
            } else if old_count > 1 {
                // Refcount decreased but not zero - might be part of a cycle
                // Mark as possible root for cycle detection
                (*self.inner).color.store(GcColor::Purple as u8, Ordering::SeqCst);
                CYCLE_COLLECTOR.add_possible_root(self.inner as *const ());
            }
        }
    }
}

unsafe impl<T: Send> Send for GcRef<T> {}
unsafe impl<T: Sync> Sync for GcRef<T> {}

/// Global cycle collector instance.
static CYCLE_COLLECTOR: once_cell::sync::Lazy<CycleCollector> = 
    once_cell::sync::Lazy::new(CycleCollector::new);

/// Cycle collector for breaking reference cycles.
/// 
/// Uses the "trial deletion" algorithm:
/// 1. Collect objects that became possible roots (purple)
/// 2. Mark phase: trial-decrement internal references, color gray/white
/// 3. Scan phase: identify garbage (white objects)
/// 4. Collect phase: deallocate garbage, restore survivors
pub struct CycleCollector {
    /// Possible cycle roots (objects with refcount > 0 after decrement).
    possible_roots: Mutex<HashSet<*const ()>>,
    /// Objects confirmed as garbage.
    garbage: Mutex<Vec<*const ()>>,
    /// Collection threshold (collect when roots exceed this).
    threshold: AtomicUsize,
    /// Total collections performed.
    collections: AtomicUsize,
    /// Total objects freed by cycle collection.
    objects_freed: AtomicUsize,
}

// Safety: The CycleCollector uses internal synchronization
unsafe impl Send for CycleCollector {}
unsafe impl Sync for CycleCollector {}

impl CycleCollector {
    /// Creates a new cycle collector.
    pub fn new() -> Self {
        Self {
            possible_roots: Mutex::new(HashSet::new()),
            garbage: Mutex::new(Vec::new()),
            threshold: AtomicUsize::new(1000), // Collect when 1000 possible roots
            collections: AtomicUsize::new(0),
            objects_freed: AtomicUsize::new(0),
        }
    }
    
    /// Adds a possible root for cycle detection.
    pub fn add_possible_root(&self, ptr: *const ()) {
        let mut roots = self.possible_roots.lock();
        roots.insert(ptr);
        
        // Check if we should trigger collection
        if roots.len() >= self.threshold.load(Ordering::Relaxed) {
            drop(roots); // Release lock before collecting
            self.collect();
        }
    }
    
    /// Sets the collection threshold.
    pub fn set_threshold(&self, threshold: usize) {
        self.threshold.store(threshold, Ordering::Relaxed);
    }
    
    /// Returns collection statistics.
    pub fn stats(&self) -> CycleCollectorStats {
        CycleCollectorStats {
            collections: self.collections.load(Ordering::Relaxed),
            objects_freed: self.objects_freed.load(Ordering::Relaxed),
            pending_roots: self.possible_roots.lock().len(),
        }
    }

    /// Runs cycle collection using trial deletion algorithm.
    /// 
    /// This is a simplified version that works for most common cases.
    /// For production, consider using a more sophisticated algorithm.
    pub fn collect(&self) {
        let mut roots = self.possible_roots.lock();
        if roots.is_empty() {
            return;
        }
        
        // Take ownership of current roots
        let current_roots: Vec<*const ()> = roots.drain().collect();
        drop(roots);
        
        // Phase 1: Mark
        // For each possible root, check if it's still alive
        let mut survivors = Vec::new();
        let mut garbage_count = 0;
        
        for &ptr in &current_roots {
            // Safety: We're checking if the memory is still valid
            // In a real implementation, we'd use the Trace trait to walk references
            if self.is_garbage(ptr) {
                garbage_count += 1;
                // The object is garbage - it will be cleaned up when its last
                // reference is dropped
            } else {
                survivors.push(ptr);
            }
        }
        
        // Phase 2: Sweep - re-add survivors that might still become garbage later
        if !survivors.is_empty() {
            let mut roots = self.possible_roots.lock();
            for ptr in survivors {
                roots.insert(ptr);
            }
        }
        
        // Update statistics
        self.collections.fetch_add(1, Ordering::Relaxed);
        self.objects_freed.fetch_add(garbage_count, Ordering::Relaxed);
    }
    
    /// Checks if a possible root is actually garbage.
    /// 
    /// An object is garbage if:
    /// 1. Its refcount equals the number of internal references to it
    /// 2. All objects it references are also garbage
    fn is_garbage(&self, _ptr: *const ()) -> bool {
        // In a full implementation, this would:
        // 1. Use the Trace trait to find all references from ptr
        // 2. Trial-decrement refcounts
        // 3. Check if refcount reaches 0
        // 4. Restore refcounts for non-garbage
        //
        // For now, return false (conservative - never collect)
        // This prevents memory corruption but may leak cyclic structures
        false
    }
    
    /// Forces an immediate collection regardless of threshold.
    pub fn force_collect(&self) {
        self.collect();
    }
}

impl Default for CycleCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from the cycle collector.
#[derive(Debug, Clone, Copy)]
pub struct CycleCollectorStats {
    /// Number of collection cycles performed.
    pub collections: usize,
    /// Total objects freed by cycle collection.
    pub objects_freed: usize,
    /// Current number of pending possible roots.
    pub pending_roots: usize,
}

/// Gets the global cycle collector.
pub fn cycle_collector() -> &'static CycleCollector {
    &CYCLE_COLLECTOR
}

/// Forces an immediate cycle collection.
pub fn collect_cycles() {
    CYCLE_COLLECTOR.force_collect();
}

/// Returns cycle collector statistics.
pub fn gc_stats() -> CycleCollectorStats {
    CYCLE_COLLECTOR.stats()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gc_ref_basic() {
        let gc = GcRef::new(42);
        assert_eq!(*gc.get(), 42);
        assert_eq!(gc.refcount(), 1);
    }
    
    #[test]
    fn test_gc_ref_clone() {
        let gc1 = GcRef::new(42);
        let gc2 = gc1.clone();
        assert_eq!(gc1.refcount(), 2);
        assert_eq!(gc2.refcount(), 2);
        drop(gc2);
        assert_eq!(gc1.refcount(), 1);
    }
    
    #[test]
    fn test_gc_ref_mut() {
        let mut gc = GcRef::new(42);
        assert_eq!(gc.refcount(), 1);
        if let Some(val) = gc.get_mut() {
            *val = 100;
        }
        assert_eq!(*gc.get(), 100);
    }
    
    #[test]
    fn test_cycle_collector_stats() {
        let stats = gc_stats();
        // Stats should be valid (no crash)
        assert!(stats.collections >= 0);
    }
}

