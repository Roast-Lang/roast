//! GPU backend abstraction.

use std::sync::Arc;
use crate::device::{DeviceInfo, DeviceType, DeviceContext};
use crate::kernel::CompiledKernel;
use crate::error::{GpuError, GpuResult};

/// Backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendType {
    Cuda,
    OpenCL,
    Metal,
    Vulkan,
    Cpu,
}

impl BackendType {
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

/// Backend trait for GPU operations.
pub trait Backend: Send + Sync {
    /// Get backend type.
    fn backend_type(&self) -> BackendType;
    
    /// Check if backend is available.
    fn is_available(&self) -> bool;
    
    /// Get number of devices.
    fn device_count(&self) -> GpuResult<usize>;
    
    /// Get device information.
    fn get_device_info(&self, index: usize) -> GpuResult<DeviceInfo>;
    
    /// Create a device context.
    fn create_context(&self, index: usize) -> GpuResult<DeviceContext>;
    
    /// Destroy a device context.
    fn destroy_context(&self, context: &DeviceContext) -> GpuResult<()>;
    
    /// Get available memory.
    fn get_available_memory(&self, index: usize) -> GpuResult<u64>;
    
    /// Allocate device memory.
    fn alloc(&self, size: usize) -> GpuResult<usize>;
    
    /// Free device memory.
    fn free(&self, ptr: usize) -> GpuResult<()>;
    
    /// Copy host to device.
    fn copy_htod(&self, src: *const u8, dst: usize, size: usize) -> GpuResult<()>;
    
    /// Copy device to host.
    fn copy_dtoh(&self, src: usize, dst: *mut u8, size: usize) -> GpuResult<()>;
    
    /// Copy device to device.
    fn copy_dtod(&self, src: usize, dst: usize, size: usize) -> GpuResult<()>;
    
    /// Synchronize stream.
    fn synchronize(&self, stream: usize) -> GpuResult<()>;
    
    /// Compile a kernel.
    fn compile_kernel(&self, source: &str, name: &str) -> GpuResult<CompiledKernel>;
    
    /// Launch a kernel.
    fn launch_kernel(
        &self,
        kernel: &CompiledKernel,
        grid: [u32; 3],
        block: [u32; 3],
        args: &[KernelArg],
        stream: usize,
    ) -> GpuResult<()>;
}

/// Kernel argument.
#[derive(Debug, Clone)]
pub enum KernelArg {
    /// Device pointer.
    Ptr(usize),
    
    /// Scalar i32.
    I32(i32),
    
    /// Scalar i64.
    I64(i64),
    
    /// Scalar u32.
    U32(u32),
    
    /// Scalar u64.
    U64(u64),
    
    /// Scalar f32.
    F32(f32),
    
    /// Scalar f64.
    F64(f64),
}

impl KernelArg {
    /// Get size of argument.
    pub fn size(&self) -> usize {
        match self {
            Self::Ptr(_) => std::mem::size_of::<usize>(),
            Self::I32(_) | Self::U32(_) | Self::F32(_) => 4,
            Self::I64(_) | Self::U64(_) | Self::F64(_) => 8,
        }
    }
    
    /// Get raw bytes.
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            Self::Ptr(v) => v.to_ne_bytes().to_vec(),
            Self::I32(v) => v.to_ne_bytes().to_vec(),
            Self::I64(v) => v.to_ne_bytes().to_vec(),
            Self::U32(v) => v.to_ne_bytes().to_vec(),
            Self::U64(v) => v.to_ne_bytes().to_vec(),
            Self::F32(v) => v.to_ne_bytes().to_vec(),
            Self::F64(v) => v.to_ne_bytes().to_vec(),
        }
    }
}

