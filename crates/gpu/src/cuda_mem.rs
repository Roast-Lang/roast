//! Full CUDA memory management.
//!
//! This module provides comprehensive memory management for CUDA devices:
//! - Device memory allocation (cuMemAlloc/cuMemFree)
//! - Pinned host memory
//! - Unified memory
//! - Memory pools with caching
//! - Async memory operations

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::ptr;
use libloading::{Library, Symbol};
use crate::error::{GpuError, GpuResult};

// =============================================================================
// CUDA Memory API Types
// =============================================================================

type CuMemAlloc = unsafe extern "C" fn(*mut usize, usize) -> i32;
type CuMemFree = unsafe extern "C" fn(usize) -> i32;
type CuMemcpyHtoD = unsafe extern "C" fn(usize, *const u8, usize) -> i32;
type CuMemcpyDtoH = unsafe extern "C" fn(*mut u8, usize, usize) -> i32;
type CuMemcpyDtoD = unsafe extern "C" fn(usize, usize, usize) -> i32;
type CuMemcpyHtoDAsync = unsafe extern "C" fn(usize, *const u8, usize, usize) -> i32;
type CuMemcpyDtoHAsync = unsafe extern "C" fn(*mut u8, usize, usize, usize) -> i32;
type CuMemcpyDtoDAsync = unsafe extern "C" fn(usize, usize, usize, usize) -> i32;
type CuMemsetD8 = unsafe extern "C" fn(usize, u8, usize) -> i32;
type CuMemsetD16 = unsafe extern "C" fn(usize, u16, usize) -> i32;
type CuMemsetD32 = unsafe extern "C" fn(usize, u32, usize) -> i32;
type CuMemsetD8Async = unsafe extern "C" fn(usize, u8, usize, usize) -> i32;
type CuMemsetD32Async = unsafe extern "C" fn(usize, u32, usize, usize) -> i32;
type CuMemGetInfo = unsafe extern "C" fn(*mut usize, *mut usize) -> i32;
type CuMemAllocHost = unsafe extern "C" fn(*mut *mut u8, usize) -> i32;
type CuMemFreeHost = unsafe extern "C" fn(*mut u8) -> i32;
type CuMemAllocManaged = unsafe extern "C" fn(*mut usize, usize, u32) -> i32;
type CuMemPrefetchAsync = unsafe extern "C" fn(usize, usize, i32, usize) -> i32;
type CuMemAdvise = unsafe extern "C" fn(usize, usize, i32, i32) -> i32;
type CuPointerGetAttribute = unsafe extern "C" fn(*mut i32, i32, usize) -> i32;

// Async memory pool APIs (CUDA 11.2+)
type CuMemPoolCreate = unsafe extern "C" fn(*mut usize, *const MemPoolProps) -> i32;
type CuMemPoolDestroy = unsafe extern "C" fn(usize) -> i32;
type CuMemAllocFromPool = unsafe extern "C" fn(*mut usize, usize, usize, usize) -> i32;
type CuMemPoolTrimTo = unsafe extern "C" fn(usize, usize) -> i32;
type CuMemPoolSetAttribute = unsafe extern "C" fn(usize, i32, *const std::ffi::c_void) -> i32;
type CuMemPoolGetAttribute = unsafe extern "C" fn(usize, i32, *mut std::ffi::c_void) -> i32;

/// Memory pool properties.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MemPoolProps {
    alloc_type: i32,
    handle_types: i32,
    location_type: i32,
    location_id: i32,
    reserved: [u8; 64],
}

impl Default for MemPoolProps {
    fn default() -> Self {
        Self {
            alloc_type: 0,
            handle_types: 0,
            location_type: 0,
            location_id: 0,
            reserved: [0; 64],
        }
    }
}

// =============================================================================
// CUDA Memory Library
// =============================================================================

