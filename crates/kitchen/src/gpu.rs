//! GPU detection and configuration.

use std::process::Command;
use std::path::PathBuf;
use crate::{Error, Result};

/// GPU information.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU backend type.
    pub backend: GpuBackend,
    
    /// Device name.
    pub device_name: String,
    
    /// Device vendor.
    pub vendor: GpuVendor,
    
    /// Total memory in bytes.
    pub memory: u64,
    
    /// Compute capability (for CUDA).
    pub compute_capability: Option<(u32, u32)>,
    
    /// Driver version.
    pub driver_version: Option<String>,
    
    /// CUDA toolkit path (if applicable).
    pub cuda_path: Option<PathBuf>,
    
    /// OpenCL library path (if applicable).
    pub opencl_path: Option<PathBuf>,
}

/// GPU backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// NVIDIA CUDA.
    Cuda,
    /// OpenCL (cross-platform).
    OpenCL,
    /// Apple Metal.
    Metal,
    /// Vulkan Compute.
    Vulkan,
    /// CPU fallback.
    Cpu,
}

impl GpuBackend {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cuda => "CUDA",
            Self::OpenCL => "OpenCL",
            Self::Metal => "Metal",
            Self::Vulkan => "Vulkan",
            Self::Cpu => "CPU",
        }
    }
}

/// GPU vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
}

impl GpuVendor {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Nvidia => "NVIDIA",
            Self::Amd => "AMD",
            Self::Intel => "Intel",
            Self::Apple => "Apple",
            Self::Other => "Unknown",
        }
    }
}

impl GpuInfo {
    /// Detect GPU on the system.
    pub fn detect() -> Result<Self> {
        // Try CUDA first (NVIDIA)
        if let Ok(info) = Self::detect_cuda() {
            return Ok(info);
        }
        
        // Try Metal (macOS)
        #[cfg(target_os = "macos")]
        if let Ok(info) = Self::detect_metal() {
            return Ok(info);
        }
        
        // Try OpenCL
        if let Ok(info) = Self::detect_opencl() {
            return Ok(info);
        }
        
        // Try Vulkan
        if let Ok(info) = Self::detect_vulkan() {
            return Ok(info);
        }
        
        // CPU fallback
        Ok(Self::cpu_fallback())
    }
    
