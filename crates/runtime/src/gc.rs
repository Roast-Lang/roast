//! Garbage collection for Roast.
//!
//! Roast uses reference counting with optional cycle detection for
//! dynamically typed code, and ownership tracking for statically typed code.

use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::Mutex;

/// A reference-counted garbage collected object.
pub struct GcRef<T> {
    inner: *mut GcInner<T>,
}

struct GcInner<T> {
    refcount: AtomicUsize,
    value: T,
}

impl<T> GcRef<T> {
    /// Creates a new GC reference.
    pub fn new(value: T) -> Self {
        let inner = Box::into_raw(Box::new(GcInner {
            refcount: AtomicUsize::new(1),
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
            if (*self.inner).refcount.fetch_sub(1, Ordering::SeqCst) == 1 {
                drop(Box::from_raw(self.inner));
            }
        }
    }
}

unsafe impl<T: Send> Send for GcRef<T> {}
unsafe impl<T: Sync> Sync for GcRef<T> {}

/// Cycle collector for breaking reference cycles.
pub struct CycleCollector {
    roots: Mutex<Vec<*const ()>>,
}

impl CycleCollector {
    pub fn new() -> Self {
        Self {
            roots: Mutex::new(Vec::new()),
        }
    }

    /// Runs cycle collection.
    pub fn collect(&self) {
        // Would implement cycle detection algorithm
    }
}

impl Default for CycleCollector {
    fn default() -> Self {
        Self::new()
    }
}