/// CUDA memory API wrapper.
pub struct CudaMemLib {
    _lib: Library,
    mem_alloc: Symbol<'static, CuMemAlloc>,
    mem_free: Symbol<'static, CuMemFree>,
    memcpy_htod: Symbol<'static, CuMemcpyHtoD>,
    memcpy_dtoh: Symbol<'static, CuMemcpyDtoH>,
    memcpy_dtod: Symbol<'static, CuMemcpyDtoD>,
    memcpy_htod_async: Symbol<'static, CuMemcpyHtoDAsync>,
    memcpy_dtoh_async: Symbol<'static, CuMemcpyDtoHAsync>,
    memcpy_dtod_async: Symbol<'static, CuMemcpyDtoDAsync>,
    memset_d8: Symbol<'static, CuMemsetD8>,
    memset_d32: Symbol<'static, CuMemsetD32>,
    memset_d8_async: Symbol<'static, CuMemsetD8Async>,
    memset_d32_async: Symbol<'static, CuMemsetD32Async>,
    mem_get_info: Symbol<'static, CuMemGetInfo>,
    mem_alloc_host: Symbol<'static, CuMemAllocHost>,
    mem_free_host: Symbol<'static, CuMemFreeHost>,
    mem_alloc_managed: Option<Symbol<'static, CuMemAllocManaged>>,
    mem_prefetch_async: Option<Symbol<'static, CuMemPrefetchAsync>>,
}

