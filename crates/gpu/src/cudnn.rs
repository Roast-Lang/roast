//! cuDNN FFI bindings and deep learning operations.
//!
//! This module provides GPU-accelerated deep learning primitives
//! using NVIDIA's cuDNN library.

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use libloading::{Library, Symbol};
use crate::error::{GpuError, GpuResult};
use crate::tensor::{Tensor, Shape, DType};

// =============================================================================
// cuDNN Types
// =============================================================================

/// cuDNN handle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnHandle {
    _opaque: *mut c_void,
}

impl CudnnHandle {
    pub fn null() -> Self {
        Self { _opaque: ptr::null_mut() }
    }
}

/// cuDNN tensor descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnTensorDescriptor {
    _opaque: *mut c_void,
}

impl CudnnTensorDescriptor {
    pub fn null() -> Self {
        Self { _opaque: ptr::null_mut() }
    }
}

/// cuDNN filter descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnFilterDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN convolution descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnConvolutionDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN pooling descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnPoolingDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN activation descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnActivationDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN dropout descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnDropoutDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN RNN descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnRnnDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN attention descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CudnnAttnDescriptor {
    _opaque: *mut c_void,
}

/// cuDNN status codes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnStatus {
    Success = 0,
    NotInitialized = 1,
    AllocFailed = 2,
    BadParam = 3,
    InternalError = 4,
    InvalidValue = 5,
    ArchMismatch = 6,
    MappingError = 7,
    ExecutionFailed = 8,
    NotSupported = 9,
    LicenseError = 10,
    RuntimePrerequisiteMissing = 11,
    RuntimeInProgress = 12,
    RuntimeFpOverflow = 13,
}

impl CudnnStatus {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::NotInitialized,
            2 => Self::AllocFailed,
            3 => Self::BadParam,
            4 => Self::InternalError,
            5 => Self::InvalidValue,
            6 => Self::ArchMismatch,
            7 => Self::MappingError,
            8 => Self::ExecutionFailed,
            9 => Self::NotSupported,
            10 => Self::LicenseError,
            _ => Self::InternalError,
        }
    }
    
    pub fn is_success(&self) -> bool {
        *self == Self::Success
    }
    
    pub fn to_error(&self) -> GpuError {
        GpuError::Cuda(format!("cuDNN error: {:?}", self))
    }
}

/// cuDNN data type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnDataType {
    Float = 0,
    Double = 1,
    Half = 2,
    Int8 = 3,
    Int32 = 4,
    Int8x4 = 5,
    Uint8 = 6,
    Uint8x4 = 7,
    Int8x32 = 8,
    BFloat16 = 9,
    Int64 = 10,
    Boolean = 11,
}

impl From<DType> for CudnnDataType {
    fn from(dtype: DType) -> Self {
        match dtype {
            DType::Float32 => Self::Float,
            DType::Float64 => Self::Double,
            DType::Float16 => Self::Half,
            DType::BFloat16 => Self::BFloat16,
            DType::Int8 => Self::Int8,
            DType::Int32 => Self::Int32,
            DType::Int64 => Self::Int64,
            DType::UInt8 => Self::Uint8,
            DType::Bool => Self::Boolean,
            _ => Self::Float,
        }
    }
}

/// cuDNN tensor format.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnTensorFormat {
    NCHW = 0,
    NHWC = 1,
    NCHWVectC = 2,
}

/// cuDNN activation mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnActivationMode {
    Sigmoid = 0,
    Relu = 1,
    Tanh = 2,
    ClippedRelu = 3,
    Elu = 4,
    Identity = 5,
    Swish = 6,
}

/// cuDNN pooling mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnPoolingMode {
    Max = 0,
    AverageCountIncludePadding = 1,
    AverageCountExcludePadding = 2,
    MaxDeterministic = 3,
}

/// cuDNN softmax algorithm.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnSoftmaxAlgorithm {
    Fast = 0,
    Accurate = 1,
    Log = 2,
}