/// Create a backend by type.
pub fn create_backend(backend_type: BackendType) -> GpuResult<Box<dyn Backend>> {
    match backend_type {
        BackendType::Cuda => {
            #[cfg(feature = "cuda")]
            {
                Ok(Box::new(CudaBackend::new()?))
            }
            #[cfg(not(feature = "cuda"))]
            {
                Err(GpuError::BackendNotAvailable("CUDA".to_string()))
            }
        }
        BackendType::OpenCL => {
            // OpenCL backend not fully implemented yet
            Err(GpuError::BackendNotAvailable("OpenCL not fully implemented".to_string()))
        }
        BackendType::Metal => {
            #[cfg(all(feature = "metal", target_os = "macos"))]
            {
                Ok(Box::new(MetalBackend::new()?))
            }
            #[cfg(not(all(feature = "metal", target_os = "macos")))]
            {
                Err(GpuError::BackendNotAvailable("Metal".to_string()))
            }
        }
        BackendType::Vulkan => {
            #[cfg(feature = "vulkan")]
            {
                Ok(Box::new(VulkanBackend::new()?))
            }
            #[cfg(not(feature = "vulkan"))]
            {
                Err(GpuError::BackendNotAvailable("Vulkan".to_string()))
            }
        }
        BackendType::Cpu => Ok(Box::new(CpuBackend::new())),
    }
}

// =============================================================================
// CUDA Backend
// =============================================================================

#[cfg(feature = "cuda")]
mod cuda {
    use super::*;
    use libloading::{Library, Symbol};
    use std::ffi::CString;
    
    /// CUDA backend.
    pub struct CudaBackend {
        lib: Library,
        initialized: bool,
    }
    
    impl CudaBackend {
        pub fn new() -> GpuResult<Self> {
            // Try to load CUDA runtime library
            let lib_name = if cfg!(windows) {
                "cudart64_12.dll"
            } else if cfg!(target_os = "macos") {
                "libcudart.dylib"
            } else {
                "libcudart.so"
            };
            
            let lib = unsafe {
                Library::new(lib_name).map_err(|e| {
                    GpuError::DriverNotFound(format!("CUDA: {}", e))
                })?
            };
            
            let mut backend = Self {
                lib,
                initialized: false,
            };
            
            // Initialize CUDA
            backend.init()?;
            
            Ok(backend)
        }
        
        fn init(&mut self) -> GpuResult<()> {
            // Call cudaSetDevice(0) to initialize
            type CudaSetDevice = unsafe extern "C" fn(i32) -> i32;
            
            let func: Symbol<CudaSetDevice> = unsafe {
                self.lib.get(b"cudaSetDevice\0").map_err(|e| {
                    GpuError::Cuda(format!("Failed to load cudaSetDevice: {}", e))
                })?
            };
            
            let result = unsafe { func(0) };
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaSetDevice failed: {}", result)));
            }
            