impl CudaMemLib {
    /// Load the CUDA driver library for memory operations.
    pub fn load() -> GpuResult<Self> {
        let lib_names = if cfg!(windows) {
            vec!["nvcuda.dll"]
        } else if cfg!(target_os = "macos") {
            vec!["libcuda.dylib"]
        } else {
            vec!["libcuda.so.1", "libcuda.so"]
        };
        
        let mut last_error = None;
        for name in lib_names {
            match unsafe { Library::new(name) } {
                Ok(lib) => {
                    return Self::from_library(lib);
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }
        
        Err(GpuError::DriverNotFound(format!(
            "CUDA driver not found: {:?}",
            last_error
        )))
    }
    
    fn from_library(lib: Library) -> GpuResult<Self> {
        unsafe {
            let lib = Box::leak(Box::new(lib));
            
            macro_rules! load_symbol {
                ($name:ident, $ty:ty, $sym:expr) => {
                    let $name: Symbol<$ty> = lib
                        .get($sym)
                        .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
                    let $name: Symbol<'static, $ty> = std::mem::transmute($name);
                };
            }
            
            macro_rules! load_symbol_opt {
                ($name:ident, $ty:ty, $sym:expr) => {
                    let $name: Option<Symbol<$ty>> = lib.get($sym).ok();
                    let $name: Option<Symbol<'static, $ty>> = $name.map(|s| std::mem::transmute(s));
                };
            }
            
            load_symbol!(mem_alloc, CuMemAlloc, b"cuMemAlloc_v2\0");
            load_symbol!(mem_free, CuMemFree, b"cuMemFree_v2\0");
            load_symbol!(memcpy_htod, CuMemcpyHtoD, b"cuMemcpyHtoD_v2\0");
            load_symbol!(memcpy_dtoh, CuMemcpyDtoH, b"cuMemcpyDtoH_v2\0");
            load_symbol!(memcpy_dtod, CuMemcpyDtoD, b"cuMemcpyDtoD_v2\0");
            load_symbol!(memcpy_htod_async, CuMemcpyHtoDAsync, b"cuMemcpyHtoDAsync_v2\0");
            load_symbol!(memcpy_dtoh_async, CuMemcpyDtoHAsync, b"cuMemcpyDtoHAsync_v2\0");
            load_symbol!(memcpy_dtod_async, CuMemcpyDtoDAsync, b"cuMemcpyDtoDAsync_v2\0");
            load_symbol!(memset_d8, CuMemsetD8, b"cuMemsetD8_v2\0");
            load_symbol!(memset_d32, CuMemsetD32, b"cuMemsetD32_v2\0");
            load_symbol!(memset_d8_async, CuMemsetD8Async, b"cuMemsetD8Async\0");
            load_symbol!(memset_d32_async, CuMemsetD32Async, b"cuMemsetD32Async\0");
            load_symbol!(mem_get_info, CuMemGetInfo, b"cuMemGetInfo_v2\0");
            load_symbol!(mem_alloc_host, CuMemAllocHost, b"cuMemAllocHost_v2\0");
            load_symbol!(mem_free_host, CuMemFreeHost, b"cuMemFreeHost\0");
            
            // Optional (CUDA 6.0+)
            load_symbol_opt!(mem_alloc_managed, CuMemAllocManaged, b"cuMemAllocManaged\0");
            load_symbol_opt!(mem_prefetch_async, CuMemPrefetchAsync, b"cuMemPrefetchAsync\0");
            
            let lib_ref = &*(lib as *const Library);
            
            Ok(Self {
                _lib: ptr::read(lib_ref),
                mem_alloc, mem_free,
                memcpy_htod, memcpy_dtoh, memcpy_dtod,
                memcpy_htod_async, memcpy_dtoh_async, memcpy_dtod_async,
                memset_d8, memset_d32, memset_d8_async, memset_d32_async,
                mem_get_info, mem_alloc_host, mem_free_host,
                mem_alloc_managed, mem_prefetch_async,
            })
        }
    }
    
    /// Allocate device memory.
    pub fn alloc(&self, size: usize) -> GpuResult<usize> {
        let mut ptr: usize = 0;
        let result = unsafe { (self.mem_alloc)(&mut ptr, size) };
        
        if result != 0 {
            if result == 2 { // CUDA_ERROR_OUT_OF_MEMORY
                return Err(GpuError::OutOfMemory { requested: size, available: 0 });
            }
            return Err(GpuError::Allocation(format!("cuMemAlloc failed: {}", result)));
        }
        
        Ok(ptr)
    }
    
    /// Free device memory.
    pub fn free(&self, ptr: usize) -> GpuResult<()> {
        if ptr == 0 {
            return Ok(());
        }
        
        let result = unsafe { (self.mem_free)(ptr) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemFree failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Copy from host to device.
    pub fn copy_htod(&self, dst: usize, src: *const u8, size: usize) -> GpuResult<()> {
        let result = unsafe { (self.memcpy_htod)(dst, src, size) };
        
        if result != 0 {
            return Err(GpuError::Transfer(format!("cuMemcpyHtoD failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Copy from device to host.
    pub fn copy_dtoh(&self, dst: *mut u8, src: usize, size: usize) -> GpuResult<()> {
        let result = unsafe { (self.memcpy_dtoh)(dst, src, size) };
        
        if result != 0 {
            return Err(GpuError::Transfer(format!("cuMemcpyDtoH failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Copy from device to device.
    pub fn copy_dtod(&self, dst: usize, src: usize, size: usize) -> GpuResult<()> {
        let result = unsafe { (self.memcpy_dtod)(dst, src, size) };
        
        if result != 0 {
            return Err(GpuError::Transfer(format!("cuMemcpyDtoD failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Async copy from host to device.
    pub fn copy_htod_async(&self, dst: usize, src: *const u8, size: usize, stream: usize) -> GpuResult<()> {
        let result = unsafe { (self.memcpy_htod_async)(dst, src, size, stream) };
        
        if result != 0 {
            return Err(GpuError::Transfer(format!("cuMemcpyHtoDAsync failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Async copy from device to host.
    pub fn copy_dtoh_async(&self, dst: *mut u8, src: usize, size: usize, stream: usize) -> GpuResult<()> {
        let result = unsafe { (self.memcpy_dtoh_async)(dst, src, size, stream) };
        
        if result != 0 {
            return Err(GpuError::Transfer(format!("cuMemcpyDtoHAsync failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Set memory to a byte value.
    pub fn memset(&self, ptr: usize, value: u8, count: usize) -> GpuResult<()> {
        let result = unsafe { (self.memset_d8)(ptr, value, count) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemsetD8 failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Set memory to a 32-bit value.
    pub fn memset_d32(&self, ptr: usize, value: u32, count: usize) -> GpuResult<()> {
        let result = unsafe { (self.memset_d32)(ptr, value, count) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemsetD32 failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Async memset.
    pub fn memset_async(&self, ptr: usize, value: u8, count: usize, stream: usize) -> GpuResult<()> {
        let result = unsafe { (self.memset_d8_async)(ptr, value, count, stream) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemsetD8Async failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Get memory info.
    pub fn get_mem_info(&self) -> GpuResult<(usize, usize)> {
        let mut free: usize = 0;
        let mut total: usize = 0;
        let result = unsafe { (self.mem_get_info)(&mut free, &mut total) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemGetInfo failed: {}", result)));
        }
        
        Ok((free, total))
    }
    
    /// Allocate pinned host memory.
    pub fn alloc_host(&self, size: usize) -> GpuResult<*mut u8> {
        let mut ptr: *mut u8 = ptr::null_mut();
        let result = unsafe { (self.mem_alloc_host)(&mut ptr, size) };
        
        if result != 0 {
            return Err(GpuError::Allocation(format!("cuMemAllocHost failed: {}", result)));
        }
        
        Ok(ptr)
    }
    
    /// Free pinned host memory.
    pub fn free_host(&self, ptr: *mut u8) -> GpuResult<()> {
        if ptr.is_null() {
            return Ok(());
        }
        
        let result = unsafe { (self.mem_free_host)(ptr) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemFreeHost failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Allocate unified/managed memory.
    pub fn alloc_managed(&self, size: usize) -> GpuResult<usize> {
        let alloc_fn = self.mem_alloc_managed.as_ref()
            .ok_or_else(|| GpuError::Unsupported("Managed memory not available".to_string()))?;
        
        let mut ptr: usize = 0;
        const CU_MEM_ATTACH_GLOBAL: u32 = 1;
        let result = unsafe { alloc_fn(&mut ptr, size, CU_MEM_ATTACH_GLOBAL) };
        
        if result != 0 {
            return Err(GpuError::Allocation(format!("cuMemAllocManaged failed: {}", result)));
        }
        
        Ok(ptr)
    }
    
    /// Prefetch managed memory to device.
    pub fn prefetch(&self, ptr: usize, size: usize, device: i32, stream: usize) -> GpuResult<()> {
        let prefetch_fn = self.mem_prefetch_async.as_ref()
            .ok_or_else(|| GpuError::Unsupported("Memory prefetch not available".to_string()))?;
        
        let result = unsafe { prefetch_fn(ptr, size, device, stream) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuMemPrefetchAsync failed: {}", result)));
        }
        
        Ok(())
    }
}

// =============================================================================
// Memory Allocator
// =============================================================================

/// Allocation record.
#[derive(Debug, Clone)]
struct Allocation {
    ptr: usize,
    size: usize,
    is_pooled: bool,
}

/// Caching memory allocator.
pub struct CachingAllocator {
    /// CUDA memory API.
    lib: Arc<CudaMemLib>,
    
    /// Free blocks by size class (power of 2).
    free_blocks: Mutex<HashMap<usize, Vec<usize>>>,
    
    /// Active allocations.
    allocations: Mutex<HashMap<usize, Allocation>>,
    
    /// Minimum block size (256 bytes).
    min_block_size: usize,
    
    /// Maximum cached size (1 GB).
    max_cached_size: usize,
    
    /// Current cached size.
    cached_size: Mutex<usize>,
    
    /// Statistics.
    stats: Mutex<AllocatorStats>,
}

/// Allocator statistics.
#[derive(Debug, Clone, Default)]
pub struct AllocatorStats {
    /// Total bytes allocated.
    pub bytes_allocated: usize,
    
    /// Current bytes in use.
    pub bytes_in_use: usize,
    
    /// Total allocations.
    pub num_allocations: u64,
    
    /// Total frees.
    pub num_frees: u64,
    
    /// Cache hits.
    pub cache_hits: u64,
    
    /// Cache misses.
    pub cache_misses: u64,
}

impl CachingAllocator {
    /// Create a new caching allocator.
    pub fn new(lib: Arc<CudaMemLib>) -> Self {
        Self {
            lib,
            free_blocks: Mutex::new(HashMap::new()),
            allocations: Mutex::new(HashMap::new()),
            min_block_size: 256,
            max_cached_size: 1024 * 1024 * 1024, // 1 GB
            cached_size: Mutex::new(0),
            stats: Mutex::new(AllocatorStats::default()),
        }
    }
    
    /// Allocate memory.
    pub fn alloc(&self, size: usize) -> GpuResult<usize> {
        if size == 0 {
            return Ok(0);
        }
        
        // Round up to power of 2 and minimum block size
        let block_size = size.next_power_of_two().max(self.min_block_size);
        
        // Try to get from cache
        {
            let mut free = self.free_blocks.lock().unwrap();
            if let Some(blocks) = free.get_mut(&block_size) {
                if let Some(ptr) = blocks.pop() {
                    // Cache hit
                    let mut stats = self.stats.lock().unwrap();
                    stats.cache_hits += 1;
                    stats.bytes_in_use += block_size;
                    
                    let mut allocs = self.allocations.lock().unwrap();
                    allocs.insert(ptr, Allocation {
                        ptr,
                        size: block_size,
                        is_pooled: true,
                    });
                    
                    let mut cached = self.cached_size.lock().unwrap();
                    *cached -= block_size;
                    
                    return Ok(ptr);
                }
            }
        }
        
        // Cache miss - allocate new
        let ptr = self.lib.alloc(block_size)?;
        
        let mut stats = self.stats.lock().unwrap();
        stats.cache_misses += 1;
        stats.num_allocations += 1;
        stats.bytes_allocated += block_size;
        stats.bytes_in_use += block_size;
        
        let mut allocs = self.allocations.lock().unwrap();
        allocs.insert(ptr, Allocation {
            ptr,
            size: block_size,
            is_pooled: false,
        });
        
        Ok(ptr)
    }
    
    /// Free memory (returns to cache).
    pub fn free(&self, ptr: usize) -> GpuResult<()> {
        if ptr == 0 {
            return Ok(());
        }
        
        let alloc = {
            let mut allocs = self.allocations.lock().unwrap();
            match allocs.remove(&ptr) {
                Some(a) => a,
                None => return Err(GpuError::Cuda("Invalid pointer".to_string())),
            }
        };
        
        let mut stats = self.stats.lock().unwrap();
        stats.num_frees += 1;
        stats.bytes_in_use -= alloc.size;
        
        // Return to cache if under limit
        let mut cached = self.cached_size.lock().unwrap();
        if *cached + alloc.size <= self.max_cached_size {
            let mut free = self.free_blocks.lock().unwrap();
            free.entry(alloc.size).or_default().push(ptr);
            *cached += alloc.size;
        } else {
            // Actually free
            self.lib.free(ptr)?;
            stats.bytes_allocated -= alloc.size;
        }
        
        Ok(())
    }
    
    /// Clear the cache.
    pub fn clear_cache(&self) -> GpuResult<()> {
        let mut free = self.free_blocks.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        let mut cached = self.cached_size.lock().unwrap();
        
        for (size, blocks) in free.drain() {
            for ptr in blocks {
                self.lib.free(ptr)?;
                stats.bytes_allocated -= size;
            }
        }
        
        *cached = 0;
        
        Ok(())
    }
    
    /// Trim cache to target size.
    pub fn trim_to(&self, target: usize) -> GpuResult<()> {
        let mut free = self.free_blocks.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        let mut cached = self.cached_size.lock().unwrap();
        
        while *cached > target {
            // Find largest block to free
            let mut largest_size = 0;
            let mut largest_key = 0;
            
            for (&size, blocks) in free.iter() {
                if !blocks.is_empty() && size > largest_size {
                    largest_size = size;
                    largest_key = size;
                }
            }
            
            if largest_size == 0 {
                break;
            }
            
            if let Some(blocks) = free.get_mut(&largest_key) {
                if let Some(ptr) = blocks.pop() {
                    self.lib.free(ptr)?;
                    stats.bytes_allocated -= largest_size;
                    *cached -= largest_size;
                }
            }
        }
        
        Ok(())
    }
    
    /// Get statistics.
    pub fn stats(&self) -> AllocatorStats {
        self.stats.lock().unwrap().clone()
    }
    
    /// Get memory info.
    pub fn mem_info(&self) -> GpuResult<(usize, usize)> {
        self.lib.get_mem_info()
    }
}

impl Drop for CachingAllocator {
    fn drop(&mut self) {
        let _ = self.clear_cache();
    }
}

// =============================================================================
// Device Memory Handle
// =============================================================================

/// RAII handle for device memory.
pub struct DeviceMemory {
    ptr: usize,
    size: usize,
    allocator: Option<Arc<CachingAllocator>>,
    lib: Option<Arc<CudaMemLib>>,
}

impl DeviceMemory {
    /// Allocate device memory with caching allocator.
    pub fn alloc(allocator: Arc<CachingAllocator>, size: usize) -> GpuResult<Self> {
        let ptr = allocator.alloc(size)?;
        Ok(Self {
            ptr,
            size,
            allocator: Some(allocator),
            lib: None,
        })
    }
    
    /// Allocate device memory directly.
    pub fn alloc_direct(lib: Arc<CudaMemLib>, size: usize) -> GpuResult<Self> {
        let ptr = lib.alloc(size)?;
        Ok(Self {
            ptr,
            size,
            allocator: None,
            lib: Some(lib),
        })
    }
    
    /// Get pointer.
    pub fn ptr(&self) -> usize {
        self.ptr
    }
    
    /// Get size.
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Copy from host.
    pub fn copy_from_host(&self, data: &[u8]) -> GpuResult<()> {
        if data.len() > self.size {
            return Err(GpuError::Transfer("Data too large".to_string()));
        }
        
        if let Some(alloc) = &self.allocator {
            alloc.lib.copy_htod(self.ptr, data.as_ptr(), data.len())
        } else if let Some(lib) = &self.lib {
            lib.copy_htod(self.ptr, data.as_ptr(), data.len())
        } else {
            Err(GpuError::Runtime("No library available".to_string()))
        }
    }
    
    /// Copy to host.
    pub fn copy_to_host(&self, data: &mut [u8]) -> GpuResult<()> {
        if data.len() > self.size {
            return Err(GpuError::Transfer("Buffer too small".to_string()));
        }
        
        if let Some(alloc) = &self.allocator {
            alloc.lib.copy_dtoh(data.as_mut_ptr(), self.ptr, data.len())
        } else if let Some(lib) = &self.lib {
            lib.copy_dtoh(data.as_mut_ptr(), self.ptr, data.len())
        } else {
            Err(GpuError::Runtime("No library available".to_string()))
        }
    }
    
    /// Zero memory.
    pub fn zero(&self) -> GpuResult<()> {
        if let Some(alloc) = &self.allocator {
            alloc.lib.memset(self.ptr, 0, self.size)
        } else if let Some(lib) = &self.lib {
            lib.memset(self.ptr, 0, self.size)
        } else {
            Err(GpuError::Runtime("No library available".to_string()))
        }
    }
}

impl Drop for DeviceMemory {
    fn drop(&mut self) {
        if self.ptr != 0 {
            if let Some(alloc) = &self.allocator {
                let _ = alloc.free(self.ptr);
            } else if let Some(lib) = &self.lib {
                let _ = lib.free(self.ptr);
            }
        }
    }
}

// =============================================================================
// Pinned Host Memory
// =============================================================================

/// RAII handle for pinned host memory.
pub struct PinnedHostMemory {
    ptr: *mut u8,
    size: usize,
    lib: Arc<CudaMemLib>,
}

impl PinnedHostMemory {
    /// Allocate pinned memory.
    pub fn alloc(lib: Arc<CudaMemLib>, size: usize) -> GpuResult<Self> {
        let ptr = lib.alloc_host(size)?;
        Ok(Self { ptr, size, lib })
    }
    
    /// Get pointer.
    pub fn ptr(&self) -> *mut u8 {
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

impl Drop for PinnedHostMemory {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            let _ = self.lib.free_host(self.ptr);
        }
    }
}

unsafe impl Send for PinnedHostMemory {}
unsafe impl Sync for PinnedHostMemory {}

// =============================================================================
// Unified Memory
// =============================================================================

/// RAII handle for unified/managed memory.
pub struct UnifiedMemory {
    ptr: usize,
    size: usize,
    lib: Arc<CudaMemLib>,
}

impl UnifiedMemory {
    /// Allocate unified memory.
    pub fn alloc(lib: Arc<CudaMemLib>, size: usize) -> GpuResult<Self> {
        let ptr = lib.alloc_managed(size)?;
        Ok(Self { ptr, size, lib })
    }
    
    /// Get device pointer.
    pub fn device_ptr(&self) -> usize {
        self.ptr
    }
    
    /// Get host pointer.
    pub fn host_ptr(&self) -> *mut u8 {
        self.ptr as *mut u8
    }
    
    /// Get size.
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Prefetch to device.
    pub fn prefetch_to_device(&self, device: i32, stream: usize) -> GpuResult<()> {
        self.lib.prefetch(self.ptr, self.size, device, stream)
    }
    
    /// Prefetch to host (device -1).
    pub fn prefetch_to_host(&self, stream: usize) -> GpuResult<()> {
        self.lib.prefetch(self.ptr, self.size, -1, stream)
    }
    
    /// Get as slice.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.size) }
    }
    
    /// Get as mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut u8, self.size) }
    }
}

impl Drop for UnifiedMemory {
    fn drop(&mut self) {
        if self.ptr != 0 {
            let _ = self.lib.free(self.ptr);
        }
    }
}

unsafe impl Send for UnifiedMemory {}
unsafe impl Sync for UnifiedMemory {}

// =============================================================================
// Global Allocator
// =============================================================================

static GLOBAL_ALLOCATOR: RwLock<Option<Arc<CachingAllocator>>> = RwLock::new(None);

/// Initialize the global memory allocator.
pub fn init_allocator() -> GpuResult<()> {
    let lib = Arc::new(CudaMemLib::load()?);
    let alloc = Arc::new(CachingAllocator::new(lib));
    
    let mut global = GLOBAL_ALLOCATOR.write().unwrap();
    *global = Some(alloc);
    
    Ok(())
}

/// Get the global allocator.
pub fn get_allocator() -> GpuResult<Arc<CachingAllocator>> {
    let global = GLOBAL_ALLOCATOR.read().unwrap();
    global.clone().ok_or_else(|| GpuError::Runtime("Allocator not initialized".to_string()))
}

/// Allocate device memory using global allocator.
pub fn cuda_alloc(size: usize) -> GpuResult<usize> {
    get_allocator()?.alloc(size)
}

/// Free device memory using global allocator.
pub fn cuda_free(ptr: usize) -> GpuResult<()> {
    get_allocator()?.free(ptr)
}

/// Get memory info.
pub fn cuda_mem_info() -> GpuResult<(usize, usize)> {
    get_allocator()?.mem_info()
}

/// Clear the allocator cache.
pub fn cuda_empty_cache() -> GpuResult<()> {
    get_allocator()?.clear_cache()
}

/// Get allocator statistics.
pub fn cuda_memory_stats() -> GpuResult<AllocatorStats> {
    Ok(get_allocator()?.stats())
}