/// cuDNN softmax mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnSoftmaxMode {
    Instance = 0,
    Channel = 1,
}

/// cuDNN convolution mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnConvolutionMode {
    Convolution = 0,
    CrossCorrelation = 1,
}

/// cuDNN batch norm mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnBatchNormMode {
    PerActivation = 0,
    Spatial = 1,
    SpatialPersistent = 2,
}

/// cuDNN RNN mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudnnRnnMode {
    RnnRelu = 0,
    RnnTanh = 1,
    Lstm = 2,
    Gru = 3,
}

// =============================================================================
// cuDNN Function Types
// =============================================================================

type CudnnCreate = unsafe extern "C" fn(*mut CudnnHandle) -> i32;
type CudnnDestroy = unsafe extern "C" fn(CudnnHandle) -> i32;
type CudnnSetStream = unsafe extern "C" fn(CudnnHandle, usize) -> i32;

type CudnnCreateTensorDescriptor = unsafe extern "C" fn(*mut CudnnTensorDescriptor) -> i32;
type CudnnDestroyTensorDescriptor = unsafe extern "C" fn(CudnnTensorDescriptor) -> i32;
type CudnnSetTensor4dDescriptor = unsafe extern "C" fn(
    CudnnTensorDescriptor, i32, i32, i32, i32, i32, i32,
) -> i32;

type CudnnCreateActivationDescriptor = unsafe extern "C" fn(*mut CudnnActivationDescriptor) -> i32;
type CudnnDestroyActivationDescriptor = unsafe extern "C" fn(CudnnActivationDescriptor) -> i32;
type CudnnSetActivationDescriptor = unsafe extern "C" fn(
    CudnnActivationDescriptor, i32, i32, f64,
) -> i32;

type CudnnActivationForward = unsafe extern "C" fn(
    CudnnHandle,
    CudnnActivationDescriptor,
    *const f32, CudnnTensorDescriptor, *const c_void,
    *const f32, CudnnTensorDescriptor, *mut c_void,
) -> i32;

type CudnnActivationBackward = unsafe extern "C" fn(
    CudnnHandle,
    CudnnActivationDescriptor,
    *const f32, CudnnTensorDescriptor, *const c_void,
    CudnnTensorDescriptor, *const c_void,
    CudnnTensorDescriptor, *const c_void,
    *const f32, CudnnTensorDescriptor, *mut c_void,
) -> i32;

type CudnnSoftmaxForward = unsafe extern "C" fn(
    CudnnHandle, i32, i32,
    *const f32, CudnnTensorDescriptor, *const c_void,
    *const f32, CudnnTensorDescriptor, *mut c_void,
) -> i32;

type CudnnSoftmaxBackward = unsafe extern "C" fn(
    CudnnHandle, i32, i32,
    *const f32, CudnnTensorDescriptor, *const c_void,
    CudnnTensorDescriptor, *const c_void,
    *const f32, CudnnTensorDescriptor, *mut c_void,
) -> i32;

type CudnnCreatePoolingDescriptor = unsafe extern "C" fn(*mut CudnnPoolingDescriptor) -> i32;
type CudnnDestroyPoolingDescriptor = unsafe extern "C" fn(CudnnPoolingDescriptor) -> i32;
type CudnnSetPooling2dDescriptor = unsafe extern "C" fn(
    CudnnPoolingDescriptor, i32, i32, i32, i32, i32, i32, i32, i32,
) -> i32;

type CudnnPoolingForward = unsafe extern "C" fn(
    CudnnHandle, CudnnPoolingDescriptor,
    *const f32, CudnnTensorDescriptor, *const c_void,
    *const f32, CudnnTensorDescriptor, *mut c_void,
) -> i32;

type CudnnBatchNormForward = unsafe extern "C" fn(
    CudnnHandle, i32,
    *const f32, *const f32,
    CudnnTensorDescriptor, *const c_void,
    CudnnTensorDescriptor, *mut c_void,
    CudnnTensorDescriptor,
    *const c_void, *const c_void,
    f64,
    *mut c_void, *mut c_void,
    f64,
    *mut c_void, *mut c_void,
) -> i32;

