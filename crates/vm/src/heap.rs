//! Heap management for the VM.

use roast_runtime::Value;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique heap object ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HeapId(u64);

impl HeapId {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// A heap-allocated object.
#[derive(Clone, Debug)]
pub struct HeapObject {
    /// Unique identifier.
    pub id: HeapId,
    /// The actual value.
    pub value: Value,
    /// Reference count.
    pub ref_count: u32,
    /// Whether this object is marked (for GC).
    pub marked: bool,
    /// Weak reference count.
    pub weak_count: u32,
}

impl HeapObject {
    pub fn new(value: Value) -> Self {
        Self {
            id: HeapId::new(),
            value,
            ref_count: 1,
            marked: false,
            weak_count: 0,
        }
    }

    pub fn inc_ref(&mut self) {
        self.ref_count = self.ref_count.saturating_add(1);
    }

    pub fn dec_ref(&mut self) -> bool {
        self.ref_count = self.ref_count.saturating_sub(1);
        self.ref_count == 0 && self.weak_count == 0
    }
}

/// The VM heap for managing allocated objects.
pub struct Heap {
    /// All allocated objects.
    objects: FxHashMap<HeapId, HeapObject>,
    /// Total bytes allocated.
    bytes_allocated: usize,
    /// Threshold for triggering GC.
    gc_threshold: usize,
    /// Number of collections performed.
    collections: u64,
}

impl Heap {
    /// Creates a new heap.
    pub fn new() -> Self {
        Self {
            objects: FxHashMap::default(),
            bytes_allocated: 0,
            gc_threshold: 1024 * 1024, // 1 MB
            collections: 0,
        }
    }

    /// Creates a heap with a custom GC threshold.
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            gc_threshold: threshold,
            ..Self::new()
        }
    }

    /// Allocates a new object on the heap.
    pub fn alloc(&mut self, value: Value) -> HeapId {
        let size = self.estimate_size(&value);
        self.bytes_allocated += size;

        let obj = HeapObject::new(value);
        let id = obj.id;
        self.objects.insert(id, obj);

        // Check if we need to collect
        if self.bytes_allocated > self.gc_threshold {
            self.collect();
        }

        id
    }

    /// Gets an object by ID.
    pub fn get(&self, id: HeapId) -> Option<&HeapObject> {
        self.objects.get(&id)
    }

    /// Gets a mutable object by ID.
    pub fn get_mut(&mut self, id: HeapId) -> Option<&mut HeapObject> {
        self.objects.get_mut(&id)
    }

    /// Increments the reference count.
    pub fn inc_ref(&mut self, id: HeapId) {
        if let Some(obj) = self.objects.get_mut(&id) {
            obj.inc_ref();
        }
    }

    /// Decrements the reference count and frees if zero.
    pub fn dec_ref(&mut self, id: HeapId) {
        let should_free = self.objects.get_mut(&id).map(|obj| obj.dec_ref()).unwrap_or(false);
        if should_free {
            self.free(id);
        }
    }

    /// Frees an object.
    pub fn free(&mut self, id: HeapId) {
        if let Some(obj) = self.objects.remove(&id) {
            let size = self.estimate_size(&obj.value);
            self.bytes_allocated = self.bytes_allocated.saturating_sub(size);
        }
    }

    /// Performs garbage collection.
    pub fn collect(&mut self) {
        // Simple mark-and-sweep
        // In a real implementation, we'd trace from roots

        // Sweep: remove objects with ref_count 0
        let to_remove: Vec<_> = self.objects
            .iter()
            .filter(|(_, obj)| obj.ref_count == 0)
            .map(|(id, _)| *id)
            .collect();

        for id in to_remove {
            self.free(id);
        }

        self.collections += 1;

        // Increase threshold if we're still using a lot of memory
        if self.bytes_allocated > self.gc_threshold / 2 {
            self.gc_threshold *= 2;
        }
    }

    /// Estimates the size of a value in bytes.
    fn estimate_size(&self, value: &Value) -> usize {
        match value {
            Value::None | Value::Bool(_) => 8,
            Value::Int(_) => 16,
            Value::Float(_) => 16,
            Value::Str(s) => 24 + s.len(),
            Value::Bytes(b) => 24 + b.len(),
            Value::List(items) => 24 + items.lock().unwrap().len() * 8,
            Value::Tuple(items) => 24 + items.len() * 8,
            Value::Dict(map) => 48 + map.lock().unwrap().len() * 24,
            Value::Set(set) => 48 + set.lock().unwrap().len() * 16,
            _ => 64, // Default estimate for complex types
        }
    }

    /// Returns the number of allocated objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Returns true if the heap is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Returns the total bytes allocated.
    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated
    }

    /// Returns the number of GC collections.
    pub fn collections(&self) -> u64 {
        self.collections
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}
