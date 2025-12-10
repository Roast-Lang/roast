//! GPU kernel compilation and execution.

use std::collections::HashMap;
use std::sync::Arc;
use crate::backend::{BackendType, KernelArg};
use crate::device::Device;
use crate::error::{GpuError, GpuResult};

/// Compiled GPU kernel.
#[derive(Debug, Clone)]
pub struct CompiledKernel {
    /// Kernel name.
    pub name: String,
    
    /// Module handle.
    pub module: usize,
    
    /// Function handle.
    pub function: usize,
    
    /// Backend type.
    pub backend: BackendType,
    
    /// Source code.
    pub source: String,
}

/// Kernel launch configuration.
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// Grid dimensions (blocks).
    pub grid: [u32; 3],
    
    /// Block dimensions (threads).
    pub block: [u32; 3],
    
    /// Shared memory size.
    pub shared_mem: u32,
    
    /// Stream handle.
    pub stream: usize,
}

impl LaunchConfig {
    /// Create 1D launch config.
    pub fn linear(n: u32, block_size: u32) -> Self {
        let grid = (n + block_size - 1) / block_size;
        Self {
            grid: [grid, 1, 1],
            block: [block_size, 1, 1],
            shared_mem: 0,
            stream: 0,
        }
    }
    
    /// Create 2D launch config.
    pub fn d2(nx: u32, ny: u32, block_x: u32, block_y: u32) -> Self {
        let grid_x = (nx + block_x - 1) / block_x;
        let grid_y = (ny + block_y - 1) / block_y;
        Self {
            grid: [grid_x, grid_y, 1],
            block: [block_x, block_y, 1],
            shared_mem: 0,
            stream: 0,
        }
    }
    
    /// Create 3D launch config.
    pub fn d3(nx: u32, ny: u32, nz: u32, block_x: u32, block_y: u32, block_z: u32) -> Self {
        let grid_x = (nx + block_x - 1) / block_x;
        let grid_y = (ny + block_y - 1) / block_y;
        let grid_z = (nz + block_z - 1) / block_z;
        Self {
            grid: [grid_x, grid_y, grid_z],
            block: [block_x, block_y, block_z],
            shared_mem: 0,
            stream: 0,
        }
    }
    
    /// Set shared memory size.
    pub fn with_shared_mem(mut self, size: u32) -> Self {
        self.shared_mem = size;
        self
    }
    
    /// Set stream.
    pub fn with_stream(mut self, stream: usize) -> Self {
        self.stream = stream;
        self
    }
    
    /// Total number of threads.
    pub fn total_threads(&self) -> u64 {
        self.grid[0] as u64 * self.grid[1] as u64 * self.grid[2] as u64 *
        self.block[0] as u64 * self.block[1] as u64 * self.block[2] as u64
    }
}

/// Kernel builder.
pub struct KernelBuilder {
    /// Kernel name.
    name: String,
    
    /// Source code.
    source: String,
    
    /// Parameters.
    params: Vec<KernelParam>,
    
    /// Target backend.
    backend: Option<BackendType>,
}

/// Kernel parameter.
#[derive(Debug, Clone)]
pub struct KernelParam {
    pub name: String,
    pub dtype: String,
    pub is_ptr: bool,
}

impl KernelBuilder {
    /// Create a new kernel builder.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            source: String::new(),
            params: Vec::new(),
            backend: None,
        }
    }
    
    /// Add a parameter.
    pub fn param(mut self, name: &str, dtype: &str, is_ptr: bool) -> Self {
        self.params.push(KernelParam {
            name: name.to_string(),
            dtype: dtype.to_string(),
            is_ptr,
        });
        self
    }
    
    /// Set source code.
    pub fn source(mut self, source: &str) -> Self {
        self.source = source.to_string();
        self
    }
    
    /// Set target backend.
    pub fn backend(mut self, backend: BackendType) -> Self {
        self.backend = Some(backend);
        self
    }
    
    /// Build for CUDA.
    pub fn cuda_source(mut self, source: &str) -> Self {
        self.source = source.to_string();
        self.backend = Some(BackendType::Cuda);
        self
    }
    
    /// Build for OpenCL.
    pub fn opencl_source(mut self, source: &str) -> Self {
        self.source = source.to_string();
        self.backend = Some(BackendType::OpenCL);
        self
    }
    
    /// Compile the kernel.
    pub fn build(self, device: &Device) -> GpuResult<Kernel> {
        let backend = self.backend.unwrap_or(device.device_type().into());
        let source = self.generate_source(backend)?;
        let compiled = device.backend().compile_kernel(&source, &self.name)?;
        
        Ok(Kernel {
            compiled,
            device: device.clone(),
            params: self.params,
        })
    }
    
    fn generate_source(&self, backend: BackendType) -> GpuResult<String> {
        if !self.source.is_empty() {
            return Ok(self.source.clone());
        }
        
        // Generate source from params
        match backend {
            BackendType::Cuda => self.generate_cuda_source(),
            BackendType::OpenCL => self.generate_opencl_source(),
            _ => Err(GpuError::Unsupported(
                format!("Kernel generation for {:?}", backend)
            )),
        }
    }
    
    fn generate_cuda_source(&self) -> GpuResult<String> {
        let mut params_str = String::new();
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 {
                params_str.push_str(", ");
            }
            if p.is_ptr {
                params_str.push_str(&format!("{} *{}", p.dtype, p.name));
            } else {
                params_str.push_str(&format!("{} {}", p.dtype, p.name));
            }
        }
        
        Ok(format!(
            r#"extern "C" __global__ void {}({}) {{
    // TODO: Generated kernel body
}}
"#, self.name, params_str))
    }
    
    fn generate_opencl_source(&self) -> GpuResult<String> {
        let mut params_str = String::new();
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 {
                params_str.push_str(", ");
            }
            if p.is_ptr {
                params_str.push_str(&format!("__global {} *{}", p.dtype, p.name));
            } else {
                params_str.push_str(&format!("{} {}", p.dtype, p.name));
            }
        }
        
        Ok(format!(
            r#"__kernel void {}({}) {{
    // TODO: Generated kernel body
}}
"#, self.name, params_str))
    }
}

