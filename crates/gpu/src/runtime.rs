//! GPU runtime initialization and management.

use std::sync::{Arc, Once, RwLock};
use crate::device::{Device, DeviceInfo};
use crate::backend::{Backend, BackendType};
use crate::kernel::KernelCache;
use crate::error::{GpuError, GpuResult};

static INIT: Once = Once::new();
static RUNTIME: RwLock<Option<GpuRuntime>> = RwLock::new(None);

/// GPU runtime state.
pub struct GpuRuntime {
    /// Available backends.
    backends: Vec<BackendType>,
    
    /// Default device.
    default_device: Option<Device>,
    
    /// Kernel cache.
    kernel_cache: KernelCache,
    
    /// Is initialized.
    initialized: bool,
}

impl GpuRuntime {
    /// Create a new runtime.
    fn new() -> Self {
        Self {
            backends: Vec::new(),
            default_device: None,
            kernel_cache: KernelCache::new(),
            initialized: false,
        }
    }
    
    /// Initialize the runtime.
    fn init(&mut self) -> GpuResult<()> {
        if self.initialized {
            return Ok(());
        }
        
        // Detect available backends
        self.backends = detect_backends();
        
        // Try to get default device
        if let Ok(device) = Device::default() {
            self.default_device = Some(device);
        }
        
        self.initialized = true;
        Ok(())
    }
    
    /// Get available backends.
    pub fn available_backends(&self) -> &[BackendType] {
        &self.backends
    }
    
    /// Get default device.
    pub fn default_device(&self) -> Option<&Device> {
        self.default_device.as_ref()
    }
}

/// Initialize the GPU runtime.
pub fn init() -> GpuResult<()> {
    let mut result = Ok(());
    
    INIT.call_once(|| {
        let mut runtime = RUNTIME.write().unwrap();
        let mut rt = GpuRuntime::new();
        result = rt.init();
        *runtime = Some(rt);
    });
    
    result
}

/// Shutdown the GPU runtime.
pub fn shutdown() {
    let mut runtime = RUNTIME.write().unwrap();
    *runtime = None;
}

/// Check if runtime is initialized.
pub fn is_initialized() -> bool {
    RUNTIME.read().unwrap().is_some()
}

/// Synchronize all devices.
pub fn synchronize() -> GpuResult<()> {
    let runtime = RUNTIME.read().unwrap();
    if let Some(ref rt) = *runtime {
        if let Some(ref device) = rt.default_device {
            device.synchronize()?;
        }
    }
    Ok(())
}

/// Get the default device.
pub fn get_default_device() -> GpuResult<Device> {
    Device::default()
}

/// List all available devices.
pub fn list_devices() -> GpuResult<Vec<DeviceInfo>> {
    Device::list_all()
}

/// Detect available backends.
fn detect_backends() -> Vec<BackendType> {
    let mut backends = Vec::new();
    
    // Check CUDA
    if is_cuda_available() {
        backends.push(BackendType::Cuda);
    }
    
    // Check OpenCL
    if is_opencl_available() {
        backends.push(BackendType::OpenCL);
    }
    
    // Check Metal (macOS)
    #[cfg(target_os = "macos")]
    if is_metal_available() {
        backends.push(BackendType::Metal);
    }
    
    // Check Vulkan
    if is_vulkan_available() {
        backends.push(BackendType::Vulkan);
    }
    
    // CPU always available
    backends.push(BackendType::Cpu);
    
    backends
}

/// Check if CUDA is available.
fn is_cuda_available() -> bool {
    // Check for nvidia-smi
    which::which("nvidia-smi").is_ok()
}

/// Check if OpenCL is available.
fn is_opencl_available() -> bool {
    // Check for clinfo
    which::which("clinfo").is_ok()
}

/// Check if Metal is available.
#[cfg(target_os = "macos")]
fn is_metal_available() -> bool {
    // Metal is always available on macOS 10.11+
    true
}

/// Check if Vulkan is available.
fn is_vulkan_available() -> bool {
    // Check for vulkaninfo
    which::which("vulkaninfo").is_ok()
}

/// Runtime statistics.
#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    /// Total allocations.
    pub total_allocations: u64,
    
    /// Total bytes allocated.
    pub total_bytes_allocated: u64,
    
    /// Total kernel launches.
    pub total_kernel_launches: u64,
    
    /// Total memory transfers.
    pub total_transfers: u64,
    
    /// Transfer bytes host to device.
    pub bytes_htod: u64,
    
    /// Transfer bytes device to host.
    pub bytes_dtoh: u64,
}