            self.initialized = true;
            Ok(())
        }
    }
    
    impl Backend for CudaBackend {
        fn backend_type(&self) -> BackendType {
            BackendType::Cuda
        }
        
        fn is_available(&self) -> bool {
            self.initialized
        }
        
        fn device_count(&self) -> GpuResult<usize> {
            type CudaGetDeviceCount = unsafe extern "C" fn(*mut i32) -> i32;
            
            let func: Symbol<CudaGetDeviceCount> = unsafe {
                self.lib.get(b"cudaGetDeviceCount\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let mut count: i32 = 0;
            let result = unsafe { func(&mut count) };
            
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaGetDeviceCount failed: {}", result)));
            }
            
            Ok(count as usize)
        }
        
        fn get_device_info(&self, index: usize) -> GpuResult<DeviceInfo> {
            // This would call cudaGetDeviceProperties
            // For now, use nvidia-smi as fallback
            get_cuda_device_info(index)
        }
        
        fn create_context(&self, index: usize) -> GpuResult<DeviceContext> {
            type CudaSetDevice = unsafe extern "C" fn(i32) -> i32;
            type CudaStreamCreate = unsafe extern "C" fn(*mut usize) -> i32;
            
            let set_device: Symbol<CudaSetDevice> = unsafe {
                self.lib.get(b"cudaSetDevice\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let result = unsafe { set_device(index as i32) };
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaSetDevice failed: {}", result)));
            }
            
            let stream_create: Symbol<CudaStreamCreate> = unsafe {
                self.lib.get(b"cudaStreamCreate\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let mut stream: usize = 0;
            let result = unsafe { stream_create(&mut stream) };
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaStreamCreate failed: {}", result)));
            }
            
            Ok(DeviceContext {
                handle: index,
                stream,
                device_type: DeviceType::Cuda,
            })
        }
        
        fn destroy_context(&self, context: &DeviceContext) -> GpuResult<()> {
            type CudaStreamDestroy = unsafe extern "C" fn(usize) -> i32;
            
            let func: Symbol<CudaStreamDestroy> = unsafe {
                self.lib.get(b"cudaStreamDestroy\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let result = unsafe { func(context.stream) };
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaStreamDestroy failed: {}", result)));
            }
            
            Ok(())
        }
        
        fn get_available_memory(&self, _index: usize) -> GpuResult<u64> {
            type CudaMemGetInfo = unsafe extern "C" fn(*mut usize, *mut usize) -> i32;
            
            let func: Symbol<CudaMemGetInfo> = unsafe {
                self.lib.get(b"cudaMemGetInfo\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let mut free: usize = 0;
            let mut total: usize = 0;
            let result = unsafe { func(&mut free, &mut total) };
            
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaMemGetInfo failed: {}", result)));
            }
            
            Ok(free as u64)
        }
        
        fn alloc(&self, size: usize) -> GpuResult<usize> {
            type CudaMalloc = unsafe extern "C" fn(*mut usize, usize) -> i32;
            
            let func: Symbol<CudaMalloc> = unsafe {
                self.lib.get(b"cudaMalloc\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let mut ptr: usize = 0;
            let result = unsafe { func(&mut ptr, size) };
            
            if result != 0 {
                if result == 2 {
                    return Err(GpuError::OutOfMemory {
                        requested: size,
                        available: 0,
                    });
                }
                return Err(GpuError::Cuda(format!("cudaMalloc failed: {}", result)));
            }
            
            Ok(ptr)
        }
        
        fn free(&self, ptr: usize) -> GpuResult<()> {
            type CudaFree = unsafe extern "C" fn(usize) -> i32;
            
            let func: Symbol<CudaFree> = unsafe {
                self.lib.get(b"cudaFree\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let result = unsafe { func(ptr) };
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaFree failed: {}", result)));
            }
            
            Ok(())
        }
        
        fn copy_htod(&self, src: *const u8, dst: usize, size: usize) -> GpuResult<()> {
            type CudaMemcpy = unsafe extern "C" fn(usize, *const u8, usize, i32) -> i32;
            
            let func: Symbol<CudaMemcpy> = unsafe {
                self.lib.get(b"cudaMemcpy\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            // cudaMemcpyHostToDevice = 1
            let result = unsafe { func(dst, src, size, 1) };
            if result != 0 {
                return Err(GpuError::Transfer(format!("cudaMemcpy H2D failed: {}", result)));
            }
            
            Ok(())
        }
        
        fn copy_dtoh(&self, src: usize, dst: *mut u8, size: usize) -> GpuResult<()> {
            type CudaMemcpy = unsafe extern "C" fn(*mut u8, usize, usize, i32) -> i32;
            
            let func: Symbol<CudaMemcpy> = unsafe {
                self.lib.get(b"cudaMemcpy\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            // cudaMemcpyDeviceToHost = 2
            let result = unsafe { func(dst, src, size, 2) };
            if result != 0 {
                return Err(GpuError::Transfer(format!("cudaMemcpy D2H failed: {}", result)));
            }
            
            Ok(())
        }
        
        fn copy_dtod(&self, src: usize, dst: usize, size: usize) -> GpuResult<()> {
            type CudaMemcpy = unsafe extern "C" fn(usize, usize, usize, i32) -> i32;
            
            let func: Symbol<CudaMemcpy> = unsafe {
                self.lib.get(b"cudaMemcpy\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            // cudaMemcpyDeviceToDevice = 3
            let result = unsafe { func(dst, src, size, 3) };
            if result != 0 {
                return Err(GpuError::Transfer(format!("cudaMemcpy D2D failed: {}", result)));
            }
            
            Ok(())
        }
        
        fn synchronize(&self, stream: usize) -> GpuResult<()> {
            type CudaStreamSynchronize = unsafe extern "C" fn(usize) -> i32;
            
            let func: Symbol<CudaStreamSynchronize> = unsafe {
                self.lib.get(b"cudaStreamSynchronize\0").map_err(|e| {
                    GpuError::Cuda(e.to_string())
                })?
            };
            
            let result = unsafe { func(stream) };
            if result != 0 {
                return Err(GpuError::Cuda(format!("cudaStreamSynchronize failed: {}", result)));
            }
            
            Ok(())
        }
        
        fn compile_kernel(&self, source: &str, name: &str) -> GpuResult<CompiledKernel> {
            // This would use NVRTC to compile PTX
            // For now, return a stub
            Ok(CompiledKernel {
                name: name.to_string(),
                module: 0,
                function: 0,
                backend: BackendType::Cuda,
                source: source.to_string(),
            })
        }
        
        fn launch_kernel(
            &self,
            kernel: &CompiledKernel,
            grid: [u32; 3],
            block: [u32; 3],
            args: &[KernelArg],
            stream: usize,
        ) -> GpuResult<()> {
            // This would call cuLaunchKernel
            // For now, stub
            Ok(())
        }
    }
    
    fn get_cuda_device_info(index: usize) -> GpuResult<DeviceInfo> {
        // Use nvidia-smi to get device info
        let output = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total,driver_version,compute_cap", "--format=csv,noheader,nounits"])
            .output()
            .map_err(|e| GpuError::DriverNotFound(format!("nvidia-smi: {}", e)))?;
        
        if !output.status.success() {
            return Err(GpuError::DriverNotFound("nvidia-smi failed".to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.trim().lines().collect();
        
        if index >= lines.len() {
            return Err(GpuError::DeviceNotFound(format!("Device {} not found", index)));
        }
        
        let parts: Vec<&str> = lines[index].split(',').map(|s| s.trim()).collect();
        if parts.len() < 3 {
            return Err(GpuError::Cuda("Failed to parse nvidia-smi output".to_string()));
        }
        
        let name = parts[0].to_string();
        let memory: u64 = parts[1].parse().unwrap_or(0) * 1024 * 1024; // MB to bytes
        let driver_version = parts[2].to_string();
        
        let compute_capability = parts.get(3).and_then(|s| {
            let cc: Vec<&str> = s.split('.').collect();
            if cc.len() == 2 {
                Some((cc[0].parse().ok()?, cc[1].parse().ok()?))
            } else {
                None
            }
        });
        
        Ok(DeviceInfo {
            index,
            name,
            device_type: DeviceType::Cuda,
            vendor: "NVIDIA".to_string(),
            total_memory: memory,
            available_memory: memory,
            compute_capability,
            max_threads_per_block: 1024,
            max_block_dim: [1024, 1024, 64],
            max_grid_dim: [2147483647, 65535, 65535],
            warp_size: 32,
            multiprocessor_count: 0, // Would need cudaGetDeviceProperties
            clock_rate_mhz: 0,
            memory_clock_rate_mhz: 0,
            memory_bus_width: 0,
            l2_cache_size: 0,
            unified_memory: compute_capability.map(|(m, _)| m >= 6).unwrap_or(false),
            concurrent_kernels: true,
            driver_version,
        })
    }
}

#[cfg(feature = "cuda")]
pub use cuda::CudaBackend;

// =============================================================================
// CPU Backend (fallback)
// =============================================================================

/// CPU backend (fallback when no GPU available).
pub struct CpuBackend {
    allocations: std::sync::Mutex<std::collections::HashMap<usize, Vec<u8>>>,
    next_id: std::sync::atomic::AtomicUsize,
}

impl CpuBackend {
    pub fn new() -> Self {
        Self {
            allocations: std::sync::Mutex::new(std::collections::HashMap::new()),
            next_id: std::sync::atomic::AtomicUsize::new(1),
        }
    }
}

impl Backend for CpuBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Cpu
    }
    
    fn is_available(&self) -> bool {
        true
    }
    
    fn device_count(&self) -> GpuResult<usize> {
        Ok(1)
    }
    
    fn get_device_info(&self, _index: usize) -> GpuResult<DeviceInfo> {
        Ok(DeviceInfo {
            index: 0,
            name: "CPU".to_string(),
            device_type: DeviceType::Cpu,
            vendor: "Host".to_string(),
            total_memory: 8 * 1024 * 1024 * 1024,
            available_memory: 8 * 1024 * 1024 * 1024,
            compute_capability: None,
            max_threads_per_block: 1,
            max_block_dim: [1, 1, 1],
            max_grid_dim: [1, 1, 1],
            warp_size: 1,
            multiprocessor_count: std::thread::available_parallelism()
                .map(|p| p.get() as u32)
                .unwrap_or(4),
            clock_rate_mhz: 0,
            memory_clock_rate_mhz: 0,
            memory_bus_width: 0,
            l2_cache_size: 0,
            unified_memory: true,
            concurrent_kernels: true,
            driver_version: "N/A".to_string(),
        })
    }
    
    fn create_context(&self, _index: usize) -> GpuResult<DeviceContext> {
        Ok(DeviceContext {
            handle: 0,
            stream: 0,
            device_type: DeviceType::Cpu,
        })
    }
    
    fn destroy_context(&self, _context: &DeviceContext) -> GpuResult<()> {
        Ok(())
    }
    
    fn get_available_memory(&self, _index: usize) -> GpuResult<u64> {
        Ok(8 * 1024 * 1024 * 1024)
    }
    
    fn alloc(&self, size: usize) -> GpuResult<usize> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let buffer = vec![0u8; size];
        
        let mut allocs = self.allocations.lock().unwrap();
        allocs.insert(id, buffer);
        
        Ok(id)
    }
    
    fn free(&self, ptr: usize) -> GpuResult<()> {
        let mut allocs = self.allocations.lock().unwrap();
        allocs.remove(&ptr);
        Ok(())
    }
    
    fn copy_htod(&self, src: *const u8, dst: usize, size: usize) -> GpuResult<()> {
        let mut allocs = self.allocations.lock().unwrap();
        if let Some(buffer) = allocs.get_mut(&dst) {
            if buffer.len() >= size {
                unsafe {
                    std::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), size);
                }
                return Ok(());
            }
        }
        Err(GpuError::Transfer("Invalid destination buffer".to_string()))
    }
    
    fn copy_dtoh(&self, src: usize, dst: *mut u8, size: usize) -> GpuResult<()> {
        let allocs = self.allocations.lock().unwrap();
        if let Some(buffer) = allocs.get(&src) {
            if buffer.len() >= size {
                unsafe {
                    std::ptr::copy_nonoverlapping(buffer.as_ptr(), dst, size);
                }
                return Ok(());
            }
        }
        Err(GpuError::Transfer("Invalid source buffer".to_string()))
    }
    
    fn copy_dtod(&self, src: usize, dst: usize, size: usize) -> GpuResult<()> {
        let mut allocs = self.allocations.lock().unwrap();
        
        let src_data = allocs.get(&src)
            .ok_or_else(|| GpuError::Transfer("Invalid source".to_string()))?
            .clone();
        
        if let Some(dst_buffer) = allocs.get_mut(&dst) {
            if dst_buffer.len() >= size && src_data.len() >= size {
                dst_buffer[..size].copy_from_slice(&src_data[..size]);
                return Ok(());
            }
        }
        
        Err(GpuError::Transfer("Invalid buffers".to_string()))
    }
    
    fn synchronize(&self, _stream: usize) -> GpuResult<()> {
        Ok(())
    }
    
    fn compile_kernel(&self, source: &str, name: &str) -> GpuResult<CompiledKernel> {
        Ok(CompiledKernel {
            name: name.to_string(),
            module: 0,
            function: 0,
            backend: BackendType::Cpu,
            source: source.to_string(),
        })
    }
    
    fn launch_kernel(
        &self,
        _kernel: &CompiledKernel,
        _grid: [u32; 3],
        _block: [u32; 3],
        _args: &[KernelArg],
        _stream: usize,
    ) -> GpuResult<()> {
        // CPU fallback would execute sequentially
        Ok(())
    }
}

// =============================================================================
// OpenCL Backend (stub)
// =============================================================================

// Note: Full OpenCL implementation would require libloading and OpenCL FFI
// Similar to CUDA backend but with OpenCL API calls

