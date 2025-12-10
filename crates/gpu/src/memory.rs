//! GPU memory management.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::error::{GpuError, GpuResult};

/// Buffer usage flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    /// Read-only on device.
    ReadOnly,
    /// Write-only on device.
    WriteOnly,
    /// Read-write on device.
    ReadWrite,
    /// Host-accessible (for unified memory).
    HostAccessible,
}

/// GPU buffer.
pub struct Buffer {
    /// Raw pointer.
    ptr: usize,
    
    /// Size in bytes.
    size: usize,
    
    /// Usage flags.
    usage: BufferUsage,
    
    /// Is allocated from pool.
    pooled: bool,
}

impl Buffer {
    /// Create a new buffer.
    pub fn new(ptr: usize, size: usize, usage: BufferUsage) -> Self {
        Self {
            ptr,
            size,
            usage,
            pooled: false,
        }
    }
    
    /// Get raw pointer.
    pub fn ptr(&self) -> usize {
        self.ptr
    }
    
    /// Get size.
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Get usage.
    pub fn usage(&self) -> BufferUsage {
        self.usage
    }
}

/// Memory pool for efficient allocation.
pub struct MemoryPool {
    /// Total capacity.
    capacity: u64,
    
    /// Allocated bytes.
    allocated: Arc<Mutex<u64>>,
    
    /// Free blocks by size.
    free_blocks: Arc<Mutex<HashMap<usize, Vec<usize>>>>,
    
    /// Block size threshold for pooling.
    pool_threshold: usize,
}

impl MemoryPool {
    /// Create a new memory pool.
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            allocated: Arc::new(Mutex::new(0)),
            free_blocks: Arc::new(Mutex::new(HashMap::new())),
            pool_threshold: 256 * 1024, // 256 KB
        }
    }
    
    /// Get total capacity.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }
    
    /// Get allocated bytes.
    pub fn allocated(&self) -> u64 {
        *self.allocated.lock().unwrap()
    }
    
    /// Get available bytes.
    pub fn available(&self) -> u64 {
        self.capacity.saturating_sub(self.allocated())
    }
    
    /// Try to get a block from the pool.
    pub fn try_get(&self, size: usize) -> Option<usize> {
        if size < self.pool_threshold {
            return None;
        }
        
        // Round up to nearest power of 2 for better reuse
        let bucket_size = size.next_power_of_two();
        
        let mut free = self.free_blocks.lock().unwrap();
        if let Some(blocks) = free.get_mut(&bucket_size) {
            blocks.pop()
        } else {
            None
        }
    }
    
    /// Return a block to the pool.
    pub fn return_block(&self, ptr: usize, size: usize) {
        if size < self.pool_threshold {
            return;
        }
        
        let bucket_size = size.next_power_of_two();
        
        let mut free = self.free_blocks.lock().unwrap();
        free.entry(bucket_size).or_default().push(ptr);
    }
    
    /// Record allocation.
    pub fn record_alloc(&self, size: usize) {
        let mut allocated = self.allocated.lock().unwrap();
        *allocated += size as u64;
    }
    
    /// Record deallocation.
    pub fn record_free(&self, size: usize) {
        let mut allocated = self.allocated.lock().unwrap();
        *allocated = allocated.saturating_sub(size as u64);
    }
    
    /// Clear the pool.
    pub fn clear(&self) {
        let mut free = self.free_blocks.lock().unwrap();
        free.clear();
    }
    
    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        let free = self.free_blocks.lock().unwrap();
        let total_free_blocks: usize = free.values().map(|v| v.len()).sum();
        let total_free_bytes: usize = free.iter()
            .map(|(size, blocks)| size * blocks.len())
            .sum();
        
        PoolStats {
            capacity: self.capacity,
            allocated: self.allocated(),
            free_blocks: total_free_blocks,
            free_bytes: total_free_bytes as u64,
        }
    }
}

