//! GPU device abstraction.

use std::sync::Arc;
use std::collections::HashMap;
use crate::backend::{Backend, BackendType};
use crate::error::{GpuError, GpuResult};
use crate::memory::MemoryPool;

/// GPU device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    /// NVIDIA GPU (CUDA).
    Cuda,
    /// OpenCL device.
    OpenCL,
    /// Apple Metal.
    Metal,
    /// Vulkan compute.
    Vulkan,
    /// CPU fallback.
    Cpu,
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cuda => "CUDA",
            Self::OpenCL => "OpenCL",
            Self::Metal => "Metal",
            Self::Vulkan => "Vulkan",
            Self::Cpu => "CPU",
        }
    }
}

/// GPU device information.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device index.
    pub index: usize,
    
    /// Device name.
    pub name: String,
    
    /// Device type.
    pub device_type: DeviceType,
    
    /// Vendor name.
    pub vendor: String,
    
    /// Total memory in bytes.
    pub total_memory: u64,
    
    /// Available memory in bytes.
    pub available_memory: u64,
    
    /// Compute capability (for CUDA).
    pub compute_capability: Option<(u32, u32)>,
    
    /// Maximum threads per block.
    pub max_threads_per_block: u32,
    
    /// Maximum block dimensions.
    pub max_block_dim: [u32; 3],
    
    /// Maximum grid dimensions.
    pub max_grid_dim: [u32; 3],
    
    /// Warp/wavefront size.
    pub warp_size: u32,
    
    /// Number of multiprocessors/compute units.
    pub multiprocessor_count: u32,
    
    /// Clock rate in MHz.
    pub clock_rate_mhz: u32,
    
    /// Memory clock rate in MHz.
    pub memory_clock_rate_mhz: u32,
    
    /// Memory bus width in bits.
    pub memory_bus_width: u32,
    
    /// L2 cache size in bytes.
    pub l2_cache_size: u32,
    
    /// Supports unified memory.
    pub unified_memory: bool,
    
    /// Supports concurrent kernels.
    pub concurrent_kernels: bool,
    
    /// Driver version.
    pub driver_version: String,
}

impl DeviceInfo {
    /// Get theoretical memory bandwidth in GB/s.
    pub fn memory_bandwidth_gbps(&self) -> f64 {
        let bits_per_second = self.memory_clock_rate_mhz as f64 * 1e6 
            * self.memory_bus_width as f64 * 2.0; // DDR
        bits_per_second / 8.0 / 1e9
    }
    
    /// Get theoretical peak FLOPS.
    pub fn peak_flops(&self) -> f64 {
        // Approximate: cores * clock * 2 (FMA)
        let cores = self.multiprocessor_count * 64; // Approximate cores per SM
        cores as f64 * self.clock_rate_mhz as f64 * 1e6 * 2.0
    }
}

/// A GPU device handle.
#[derive(Clone)]
pub struct Device {
    /// Device information.
    pub info: DeviceInfo,
    
    /// Backend implementation.
    backend: Arc<dyn Backend>,
    
    /// Memory pool for this device.
    memory_pool: Arc<MemoryPool>,
    
    /// Device context/handle.
    context: DeviceContext,
}

/// Device context (backend-specific handle).
#[derive(Clone)]
pub struct DeviceContext {
    /// Raw handle.
    pub handle: usize,
    
    /// Stream/queue for async operations.
    pub stream: usize,
    
    /// Device type.
    pub device_type: DeviceType,
}

impl Device {
    /// Get the default GPU device.
    pub fn default() -> GpuResult<Self> {
        // Try backends in order of preference
        let backends = [
            BackendType::Cuda,
            BackendType::Metal,
            BackendType::OpenCL,
            BackendType::Vulkan,
            BackendType::Cpu,
        ];
        
        for backend_type in backends {
            if let Ok(device) = Self::new(backend_type, 0) {
                return Ok(device);
            }
        }
        
        Err(GpuError::NoDevice)
    }
    
    /// Create a device with specific backend and index.
    pub fn new(backend_type: BackendType, index: usize) -> GpuResult<Self> {
        let backend = crate::backend::create_backend(backend_type)?;
        let info = backend.get_device_info(index)?;
        let context = backend.create_context(index)?;
        let memory_pool = Arc::new(MemoryPool::new(info.total_memory));
        
        Ok(Self {
            info,
            backend: Arc::from(backend),
            memory_pool,
            context,
        })
    }
    