impl From<crate::device::DeviceType> for BackendType {
    fn from(dt: crate::device::DeviceType) -> Self {
        match dt {
            crate::device::DeviceType::Cuda => BackendType::Cuda,
            crate::device::DeviceType::OpenCL => BackendType::OpenCL,
            crate::device::DeviceType::Metal => BackendType::Metal,
            crate::device::DeviceType::Vulkan => BackendType::Vulkan,
            crate::device::DeviceType::Cpu => BackendType::Cpu,
        }
    }
}

/// A compiled and launchable kernel.
pub struct Kernel {
    /// Compiled kernel.
    compiled: CompiledKernel,
    
    /// Device.
    device: Device,
    
    /// Parameters.
    params: Vec<KernelParam>,
}

impl Kernel {
    /// Launch the kernel.
    pub fn launch(&self, config: &LaunchConfig, args: &[KernelArg]) -> GpuResult<()> {
        self.device.backend().launch_kernel(
            &self.compiled,
            config.grid,
            config.block,
            args,
            config.stream,
        )
    }
    
    /// Get kernel name.
    pub fn name(&self) -> &str {
        &self.compiled.name
    }
}

/// Kernel cache.
pub struct KernelCache {
    cache: HashMap<String, Arc<CompiledKernel>>,
}

impl KernelCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    
    pub fn get(&self, key: &str) -> Option<Arc<CompiledKernel>> {
        self.cache.get(key).cloned()
    }
    
    pub fn insert(&mut self, key: String, kernel: CompiledKernel) {
        self.cache.insert(key, Arc::new(kernel));
    }
    
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for KernelCache {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Built-in kernels
// =============================================================================

/// Built-in CUDA kernels.
pub mod cuda_kernels {
    /// Element-wise add kernel.
    pub const ADD: &str = r#"
extern "C" __global__ void add_kernel(
    const float *a, const float *b, float *c, int n
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        c[idx] = a[idx] + b[idx];
    }
}
"#;

    /// Element-wise multiply kernel.
    pub const MUL: &str = r#"
extern "C" __global__ void mul_kernel(
    const float *a, const float *b, float *c, int n
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        c[idx] = a[idx] * b[idx];
    }
}
"#;

    /// Scalar add kernel.
    pub const SCALAR_ADD: &str = r#"
extern "C" __global__ void scalar_add_kernel(
    const float *a, float scalar, float *c, int n
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        c[idx] = a[idx] + scalar;
    }
}
"#;

    /// Matrix multiply kernel (naive).
    pub const MATMUL: &str = r#"
extern "C" __global__ void matmul_kernel(
    const float *a, const float *b, float *c,
    int M, int N, int K
) {
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    
    if (row < M && col < N) {
        float sum = 0.0f;
        for (int k = 0; k < K; ++k) {
            sum += a[row * K + k] * b[k * N + col];
        }
        c[row * N + col] = sum;
    }
}
"#;

    /// Reduction sum kernel.
    pub const REDUCE_SUM: &str = r#"
extern "C" __global__ void reduce_sum_kernel(
    const float *input, float *output, int n
) {
    extern __shared__ float sdata[];
    
    int tid = threadIdx.x;
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    
    sdata[tid] = (idx < n) ? input[idx] : 0.0f;
    __syncthreads();
    
    for (int s = blockDim.x / 2; s > 0; s >>= 1) {
        if (tid < s) {
            sdata[tid] += sdata[tid + s];
        }
        __syncthreads();
    }
    
    if (tid == 0) {
        atomicAdd(output, sdata[0]);
    }
}
"#;

    /// ReLU activation.
    pub const RELU: &str = r#"
extern "C" __global__ void relu_kernel(
    const float *input, float *output, int n
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        output[idx] = fmaxf(0.0f, input[idx]);
    }
}
"#;

    /// Softmax kernel.
    pub const SOFTMAX: &str = r#"
extern "C" __global__ void softmax_kernel(
    const float *input, float *output, int n
) {
    extern __shared__ float sdata[];
    
    int tid = threadIdx.x;
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    
    // Find max
    float max_val = -INFINITY;
    for (int i = tid; i < n; i += blockDim.x) {
        max_val = fmaxf(max_val, input[i]);
    }
    sdata[tid] = max_val;
    __syncthreads();
    
    for (int s = blockDim.x / 2; s > 0; s >>= 1) {
        if (tid < s) {
            sdata[tid] = fmaxf(sdata[tid], sdata[tid + s]);
        }
        __syncthreads();
    }
    max_val = sdata[0];
    __syncthreads();
    
    // Compute exp(x - max) and sum
    float sum = 0.0f;
    for (int i = tid; i < n; i += blockDim.x) {
        float exp_val = expf(input[i] - max_val);
        output[i] = exp_val;
        sum += exp_val;
    }
    sdata[tid] = sum;
    __syncthreads();
    
    for (int s = blockDim.x / 2; s > 0; s >>= 1) {
        if (tid < s) {
            sdata[tid] += sdata[tid + s];
        }
        __syncthreads();
    }
    sum = sdata[0];
    __syncthreads();
    
    // Normalize
    for (int i = tid; i < n; i += blockDim.x) {
        output[i] /= sum;
    }
}
"#;
}