type CudnnCreateDropoutDescriptor = unsafe extern "C" fn(*mut CudnnDropoutDescriptor) -> i32;
type CudnnDestroyDropoutDescriptor = unsafe extern "C" fn(CudnnDropoutDescriptor) -> i32;

// =============================================================================
// cuDNN Library
// =============================================================================

/// cuDNN library handle.
pub struct CudnnLib {
    _lib: Library,
    create: Symbol<'static, CudnnCreate>,
    destroy: Symbol<'static, CudnnDestroy>,
    set_stream: Symbol<'static, CudnnSetStream>,
    
    create_tensor_desc: Symbol<'static, CudnnCreateTensorDescriptor>,
    destroy_tensor_desc: Symbol<'static, CudnnDestroyTensorDescriptor>,
    set_tensor_4d_desc: Symbol<'static, CudnnSetTensor4dDescriptor>,
    
    create_activation_desc: Symbol<'static, CudnnCreateActivationDescriptor>,
    destroy_activation_desc: Symbol<'static, CudnnDestroyActivationDescriptor>,
    set_activation_desc: Symbol<'static, CudnnSetActivationDescriptor>,
    activation_forward: Symbol<'static, CudnnActivationForward>,
    activation_backward: Symbol<'static, CudnnActivationBackward>,
    
    softmax_forward: Symbol<'static, CudnnSoftmaxForward>,
    softmax_backward: Symbol<'static, CudnnSoftmaxBackward>,
    
    create_pooling_desc: Symbol<'static, CudnnCreatePoolingDescriptor>,
    destroy_pooling_desc: Symbol<'static, CudnnDestroyPoolingDescriptor>,
    set_pooling_2d_desc: Symbol<'static, CudnnSetPooling2dDescriptor>,
    pooling_forward: Symbol<'static, CudnnPoolingForward>,
}

impl CudnnLib {
    /// Load the cuDNN library.
    pub fn load() -> GpuResult<Self> {
        let lib_names = if cfg!(windows) {
            vec!["cudnn64_9.dll", "cudnn64_8.dll", "cudnn.dll"]
        } else if cfg!(target_os = "macos") {
            vec!["libcudnn.dylib"]
        } else {
            vec!["libcudnn.so.9", "libcudnn.so.8", "libcudnn.so"]
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
            "cuDNN library not found: {:?}",
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
            
            load_symbol!(create, CudnnCreate, b"cudnnCreate\0");
            load_symbol!(destroy, CudnnDestroy, b"cudnnDestroy\0");
            load_symbol!(set_stream, CudnnSetStream, b"cudnnSetStream\0");
            
            load_symbol!(create_tensor_desc, CudnnCreateTensorDescriptor, b"cudnnCreateTensorDescriptor\0");
            load_symbol!(destroy_tensor_desc, CudnnDestroyTensorDescriptor, b"cudnnDestroyTensorDescriptor\0");
            load_symbol!(set_tensor_4d_desc, CudnnSetTensor4dDescriptor, b"cudnnSetTensor4dDescriptor\0");
            
            load_symbol!(create_activation_desc, CudnnCreateActivationDescriptor, b"cudnnCreateActivationDescriptor\0");
            load_symbol!(destroy_activation_desc, CudnnDestroyActivationDescriptor, b"cudnnDestroyActivationDescriptor\0");
            load_symbol!(set_activation_desc, CudnnSetActivationDescriptor, b"cudnnSetActivationDescriptor\0");
            load_symbol!(activation_forward, CudnnActivationForward, b"cudnnActivationForward\0");
            load_symbol!(activation_backward, CudnnActivationBackward, b"cudnnActivationBackward\0");
            
            load_symbol!(softmax_forward, CudnnSoftmaxForward, b"cudnnSoftmaxForward\0");
            load_symbol!(softmax_backward, CudnnSoftmaxBackward, b"cudnnSoftmaxBackward\0");
            
            load_symbol!(create_pooling_desc, CudnnCreatePoolingDescriptor, b"cudnnCreatePoolingDescriptor\0");
            load_symbol!(destroy_pooling_desc, CudnnDestroyPoolingDescriptor, b"cudnnDestroyPoolingDescriptor\0");
            load_symbol!(set_pooling_2d_desc, CudnnSetPooling2dDescriptor, b"cudnnSetPooling2dDescriptor\0");
            load_symbol!(pooling_forward, CudnnPoolingForward, b"cudnnPoolingForward\0");
            
            let lib_ref = &*(lib as *const Library);
            
            Ok(Self {
                _lib: ptr::read(lib_ref),
                create, destroy, set_stream,
                create_tensor_desc, destroy_tensor_desc, set_tensor_4d_desc,
                create_activation_desc, destroy_activation_desc, set_activation_desc,
                activation_forward, activation_backward,
                softmax_forward, softmax_backward,
                create_pooling_desc, destroy_pooling_desc, set_pooling_2d_desc,
                pooling_forward,
            })
        }
    }
}

