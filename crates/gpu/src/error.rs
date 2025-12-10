//! GPU error types.

use std::fmt;
use thiserror::Error;

/// GPU error type.
#[derive(Error, Debug)]
pub enum GpuError {
    #[error("No GPU device available")]
    NoDevice,
    
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    
    #[error("Backend not available: {0}")]
    BackendNotAvailable(String),
    
    #[error("CUDA error: {0}")]
    Cuda(String),
    
    #[error("OpenCL error: {0}")]
    OpenCL(String),
    
    #[error("Metal error: {0}")]
    Metal(String),
    
    #[error("Vulkan error: {0}")]
    Vulkan(String),
    
    #[error("Kernel compilation error: {0}")]
    Compilation(String),
    
    #[error("Kernel launch error: {0}")]
    Launch(String),
    
    #[error("Memory allocation error: {0}")]
    Allocation(String),
    
    #[error("Memory transfer error: {0}")]
    Transfer(String),
    
    #[error("Out of memory: requested {requested} bytes, available {available} bytes")]
    OutOfMemory { requested: usize, available: usize },
    
    #[error("Invalid tensor shape: {0}")]
    InvalidShape(String),
    
    #[error("Type mismatch: expected {expected}, got {got}")]
    TypeMismatch { expected: String, got: String },
    
    #[error("Dimension mismatch: {0}")]
    DimensionMismatch(String),
    
    #[error("Invalid kernel configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Runtime error: {0}")]
    Runtime(String),
    
    #[error("Driver not found: {0}")]
    DriverNotFound(String),
    
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// GPU result type.
pub type GpuResult<T> = Result<T, GpuError>;

/// CUDA error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CudaError {
    Success = 0,
    InvalidValue = 1,
    OutOfMemory = 2,
    NotInitialized = 3,
    Deinitialized = 4,
    NoDevice = 100,
    InvalidDevice = 101,
    InvalidImage = 200,
    InvalidContext = 201,
    MapFailed = 205,
    UnmapFailed = 206,
    LaunchFailed = 700,
    LaunchTimeout = 702,
    LaunchOutOfResources = 701,
    Unknown = -1,
}

impl CudaError {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::InvalidValue,
            2 => Self::OutOfMemory,
            3 => Self::NotInitialized,
            4 => Self::Deinitialized,
            100 => Self::NoDevice,
            101 => Self::InvalidDevice,
            200 => Self::InvalidImage,
            201 => Self::InvalidContext,
            205 => Self::MapFailed,
            206 => Self::UnmapFailed,
            700 => Self::LaunchFailed,
            701 => Self::LaunchOutOfResources,
            702 => Self::LaunchTimeout,
            _ => Self::Unknown,
        }
    }
    
    pub fn to_gpu_error(self) -> GpuError {
        GpuError::Cuda(format!("{:?}", self))
    }
}

/// OpenCL error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OpenCLError {
    Success = 0,
    DeviceNotFound = -1,
    DeviceNotAvailable = -2,
    CompilerNotAvailable = -3,
    MemObjectAllocationFailure = -4,
    OutOfResources = -5,
    OutOfHostMemory = -6,
    ProfilingInfoNotAvailable = -7,
    MemCopyOverlap = -8,
    ImageFormatMismatch = -9,
    ImageFormatNotSupported = -10,
    BuildProgramFailure = -11,
    MapFailure = -12,
    InvalidValue = -30,
    InvalidDeviceType = -31,
    InvalidPlatform = -32,
    InvalidDevice = -33,
    InvalidContext = -34,
    InvalidQueueProperties = -35,
    InvalidCommandQueue = -36,
    InvalidHostPtr = -37,
    InvalidMemObject = -38,
    InvalidImageFormatDescriptor = -39,
    InvalidImageSize = -40,
    InvalidSampler = -41,
    InvalidBinary = -42,
    InvalidBuildOptions = -43,
    InvalidProgram = -44,
    InvalidProgramExecutable = -45,
    InvalidKernelName = -46,
    InvalidKernelDefinition = -47,
    InvalidKernel = -48,
    InvalidArgIndex = -49,
    InvalidArgValue = -50,
    InvalidArgSize = -51,
    InvalidKernelArgs = -52,
    InvalidWorkDimension = -53,
    InvalidWorkGroupSize = -54,
    InvalidWorkItemSize = -55,
    InvalidGlobalOffset = -56,
    InvalidEventWaitList = -57,
    InvalidEvent = -58,
    InvalidOperation = -59,
    InvalidGlObject = -60,
    InvalidBufferSize = -61,
    InvalidMipLevel = -62,
    InvalidGlobalWorkSize = -63,
    Unknown = -9999,
}

impl OpenCLError {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Success,
            -1 => Self::DeviceNotFound,
            -2 => Self::DeviceNotAvailable,
            -3 => Self::CompilerNotAvailable,
            -4 => Self::MemObjectAllocationFailure,
            -5 => Self::OutOfResources,
            -6 => Self::OutOfHostMemory,
            -11 => Self::BuildProgramFailure,
            -30 => Self::InvalidValue,
            -33 => Self::InvalidDevice,
            -34 => Self::InvalidContext,
            -48 => Self::InvalidKernel,
            -52 => Self::InvalidKernelArgs,
            _ => Self::Unknown,
        }
    }
    
    pub fn to_gpu_error(self) -> GpuError {
        GpuError::OpenCL(format!("{:?}", self))
    }
}

