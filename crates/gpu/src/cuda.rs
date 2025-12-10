//! CUDA-specific GPU backend using NVRTC and CUDA Driver API.

use std::sync::Arc;
use std::ptr;
use crate::device::{DeviceInfo, DeviceType, DeviceContext};
use crate::kernel::CompiledKernel;
use crate::backend::{Backend, BackendType, KernelArg};
use crate::nvrtc::{NvrtcCompiler, NvrtcCompileOptions, CudaDriverLib};
use crate::error::{GpuError, GpuResult};

/// Full CUDA backend with NVRTC support.
pub struct CudaBackend {
    /// NVRTC compiler.
    compiler: NvrtcCompiler,
    
    /// Current device.
    device_id: i32,
    
    /// CUDA context.
    context: usize,
    
    /// Device info cache.
    device_info: Option<DeviceInfo>,
}

impl CudaBackend {
    /// Create a new CUDA backend.
    pub fn new() -> GpuResult<Self> {
        let compiler = NvrtcCompiler::new()?;
        
        // Get default device
        let device_id = compiler.driver().get_device(0)?;
        
        // Create context
        let context = compiler.driver().create_context(device_id)?;
        
        Ok(Self {
            compiler,
            device_id,
            context,
            device_info: None,
        })
    }
    
    /// Create for a specific device.
    pub fn new_for_device(device_id: i32) -> GpuResult<Self> {
        let compiler = NvrtcCompiler::new()?;
        let context = compiler.driver().create_context(device_id)?;
        
        Ok(Self {
            compiler,
            device_id,
            context,
            device_info: None,
        })
    }
    
    /// Get the NVRTC compiler.
    pub fn compiler(&self) -> &NvrtcCompiler {
        &self.compiler
    }
    
    /// Get CUDA driver.
    pub fn driver(&self) -> &CudaDriverLib {
        self.compiler.driver()
    }
    
    /// Compile CUDA source to PTX.
    pub fn compile_ptx(
        &self,
        source: &str,
        options: Option<&NvrtcCompileOptions>,
    ) -> GpuResult<Vec<u8>> {
        let result = self.compiler.compile(source, "kernel.cu", options)?;
        Ok(result.ptx)
    }
    
    /// Compile and load a kernel.
    pub fn compile_kernel(
        &self,
        source: &str,
        kernel_name: &str,
        options: Option<&NvrtcCompileOptions>,
    ) -> GpuResult<LoadedKernel> {
        let cuda_kernel = self.compiler.compile_and_load(source, kernel_name, options)?;
        
        Ok(LoadedKernel {
            inner: cuda_kernel,
            driver: self.compiler.driver(),
        })
    }
}