/// Get runtime statistics.
pub fn get_stats() -> RuntimeStats {
    RuntimeStats::default()
}

/// Reset runtime statistics.
pub fn reset_stats() {
    // TODO: Implement
}

/// CUDA context manager.
#[cfg(feature = "cuda")]
pub mod cuda {
    use super::*;
    
    /// Set current CUDA device.
    pub fn set_device(device_id: i32) -> GpuResult<()> {
        // Would call cudaSetDevice
        Ok(())
    }
    
    /// Get current CUDA device.
    pub fn get_device() -> GpuResult<i32> {
        Ok(0)
    }
    
    /// Enable peer access between devices.
    pub fn enable_peer_access(device: i32, peer: i32) -> GpuResult<()> {
        // Would call cudaDeviceEnablePeerAccess
        Ok(())
    }
    
    /// Disable peer access.
    pub fn disable_peer_access(peer: i32) -> GpuResult<()> {
        Ok(())
    }
    
    /// Check if peer access is possible.
    pub fn can_access_peer(device: i32, peer: i32) -> GpuResult<bool> {
        Ok(false)
    }
}

/// Stream management.
pub mod stream {
    use super::*;
    
    /// Stream handle.
    pub struct Stream {
        handle: usize,
        device: usize,
    }
    
    impl Stream {
        /// Create a new stream.
        pub fn new(_device: &Device) -> GpuResult<Self> {
            Ok(Self {
                handle: 0,
                device: 0,
            })
        }
        
        /// Get the default stream.
        pub fn default_stream() -> Self {
            Self {
                handle: 0,
                device: 0,
            }
        }
        
        /// Synchronize the stream.
        pub fn synchronize(&self) -> GpuResult<()> {
            Ok(())
        }
        
        /// Get handle.
        pub fn handle(&self) -> usize {
            self.handle
        }
    }
    
    /// Event handle for timing.
    pub struct Event {
        handle: usize,
    }
    
    impl Event {
        /// Create a new event.
        pub fn new() -> GpuResult<Self> {
            Ok(Self { handle: 0 })
        }
        
        /// Record event in stream.
        pub fn record(&self, _stream: &Stream) -> GpuResult<()> {
            Ok(())
        }
        
        /// Synchronize event.
        pub fn synchronize(&self) -> GpuResult<()> {
            Ok(())
        }
        
        /// Elapsed time between two events in milliseconds.
        pub fn elapsed_time(start: &Event, end: &Event) -> GpuResult<f32> {
            Ok(0.0)
        }
    }
    
    impl Default for Event {
        fn default() -> Self {
            Self::new().unwrap()
        }
    }
}

/// Profiling utilities.
pub mod profiler {
    use super::*;
    use std::time::Instant;
    
    /// GPU profiler.
    pub struct GpuProfiler {
        events: Vec<(String, f32)>,
        start_time: Option<Instant>,
        current_label: Option<String>,
    }
    
    impl GpuProfiler {
        /// Create a new profiler.
        pub fn new() -> Self {
            Self {
                events: Vec::new(),
                start_time: None,
                current_label: None,
            }
        }
        
        /// Start profiling a region.
        pub fn start(&mut self, label: &str) {
            self.current_label = Some(label.to_string());
            self.start_time = Some(Instant::now());
        }
        
        /// Stop profiling current region.
        pub fn stop(&mut self) {
            if let (Some(label), Some(start)) = (self.current_label.take(), self.start_time.take()) {
                let elapsed = start.elapsed().as_secs_f32() * 1000.0;
                self.events.push((label, elapsed));
            }
        }
        
        /// Get results.
        pub fn results(&self) -> &[(String, f32)] {
            &self.events
        }
        
        /// Print results.
        pub fn print_summary(&self) {
            println!("GPU Profiling Summary:");
            println!("{:-<50}", "");
            for (label, time) in &self.events {
                println!("{:40} {:>8.3} ms", label, time);
            }
            println!("{:-<50}", "");
            let total: f32 = self.events.iter().map(|(_, t)| t).sum();
            println!("{:40} {:>8.3} ms", "Total", total);
        }
        
        /// Clear results.
        pub fn clear(&mut self) {
            self.events.clear();
        }
    }
    
    impl Default for GpuProfiler {
        fn default() -> Self {
            Self::new()
        }
    }
}