    /// List all available devices.
    pub fn list_all() -> GpuResult<Vec<DeviceInfo>> {
        let mut devices = Vec::new();
        
        // Check each backend
        for backend_type in [
            BackendType::Cuda,
            BackendType::Metal,
            BackendType::OpenCL,
            BackendType::Vulkan,
        ] {
            if let Ok(backend) = crate::backend::create_backend(backend_type) {
                if let Ok(count) = backend.device_count() {
                    for i in 0..count {
                        if let Ok(info) = backend.get_device_info(i) {
                            devices.push(info);
                        }
                    }
                }
            }
        }
        
        if devices.is_empty() {
            // Add CPU fallback
            devices.push(DeviceInfo {
                index: 0,
                name: "CPU".to_string(),
                device_type: DeviceType::Cpu,
                vendor: "Host".to_string(),
                total_memory: get_system_memory(),
                available_memory: get_system_memory(),
                compute_capability: None,
                max_threads_per_block: 1,
                max_block_dim: [1, 1, 1],
                max_grid_dim: [1, 1, 1],
                warp_size: 1,
                multiprocessor_count: num_cpus(),
                clock_rate_mhz: 0,
                memory_clock_rate_mhz: 0,
                memory_bus_width: 0,
                l2_cache_size: 0,
                unified_memory: true,
                concurrent_kernels: true,
                driver_version: "N/A".to_string(),
            });
        }
        
        Ok(devices)
    }
    
    /// Get device type.
    pub fn device_type(&self) -> DeviceType {
        self.info.device_type
    }
    
    /// Get device name.
    pub fn name(&self) -> &str {
        &self.info.name
    }
    
    /// Get total memory.
    pub fn total_memory(&self) -> u64 {
        self.info.total_memory
    }
    
    /// Get available memory.
    pub fn available_memory(&self) -> GpuResult<u64> {
        self.backend.get_available_memory(self.info.index)
    }
    
    /// Synchronize all operations on this device.
    pub fn synchronize(&self) -> GpuResult<()> {
        self.backend.synchronize(self.context.stream)
    }
    
    /// Get the backend.
    pub fn backend(&self) -> &dyn Backend {
        self.backend.as_ref()
    }
    
    /// Get the context.
    pub fn context(&self) -> &DeviceContext {
        &self.context
    }
    
    /// Get the memory pool.
    pub fn memory_pool(&self) -> &MemoryPool {
        &self.memory_pool
    }
    
    /// Allocate memory on this device.
    pub fn alloc(&self, size: usize) -> GpuResult<DevicePtr> {
        let ptr = self.backend.alloc(size)?;
        Ok(DevicePtr {
            ptr,
            size,
            device_type: self.info.device_type,
        })
    }
    
    /// Free memory on this device.
    pub fn free(&self, ptr: DevicePtr) -> GpuResult<()> {
        self.backend.free(ptr.ptr)
    }
    
    /// Copy data from host to device.
    pub fn copy_to_device<T: Copy>(&self, data: &[T]) -> GpuResult<DevicePtr> {
        let size = std::mem::size_of_val(data);
        let ptr = self.alloc(size)?;
        self.backend.copy_htod(
            data.as_ptr() as *const u8,
            ptr.ptr,
            size,
        )?;
        Ok(ptr)
    }
    
    /// Copy data from device to host.
    pub fn copy_to_host<T: Copy>(&self, ptr: &DevicePtr, data: &mut [T]) -> GpuResult<()> {
        let size = std::mem::size_of_val(data);
        if size > ptr.size {
            return Err(GpuError::Transfer(
                format!("Buffer too small: {} bytes, need {}", size, ptr.size)
            ));
        }
        self.backend.copy_dtoh(
            ptr.ptr,
            data.as_mut_ptr() as *mut u8,
            size,
        )
    }
    
    /// Copy data between device buffers.
    pub fn copy_device_to_device(&self, src: &DevicePtr, dst: &DevicePtr, size: usize) -> GpuResult<()> {
        if size > src.size || size > dst.size {
            return Err(GpuError::Transfer("Buffer too small".to_string()));
        }
        self.backend.copy_dtod(src.ptr, dst.ptr, size)
    }
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("name", &self.info.name)
            .field("type", &self.info.device_type)
            .field("memory", &self.info.total_memory)
            .finish()
    }
}

/// Device memory pointer.
#[derive(Debug, Clone, Copy)]
pub struct DevicePtr {
    /// Raw pointer/handle.
    pub ptr: usize,
    
    /// Size in bytes.
    pub size: usize,
    
    /// Device type.
    pub device_type: DeviceType,
}

impl DevicePtr {
    /// Null pointer.
    pub fn null() -> Self {
        Self {
            ptr: 0,
            size: 0,
            device_type: DeviceType::Cpu,
        }
    }
    
    /// Check if null.
    pub fn is_null(&self) -> bool {
        self.ptr == 0
    }
    
    /// Offset the pointer.
    pub fn offset(&self, bytes: usize) -> Self {
        Self {
            ptr: self.ptr + bytes,
            size: self.size.saturating_sub(bytes),
            device_type: self.device_type,
        }
    }
}

/// Get system memory in bytes.
fn get_system_memory() -> u64 {
    // Try to read from /proc/meminfo on Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    
    // Default fallback
    8 * 1024 * 1024 * 1024 // 8 GB
}

/// Get number of CPU cores.
fn num_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|p| p.get() as u32)
        .unwrap_or(4)
}