/// Pool statistics.
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub capacity: u64,
    pub allocated: u64,
    pub free_blocks: usize,
    pub free_bytes: u64,
}

/// Memory allocator interface.
pub trait MemoryAllocator: Send + Sync {
    /// Allocate memory.
    fn alloc(&self, size: usize) -> GpuResult<usize>;
    
    /// Free memory.
    fn free(&self, ptr: usize) -> GpuResult<()>;
    
    /// Get allocated bytes.
    fn allocated(&self) -> u64;
}

/// Simple allocator that wraps backend allocation.
pub struct SimpleAllocator {
    allocations: Mutex<HashMap<usize, usize>>,
    allocated: Mutex<u64>,
}

impl SimpleAllocator {
    pub fn new() -> Self {
        Self {
            allocations: Mutex::new(HashMap::new()),
            allocated: Mutex::new(0),
        }
    }
}

impl Default for SimpleAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Pinned (page-locked) host memory.
pub struct PinnedMemory {
    ptr: *mut u8,
    size: usize,
}

impl PinnedMemory {
    /// Allocate pinned memory.
    pub fn new(size: usize) -> GpuResult<Self> {
        // This would call cudaHostAlloc or similar
        // For now, use regular allocation
        let layout = std::alloc::Layout::from_size_align(size, 64)
            .map_err(|e| GpuError::Allocation(e.to_string()))?;
        
        let ptr = unsafe { std::alloc::alloc(layout) };
        
        if ptr.is_null() {
            return Err(GpuError::OutOfMemory {
                requested: size,
                available: 0,
            });
        }
        
        Ok(Self { ptr, size })
    }
    
    /// Get pointer.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }
    
    /// Get mutable pointer.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }
    
    /// Get size.
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Get as slice.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }
    
    /// Get as mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl Drop for PinnedMemory {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            let layout = std::alloc::Layout::from_size_align(self.size, 64)
                .expect("Invalid layout");
            unsafe { std::alloc::dealloc(self.ptr, layout) };
        }
    }
}

unsafe impl Send for PinnedMemory {}
unsafe impl Sync for PinnedMemory {}

/// Unified memory (accessible from both host and device).
pub struct UnifiedMemory {
    ptr: usize,
    size: usize,
}

impl UnifiedMemory {
    /// Allocate unified memory.
    pub fn new(size: usize) -> GpuResult<Self> {
        // This would call cudaMallocManaged
        // For now, stub
        Err(GpuError::Unsupported("Unified memory not implemented".to_string()))
    }
    
    /// Get device pointer.
    pub fn device_ptr(&self) -> usize {
        self.ptr
    }
    
    /// Get host pointer.
    pub fn host_ptr(&self) -> *mut u8 {
        self.ptr as *mut u8
    }
    
    /// Prefetch to device.
    pub fn prefetch_to_device(&self, _device_id: i32) -> GpuResult<()> {
        Ok(())
    }
    
    /// Prefetch to host.
    pub fn prefetch_to_host(&self) -> GpuResult<()> {
        Ok(())
    }
}

/// Memory copy direction.
#[derive(Debug, Clone, Copy)]
pub enum MemcpyKind {
    HostToDevice,
    DeviceToHost,
    DeviceToDevice,
    HostToHost,
}

/// Asynchronous memory copy handle.
pub struct AsyncMemcpy {
    /// Source pointer.
    src: usize,
    
    /// Destination pointer.
    dst: usize,
    
    /// Size.
    size: usize,
    
    /// Direction.
    kind: MemcpyKind,
    
    /// Stream.
    stream: usize,
}

impl AsyncMemcpy {
    /// Wait for completion.
    pub fn wait(&self) -> GpuResult<()> {
        // This would call cudaStreamSynchronize
        Ok(())
    }
    
    /// Check if complete.
    pub fn is_complete(&self) -> GpuResult<bool> {
        Ok(true)
    }
}