impl Backend for CudaBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Cuda
    }
    
    fn is_available(&self) -> bool {
        true
    }
    
    fn device_count(&self) -> GpuResult<usize> {
        // Use nvidia-smi to get device count
        let output = std::process::Command::new("nvidia-smi")
            .args(["--list-gpus"])
            .output()
            .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().count())
    }
    
    fn get_device_info(&self, index: usize) -> GpuResult<DeviceInfo> {
        // Query nvidia-smi
        let output = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total,driver_version,compute_cap,clocks.sm,clocks.mem",
                "--format=csv,noheader,nounits",
                "-i", &index.to_string(),
            ])
            .output()
            .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
        
        if !output.status.success() {
            return Err(GpuError::DeviceNotFound(format!("Device {} not found", index)));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split(',').map(|s| s.trim()).collect();
        
        if parts.len() < 4 {
            return Err(GpuError::Cuda("Failed to parse nvidia-smi output".to_string()));
        }
        
        let name = parts[0].to_string();
        let memory: u64 = parts[1].parse().unwrap_or(0) * 1024 * 1024;
        let driver_version = parts[2].to_string();
        
        let compute_capability = parts.get(3).and_then(|s| {
            let cc: Vec<&str> = s.split('.').collect();
            if cc.len() == 2 {
                Some((cc[0].parse().ok()?, cc[1].parse().ok()?))
            } else {
                None
            }
        });
        
        let clock_rate: u32 = parts.get(4)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        
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
            multiprocessor_count: 0,
            clock_rate_mhz: clock_rate,
            memory_clock_rate_mhz: parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0),
            memory_bus_width: 0,
            l2_cache_size: 0,
            unified_memory: compute_capability.map(|(m, _)| m >= 6).unwrap_or(false),
            concurrent_kernels: true,
            driver_version,
        })
    }
    
    fn create_context(&self, index: usize) -> GpuResult<DeviceContext> {
        let device = self.driver().get_device(index as i32)?;
        let ctx = self.driver().create_context(device)?;
        
        Ok(DeviceContext {
            handle: ctx,
            stream: 0,
            device_type: DeviceType::Cuda,
        })
    }
    
    fn destroy_context(&self, context: &DeviceContext) -> GpuResult<()> {
        self.driver().destroy_context(context.handle)
    }
    
    fn get_available_memory(&self, _index: usize) -> GpuResult<u64> {
        // This would need cuMemGetInfo
        Ok(0)
    }
    
    fn alloc(&self, _size: usize) -> GpuResult<usize> {
        // This would need cuMemAlloc
        Err(GpuError::Unsupported("Direct allocation not yet implemented".to_string()))
    }
    
    fn free(&self, _ptr: usize) -> GpuResult<()> {
        Err(GpuError::Unsupported("Direct free not yet implemented".to_string()))
    }
    
    fn copy_htod(&self, _src: *const u8, _dst: usize, _size: usize) -> GpuResult<()> {
        Err(GpuError::Unsupported("copy_htod not yet implemented".to_string()))
    }
    
    fn copy_dtoh(&self, _src: usize, _dst: *mut u8, _size: usize) -> GpuResult<()> {
        Err(GpuError::Unsupported("copy_dtoh not yet implemented".to_string()))
    }
    
    fn copy_dtod(&self, _src: usize, _dst: usize, _size: usize) -> GpuResult<()> {
        Err(GpuError::Unsupported("copy_dtod not yet implemented".to_string()))
    }
    
    fn synchronize(&self, stream: usize) -> GpuResult<()> {
        self.driver().stream_synchronize(stream)
    }
    
    fn compile_kernel(&self, source: &str, name: &str) -> GpuResult<CompiledKernel> {
        let result = self.compiler.compile(source, "kernel.cu", None)?;
        let kernel = self.compiler.load_kernel(&result.ptx, name)?;
        
        Ok(CompiledKernel {
            name: name.to_string(),
            module: kernel.module(),
            function: kernel.function(),
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
        // Convert args to raw pointers
        let mut arg_values: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes()).collect();
        let mut arg_ptrs: Vec<*mut std::ffi::c_void> = arg_values
            .iter_mut()
            .map(|v| v.as_mut_ptr() as *mut std::ffi::c_void)
            .collect();
        
        self.driver().launch_kernel(
            kernel.function,
            grid,
            block,
            0, // shared mem
            stream,
            &mut arg_ptrs,
        )
    }
}

impl Drop for CudaBackend {
    fn drop(&mut self) {
        if self.context != 0 {
            let _ = self.driver().destroy_context(self.context);
        }
    }
}

/// A loaded and executable kernel.
pub struct LoadedKernel<'a> {
    inner: crate::nvrtc::CudaKernel,
    driver: &'a CudaDriverLib,
}

impl<'a> LoadedKernel<'a> {
    /// Get kernel name.
    pub fn name(&self) -> &str {
        self.inner.name()
    }
    
    /// Launch the kernel.
    pub fn launch(
        &self,
        grid: [u32; 3],
        block: [u32; 3],
        args: &[KernelArg],
    ) -> GpuResult<()> {
        let mut arg_values: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes()).collect();
        let mut arg_ptrs: Vec<*mut std::ffi::c_void> = arg_values
            .iter_mut()
            .map(|v| v.as_mut_ptr() as *mut std::ffi::c_void)
            .collect();
        
        self.inner.launch(self.driver, grid, block, 0, 0, &mut arg_ptrs)
    }
    
    /// Launch with shared memory.
    pub fn launch_with_shared(
        &self,
        grid: [u32; 3],
        block: [u32; 3],
        shared_mem: u32,
        stream: usize,
        args: &[KernelArg],
    ) -> GpuResult<()> {
        let mut arg_values: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes()).collect();
        let mut arg_ptrs: Vec<*mut std::ffi::c_void> = arg_values
            .iter_mut()
            .map(|v| v.as_mut_ptr() as *mut std::ffi::c_void)
            .collect();
        
        self.inner.launch(self.driver, grid, block, shared_mem, stream, &mut arg_ptrs)
    }
}

// =============================================================================
// Convenience macros and utilities
// =============================================================================

/// Create a simple kernel from source.
#[macro_export]
macro_rules! cuda_kernel {
    ($name:ident, $source:expr) => {
        lazy_static::lazy_static! {
            static ref $name: std::sync::Mutex<Option<$crate::nvrtc::CudaKernel>> = std::sync::Mutex::new(None);
        }
    };
}