    /// Detect NVIDIA CUDA GPU.
    fn detect_cuda() -> Result<Self> {
        // Check if nvidia-smi exists
        let nvidia_smi = which::which("nvidia-smi")
            .map_err(|_| Error::Gpu("nvidia-smi not found".to_string()))?;
        
        // Run nvidia-smi to get GPU info
        let output = Command::new(&nvidia_smi)
            .args(["--query-gpu=name,memory.total,driver_version,compute_cap", "--format=csv,noheader,nounits"])
            .output()
            .map_err(|e| Error::Gpu(format!("Failed to run nvidia-smi: {}", e)))?;
        
        if !output.status.success() {
            return Err(Error::Gpu("nvidia-smi failed".to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split(',').map(|s| s.trim()).collect();
        
        if parts.len() < 3 {
            return Err(Error::Gpu("Failed to parse nvidia-smi output".to_string()));
        }
        
        let device_name = parts[0].to_string();
        let memory: u64 = parts[1].parse().unwrap_or(0) * 1024 * 1024; // MB to bytes
        let driver_version = parts.get(2).map(|s| s.to_string());
        
        let compute_capability = parts.get(3).and_then(|s| {
            let cc: Vec<&str> = s.split('.').collect();
            if cc.len() == 2 {
                Some((cc[0].parse().ok()?, cc[1].parse().ok()?))
            } else {
                None
            }
        });
        
        // Find CUDA toolkit
        let cuda_path = std::env::var("CUDA_HOME")
            .or_else(|_| std::env::var("CUDA_PATH"))
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                // Check common paths
                let paths = [
                    "/usr/local/cuda",
                    "/opt/cuda",
                    "C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v12.0",
                    "C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v11.8",
                ];
                paths.iter()
                    .map(PathBuf::from)
                    .find(|p| p.exists())
            });
        
        Ok(Self {
            backend: GpuBackend::Cuda,
            device_name,
            vendor: GpuVendor::Nvidia,
            memory,
            compute_capability,
            driver_version,
            cuda_path,
            opencl_path: None,
        })
    }
    
    /// Detect Apple Metal GPU.
    #[cfg(target_os = "macos")]
    fn detect_metal() -> Result<Self> {
        // Use system_profiler to get GPU info
        let output = Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
            .map_err(|e| Error::Gpu(format!("Failed to run system_profiler: {}", e)))?;
        
        if !output.status.success() {
            return Err(Error::Gpu("system_profiler failed".to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Simple parsing - look for chipset model
        let device_name = if stdout.contains("Apple M") {
            // Extract Apple Silicon name
            if stdout.contains("M3") {
                "Apple M3 GPU".to_string()
            } else if stdout.contains("M2") {
                "Apple M2 GPU".to_string()
            } else if stdout.contains("M1") {
                "Apple M1 GPU".to_string()
            } else {
                "Apple Silicon GPU".to_string()
            }
        } else {
            "Apple GPU".to_string()
        };
        
        Ok(Self {
            backend: GpuBackend::Metal,
            device_name,
            vendor: GpuVendor::Apple,
            memory: 0, // Would need to parse more carefully
            compute_capability: None,
            driver_version: None,
            cuda_path: None,
            opencl_path: None,
        })
    }
    
    /// Detect OpenCL GPU.
    fn detect_opencl() -> Result<Self> {
        // Check for clinfo
        let clinfo = which::which("clinfo")
            .map_err(|_| Error::Gpu("clinfo not found".to_string()))?;
        
        let output = Command::new(&clinfo)
            .output()
            .map_err(|e| Error::Gpu(format!("Failed to run clinfo: {}", e)))?;
        
        if !output.status.success() {
            return Err(Error::Gpu("clinfo failed".to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse device name
        let device_name = stdout.lines()
            .find(|l| l.contains("Device Name"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_else(|| "OpenCL Device".to_string());
        
        // Determine vendor
        let vendor = if device_name.contains("NVIDIA") || stdout.contains("NVIDIA") {
            GpuVendor::Nvidia
        } else if device_name.contains("AMD") || device_name.contains("Radeon") {
            GpuVendor::Amd
        } else if device_name.contains("Intel") {
            GpuVendor::Intel
        } else {
            GpuVendor::Other
        };
        
        // Parse memory
        let memory = stdout.lines()
            .find(|l| l.contains("Global memory size"))
            .and_then(|l| l.split_once(':'))
            .and_then(|(_, v)| {
                let v = v.trim();
                // Parse size with units
                if v.ends_with("GB") {
                    v.trim_end_matches("GB").trim().parse::<u64>().ok().map(|n| n * 1024 * 1024 * 1024)
                } else if v.ends_with("MB") {
                    v.trim_end_matches("MB").trim().parse::<u64>().ok().map(|n| n * 1024 * 1024)
                } else {
                    v.parse().ok()
                }
            })
            .unwrap_or(0);
        
        Ok(Self {
            backend: GpuBackend::OpenCL,
            device_name,
            vendor,
            memory,
            compute_capability: None,
            driver_version: None,
            cuda_path: None,
            opencl_path: None,
        })
    }
    
    /// Detect Vulkan GPU.
    fn detect_vulkan() -> Result<Self> {
        // Check for vulkaninfo
        let vulkaninfo = which::which("vulkaninfo")
            .map_err(|_| Error::Gpu("vulkaninfo not found".to_string()))?;
        
        let output = Command::new(&vulkaninfo)
            .args(["--summary"])
            .output()
            .map_err(|e| Error::Gpu(format!("Failed to run vulkaninfo: {}", e)))?;
        
        if !output.status.success() {
            return Err(Error::Gpu("vulkaninfo failed".to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse device name
        let device_name = stdout.lines()
            .find(|l| l.contains("deviceName"))
            .and_then(|l| l.split_once('='))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_else(|| "Vulkan Device".to_string());
        
        // Determine vendor
        let vendor = if device_name.contains("NVIDIA") || stdout.contains("NVIDIA") {
            GpuVendor::Nvidia
        } else if device_name.contains("AMD") || device_name.contains("Radeon") {
            GpuVendor::Amd
        } else if device_name.contains("Intel") {
            GpuVendor::Intel
        } else {
            GpuVendor::Other
        };
        
        Ok(Self {
            backend: GpuBackend::Vulkan,
            device_name,
            vendor,
            memory: 0,
            compute_capability: None,
            driver_version: None,
            cuda_path: None,
            opencl_path: None,
        })
    }
    
    /// CPU fallback.
    fn cpu_fallback() -> Self {
        Self {
            backend: GpuBackend::Cpu,
            device_name: "CPU (fallback)".to_string(),
            vendor: GpuVendor::Other,
            memory: 0,
            compute_capability: None,
            driver_version: None,
            cuda_path: None,
            opencl_path: None,
        }
    }
    
    /// Check if GPU is available (not CPU fallback).
    pub fn is_available(&self) -> bool {
        self.backend != GpuBackend::Cpu
    }
    
    /// Get human-readable description.
    pub fn description(&self) -> String {
        let memory_str = if self.memory > 0 {
            format!(" ({} GB)", self.memory / (1024 * 1024 * 1024))
        } else {
            String::new()
        };
        
        format!("{} {} via {}{}",
            self.vendor.as_str(),
            self.device_name,
            self.backend.as_str(),
            memory_str
        )
    }
}

/// Detect all available GPUs.
pub fn detect_all_gpus() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();
    
    // Try CUDA
    if let Ok(gpu) = GpuInfo::detect_cuda() {
        gpus.push(gpu);
    }
    
    // Try Metal (macOS)
    #[cfg(target_os = "macos")]
    if let Ok(gpu) = GpuInfo::detect_metal() {
        gpus.push(gpu);
    }
    
    // Try OpenCL
    if let Ok(gpu) = GpuInfo::detect_opencl() {
        // Avoid duplicates
        if !gpus.iter().any(|g| g.device_name == gpu.device_name) {
            gpus.push(gpu);
        }
    }
    
    gpus
}

/// Print GPU info to console.
pub fn print_gpu_info() {
    println!("GPU Detection Results:");
    println!("{}", "=".repeat(50));
    
    let gpus = detect_all_gpus();
    
    if gpus.is_empty() {
        println!("No GPU detected. CPU will be used for computation.");
    } else {
        for (i, gpu) in gpus.iter().enumerate() {
            println!("GPU {}: {}", i, gpu.description());
            
            if let Some((major, minor)) = gpu.compute_capability {
                println!("  Compute Capability: {}.{}", major, minor);
            }
            
            if let Some(ref driver) = gpu.driver_version {
                println!("  Driver Version: {}", driver);
            }
            
            if let Some(ref path) = gpu.cuda_path {
                println!("  CUDA Path: {}", path.display());
            }
        }
    }
    
    println!("{}", "=".repeat(50));
}