// =============================================================================
// cuDNN Context
// =============================================================================

/// cuDNN context for deep learning operations.
pub struct Cudnn {
    lib: Arc<CudnnLib>,
    handle: CudnnHandle,
}

impl Cudnn {
    /// Create a new cuDNN context.
    pub fn new() -> GpuResult<Self> {
        let lib = Arc::new(CudnnLib::load()?);
        
        let mut handle = CudnnHandle::null();
        let status = unsafe { (lib.create)(&mut handle) };
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(Self { lib, handle })
    }
    
    /// Set the CUDA stream.
    pub fn set_stream(&self, stream: usize) -> GpuResult<()> {
        let status = unsafe { (self.lib.set_stream)(self.handle, stream) };
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// Create a tensor descriptor for 4D tensor (NCHW).
    pub fn create_tensor_desc_4d(
        &self,
        n: i32, c: i32, h: i32, w: i32,
        dtype: DType,
    ) -> GpuResult<CudnnTensorDescriptor> {
        let mut desc = CudnnTensorDescriptor::null();
        
        let status = unsafe { (self.lib.create_tensor_desc)(&mut desc) };
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        let status = unsafe {
            (self.lib.set_tensor_4d_desc)(
                desc,
                CudnnTensorFormat::NCHW as i32,
                CudnnDataType::from(dtype) as i32,
                n, c, h, w,
            )
        };
        
        if !CudnnStatus::from_code(status).is_success() {
            unsafe { (self.lib.destroy_tensor_desc)(desc) };
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(desc)
    }
    
    /// Destroy a tensor descriptor.
    pub fn destroy_tensor_desc(&self, desc: CudnnTensorDescriptor) -> GpuResult<()> {
        let status = unsafe { (self.lib.destroy_tensor_desc)(desc) };
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
}

impl Drop for Cudnn {
    fn drop(&mut self) {
        unsafe { (self.lib.destroy)(self.handle) };
    }
}

// =============================================================================
// High-Level Deep Learning Operations
// =============================================================================

/// Deep learning operations using cuDNN.
pub struct DnnOps {
    cudnn: Cudnn,
}

impl DnnOps {
    /// Create a new DNN operations context.
    pub fn new() -> GpuResult<Self> {
        Ok(Self {
            cudnn: Cudnn::new()?,
        })
    }
    
    /// ReLU activation forward.
    pub fn relu(&self, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        self.activation(x, y, CudnnActivationMode::Relu, 0.0)
    }
    
    /// Sigmoid activation forward.
    pub fn sigmoid(&self, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        self.activation(x, y, CudnnActivationMode::Sigmoid, 0.0)
    }
    
    /// Tanh activation forward.
    pub fn tanh(&self, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        self.activation(x, y, CudnnActivationMode::Tanh, 0.0)
    }
    
    /// ELU activation forward.
    pub fn elu(&self, x: &Tensor, y: &mut Tensor, alpha: f64) -> GpuResult<()> {
        self.activation(x, y, CudnnActivationMode::Elu, alpha)
    }
    
    /// Swish activation forward.
    pub fn swish(&self, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        self.activation(x, y, CudnnActivationMode::Swish, 1.0)
    }
    
    /// Generic activation forward.
    fn activation(
        &self,
        x: &Tensor,
        y: &mut Tensor,
        mode: CudnnActivationMode,
        coef: f64,
    ) -> GpuResult<()> {
        // Create activation descriptor
        let mut act_desc = CudnnActivationDescriptor { _opaque: ptr::null_mut() };
        let status = unsafe { (self.cudnn.lib.create_activation_desc)(&mut act_desc) };
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        // Set activation descriptor
        let status = unsafe {
            (self.cudnn.lib.set_activation_desc)(
                act_desc,
                mode as i32,
                0, // NaN propagation
                coef,
            )
        };
        if !CudnnStatus::from_code(status).is_success() {
            unsafe { (self.cudnn.lib.destroy_activation_desc)(act_desc) };
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        // Create tensor descriptors
        let (n, c, h, w) = get_nchw(x)?;
        let x_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, x.dtype())?;
        let y_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, y.dtype())?;
        
        // Forward
        let alpha: f32 = 1.0;
        let beta: f32 = 0.0;
        
        let status = unsafe {
            (self.cudnn.lib.activation_forward)(
                self.cudnn.handle,
                act_desc,
                &alpha, x_desc, x.data_ptr().ptr as *const c_void,
                &beta, y_desc, y.data_ptr().ptr as *mut c_void,
            )
        };
        
        // Cleanup
        self.cudnn.destroy_tensor_desc(x_desc)?;
        self.cudnn.destroy_tensor_desc(y_desc)?;
        unsafe { (self.cudnn.lib.destroy_activation_desc)(act_desc) };
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// Softmax forward.
    pub fn softmax(&self, x: &Tensor, y: &mut Tensor, dim: i32) -> GpuResult<()> {
        let (n, c, h, w) = get_nchw(x)?;
        let x_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, x.dtype())?;
        let y_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, y.dtype())?;
        
        let alpha: f32 = 1.0;
        let beta: f32 = 0.0;
        
        let mode = if dim == 1 { CudnnSoftmaxMode::Channel } else { CudnnSoftmaxMode::Instance };
        
        let status = unsafe {
            (self.cudnn.lib.softmax_forward)(
                self.cudnn.handle,
                CudnnSoftmaxAlgorithm::Accurate as i32,
                mode as i32,
                &alpha, x_desc, x.data_ptr().ptr as *const c_void,
                &beta, y_desc, y.data_ptr().ptr as *mut c_void,
            )
        };
        
        self.cudnn.destroy_tensor_desc(x_desc)?;
        self.cudnn.destroy_tensor_desc(y_desc)?;
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// Log softmax forward.
    pub fn log_softmax(&self, x: &Tensor, y: &mut Tensor, dim: i32) -> GpuResult<()> {
        let (n, c, h, w) = get_nchw(x)?;
        let x_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, x.dtype())?;
        let y_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, y.dtype())?;
        
        let alpha: f32 = 1.0;
        let beta: f32 = 0.0;
        
        let mode = if dim == 1 { CudnnSoftmaxMode::Channel } else { CudnnSoftmaxMode::Instance };
        
        let status = unsafe {
            (self.cudnn.lib.softmax_forward)(
                self.cudnn.handle,
                CudnnSoftmaxAlgorithm::Log as i32,
                mode as i32,
                &alpha, x_desc, x.data_ptr().ptr as *const c_void,
                &beta, y_desc, y.data_ptr().ptr as *mut c_void,
            )
        };
        
        self.cudnn.destroy_tensor_desc(x_desc)?;
        self.cudnn.destroy_tensor_desc(y_desc)?;
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// Max pooling 2D forward.
    pub fn max_pool2d(
        &self,
        x: &Tensor,
        y: &mut Tensor,
        kernel_size: (i32, i32),
        stride: (i32, i32),
        padding: (i32, i32),
    ) -> GpuResult<()> {
        self.pool2d(x, y, kernel_size, stride, padding, CudnnPoolingMode::Max)
    }
    
    /// Average pooling 2D forward.
    pub fn avg_pool2d(
        &self,
        x: &Tensor,
        y: &mut Tensor,
        kernel_size: (i32, i32),
        stride: (i32, i32),
        padding: (i32, i32),
    ) -> GpuResult<()> {
        self.pool2d(x, y, kernel_size, stride, padding, CudnnPoolingMode::AverageCountExcludePadding)
    }
    
    /// Generic pooling 2D forward.
    fn pool2d(
        &self,
        x: &Tensor,
        y: &mut Tensor,
        kernel_size: (i32, i32),
        stride: (i32, i32),
        padding: (i32, i32),
        mode: CudnnPoolingMode,
    ) -> GpuResult<()> {
        // Create pooling descriptor
        let mut pool_desc = CudnnPoolingDescriptor { _opaque: ptr::null_mut() };
        let status = unsafe { (self.cudnn.lib.create_pooling_desc)(&mut pool_desc) };
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        let status = unsafe {
            (self.cudnn.lib.set_pooling_2d_desc)(
                pool_desc,
                mode as i32,
                0, // NaN propagation
                kernel_size.0, kernel_size.1,
                padding.0, padding.1,
                stride.0, stride.1,
            )
        };
        if !CudnnStatus::from_code(status).is_success() {
            unsafe { (self.cudnn.lib.destroy_pooling_desc)(pool_desc) };
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        // Create tensor descriptors
        let (n, c, h, w) = get_nchw(x)?;
        let x_desc = self.cudnn.create_tensor_desc_4d(n, c, h, w, x.dtype())?;
        
        let (n_out, c_out, h_out, w_out) = get_nchw(y)?;
        let y_desc = self.cudnn.create_tensor_desc_4d(n_out, c_out, h_out, w_out, y.dtype())?;
        
        let alpha: f32 = 1.0;
        let beta: f32 = 0.0;
        
        let status = unsafe {
            (self.cudnn.lib.pooling_forward)(
                self.cudnn.handle,
                pool_desc,
                &alpha, x_desc, x.data_ptr().ptr as *const c_void,
                &beta, y_desc, y.data_ptr().ptr as *mut c_void,
            )
        };
        
        // Cleanup
        self.cudnn.destroy_tensor_desc(x_desc)?;
        self.cudnn.destroy_tensor_desc(y_desc)?;
        unsafe { (self.cudnn.lib.destroy_pooling_desc)(pool_desc) };
        
        if !CudnnStatus::from_code(status).is_success() {
            return Err(CudnnStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
}

/// Get NCHW dimensions from tensor.
fn get_nchw(tensor: &Tensor) -> GpuResult<(i32, i32, i32, i32)> {
    match tensor.ndim() {
        1 => Ok((1, tensor.shape().dim(0) as i32, 1, 1)),
        2 => Ok((tensor.shape().dim(0) as i32, tensor.shape().dim(1) as i32, 1, 1)),
        3 => Ok((
            tensor.shape().dim(0) as i32,
            tensor.shape().dim(1) as i32,
            tensor.shape().dim(2) as i32,
            1,
        )),
        4 => Ok((
            tensor.shape().dim(0) as i32,
            tensor.shape().dim(1) as i32,
            tensor.shape().dim(2) as i32,
            tensor.shape().dim(3) as i32,
        )),
        _ => Err(GpuError::InvalidShape(
            "cuDNN requires 1-4D tensors".to_string()
        )),
    }
}