/// Common CUDA kernel templates.
pub mod templates {
    /// Vector add kernel template.
    pub fn vector_add(dtype: &str) -> String {
        format!(r#"
extern "C" __global__ void vector_add(
    const {dtype} *a, const {dtype} *b, {dtype} *c, int n
) {{
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {{
        c[idx] = a[idx] + b[idx];
    }}
}}
"#, dtype = dtype)
    }
    
    /// Vector multiply kernel template.
    pub fn vector_mul(dtype: &str) -> String {
        format!(r#"
extern "C" __global__ void vector_mul(
    const {dtype} *a, const {dtype} *b, {dtype} *c, int n
) {{
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {{
        c[idx] = a[idx] * b[idx];
    }}
}}
"#, dtype = dtype)
    }
    
    /// Scalar multiply kernel template.
    pub fn scalar_mul(dtype: &str) -> String {
        format!(r#"
extern "C" __global__ void scalar_mul(
    const {dtype} *a, {dtype} scalar, {dtype} *c, int n
) {{
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {{
        c[idx] = a[idx] * scalar;
    }}
}}
"#, dtype = dtype)
    }
    
    /// SAXPY kernel template.
    pub fn saxpy(dtype: &str) -> String {
        format!(r#"
extern "C" __global__ void saxpy(
    {dtype} alpha, const {dtype} *x, const {dtype} *y, {dtype} *z, int n
) {{
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {{
        z[idx] = alpha * x[idx] + y[idx];
    }}
}}
"#, dtype = dtype)
    }
    
    /// ReLU activation kernel template.
    pub fn relu(dtype: &str) -> String {
        format!(r#"
extern "C" __global__ void relu(
    const {dtype} *input, {dtype} *output, int n
) {{
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {{
        {dtype} x = input[idx];
        output[idx] = x > 0 ? x : 0;
    }}
}}
"#, dtype = dtype)
    }
    
    /// Sigmoid activation kernel template.
    pub fn sigmoid() -> String {
        r#"
extern "C" __global__ void sigmoid(
    const float *input, float *output, int n
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        output[idx] = 1.0f / (1.0f + expf(-input[idx]));
    }
}
"#.to_string()
    }
    
    /// Matrix multiplication kernel (naive).
    pub fn matmul(dtype: &str) -> String {
        format!(r#"
extern "C" __global__ void matmul(
    const {dtype} *A, const {dtype} *B, {dtype} *C,
    int M, int N, int K
) {{
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    
    if (row < M && col < N) {{
        {dtype} sum = 0;
        for (int k = 0; k < K; ++k) {{
            sum += A[row * K + k] * B[k * N + col];
        }}
        C[row * N + col] = sum;
    }}
}}
"#, dtype = dtype)
    }
    
    /// Tiled matrix multiplication kernel.
    pub fn matmul_tiled(tile_size: usize) -> String {
        format!(r#"
#define TILE_SIZE {tile_size}

extern "C" __global__ void matmul_tiled(
    const float *A, const float *B, float *C,
    int M, int N, int K
) {{
    __shared__ float As[TILE_SIZE][TILE_SIZE];
    __shared__ float Bs[TILE_SIZE][TILE_SIZE];
    
    int bx = blockIdx.x;
    int by = blockIdx.y;
    int tx = threadIdx.x;
    int ty = threadIdx.y;
    
    int row = by * TILE_SIZE + ty;
    int col = bx * TILE_SIZE + tx;
    
    float sum = 0.0f;
    
    for (int t = 0; t < (K + TILE_SIZE - 1) / TILE_SIZE; ++t) {{
        if (row < M && t * TILE_SIZE + tx < K) {{
            As[ty][tx] = A[row * K + t * TILE_SIZE + tx];
        }} else {{
            As[ty][tx] = 0.0f;
        }}
        
        if (col < N && t * TILE_SIZE + ty < K) {{
            Bs[ty][tx] = B[(t * TILE_SIZE + ty) * N + col];
        }} else {{
            Bs[ty][tx] = 0.0f;
        }}
        
        __syncthreads();
        
        for (int k = 0; k < TILE_SIZE; ++k) {{
            sum += As[ty][k] * Bs[k][tx];
        }}
        
        __syncthreads();
    }}
    
    if (row < M && col < N) {{
        C[row * N + col] = sum;
    }}
}}
"#, tile_size = tile_size)
    }
    
    /// Reduction sum kernel.
    pub fn reduce_sum(block_size: usize) -> String {
        format!(r#"
extern "C" __global__ void reduce_sum(
    const float *input, float *output, int n
) {{
    __shared__ float sdata[{block_size}];
    
    unsigned int tid = threadIdx.x;
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    
    sdata[tid] = (i < n) ? input[i] : 0.0f;
    __syncthreads();
    
    for (unsigned int s = blockDim.x / 2; s > 0; s >>= 1) {{
        if (tid < s) {{
            sdata[tid] += sdata[tid + s];
        }}
        __syncthreads();
    }}
    
    if (tid == 0) {{
        atomicAdd(output, sdata[0]);
    }}
}}
"#, block_size = block_size)
    }
}

