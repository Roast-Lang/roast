//! Roast GPU Compute Backend
//!
//! This crate provides GPU acceleration for Roast programs:
//! - Multi-backend support: CUDA, OpenCL, Metal, Vulkan
//! - Automatic device detection and selection
//! - Tensor operations with GPU acceleration
//! - Kernel compilation and execution
//! - Memory management with automatic transfers
//!
//! # Example
//!
//! ```roast
//! from roast.gpu import Device, Tensor, kernel
//!
//! device = Device.default()
//!
//! @kernel
//! def vector_add(a: Tensor[float], b: Tensor[float], c: Tensor[float]) -> None:
//!     idx = thread_idx()
//!     if idx < len(a):
//!         c[idx] = a[idx] + b[idx]
//!
//! a = Tensor.rand(1000, device=device)
//! b = Tensor.rand(1000, device=device)
//! c = Tensor.zeros(1000, device=device)
//!
//! vector_add[4, 256](a, b, c)
//! ```

pub mod device;
pub mod tensor;
pub mod kernel;
pub mod memory;
pub mod backend;
pub mod ops;
pub mod runtime;
pub mod compiler;
pub mod error;
pub mod nvrtc;
pub mod cuda;
pub mod cublas;
pub mod cudnn;
pub mod multi_gpu;
pub mod autograd;
pub mod cuda_mem;

pub use device::{Device, DeviceInfo, DeviceType};
pub use tensor::{Tensor, TensorView, TensorMut, DType};
pub use kernel::{Kernel, KernelBuilder, LaunchConfig};
pub use memory::{Buffer, BufferUsage, MemoryPool};
pub use backend::{Backend, BackendType};
pub use error::{GpuError, GpuResult};
pub use nvrtc::{NvrtcCompiler, NvrtcCompileOptions, JitCompiler, CudaKernel};
pub use cuda::{CudaBackend, templates as cuda_templates};
pub use cublas::{Cublas, BlasOps, CublasOperation};
pub use cudnn::{Cudnn, DnnOps};
pub use multi_gpu::{MultiGpu, NcclOps, DataParallel, PeerToPeer};
pub use autograd::{Variable, Optimizer, SGD, Adam, NoGrad, no_grad};
pub use cuda_mem::{
    CudaMemLib, CachingAllocator, DeviceMemory, PinnedHostMemory, UnifiedMemory,
    init_allocator, cuda_alloc, cuda_free, cuda_mem_info, cuda_empty_cache,
};

/// GPU crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the GPU runtime.
pub fn init() -> GpuResult<()> {
    runtime::init()
}

/// Shutdown the GPU runtime.
pub fn shutdown() {
    runtime::shutdown()
}

/// Get the default GPU device.
pub fn default_device() -> GpuResult<Device> {
    Device::default()
}

/// List all available GPU devices.
pub fn list_devices() -> GpuResult<Vec<DeviceInfo>> {
    Device::list_all()
}

/// Synchronize all GPU operations.
pub fn synchronize() -> GpuResult<()> {
    runtime::synchronize()
}

