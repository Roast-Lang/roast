//! cuBLAS FFI bindings and BLAS operations.
//!
//! This module provides GPU-accelerated BLAS (Basic Linear Algebra Subprograms)
//! operations using NVIDIA's cuBLAS library.

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use libloading::{Library, Symbol};
use crate::error::{GpuError, GpuResult};
use crate::tensor::{Tensor, DType};

// =============================================================================
// cuBLAS Types
// =============================================================================

/// cuBLAS handle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CublasHandle {
    _opaque: *mut c_void,
}

impl CublasHandle {
    pub fn null() -> Self {
        Self { _opaque: ptr::null_mut() }
    }
    
    pub fn is_null(&self) -> bool {
        self._opaque.is_null()
    }
}

/// cuBLAS status codes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasStatus {
    Success = 0,
    NotInitialized = 1,
    AllocFailed = 3,
    InvalidValue = 7,
    ArchMismatch = 8,
    MappingError = 11,
    ExecutionFailed = 13,
    InternalError = 14,
    NotSupported = 15,
    LicenseError = 16,
}

impl CublasStatus {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::NotInitialized,
            3 => Self::AllocFailed,
            7 => Self::InvalidValue,
            8 => Self::ArchMismatch,
            11 => Self::MappingError,
            13 => Self::ExecutionFailed,
            14 => Self::InternalError,
            15 => Self::NotSupported,
            16 => Self::LicenseError,
            _ => Self::InternalError,
        }
    }
    
    pub fn is_success(&self) -> bool {
        *self == Self::Success
    }
    
    pub fn to_error(&self) -> GpuError {
        GpuError::Cuda(format!("cuBLAS error: {:?}", self))
    }
}

/// cuBLAS operation type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasOperation {
    None = 0,      // CUBLAS_OP_N
    Transpose = 1, // CUBLAS_OP_T
    ConjTrans = 2, // CUBLAS_OP_C
}

/// cuBLAS fill mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasFillMode {
    Lower = 0,
    Upper = 1,
}

/// cuBLAS diagonal type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasDiagType {
    NonUnit = 0,
    Unit = 1,
}

/// cuBLAS side mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasSideMode {
    Left = 0,
    Right = 1,
}

/// cuBLAS pointer mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasPointerMode {
    Host = 0,
    Device = 1,
}

/// cuBLAS atomics mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasAtomicsMode {
    NotAllowed = 0,
    Allowed = 1,
}

/// cuBLAS math mode.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CublasMathMode {
    Default = 0,
    TensorOp = 1,
    PedanticMath = 2,
    TensorOpMath = 3,
    DisallowReducedPrecision = 16,
}

// =============================================================================
// cuBLAS Function Types
// =============================================================================

type CublasCreate = unsafe extern "C" fn(*mut CublasHandle) -> i32;
type CublasDestroy = unsafe extern "C" fn(CublasHandle) -> i32;
type CublasSetStream = unsafe extern "C" fn(CublasHandle, usize) -> i32;
type CublasGetStream = unsafe extern "C" fn(CublasHandle, *mut usize) -> i32;
type CublasSetPointerMode = unsafe extern "C" fn(CublasHandle, i32) -> i32;
type CublasSetMathMode = unsafe extern "C" fn(CublasHandle, i32) -> i32;

// Level 1 BLAS
type CublasSaxpy = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, *const f32, i32, *mut f32, i32,
) -> i32;
type CublasDaxpy = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, *const f64, i32, *mut f64, i32,
) -> i32;
type CublasSdot = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, i32, *const f32, i32, *mut f32,
) -> i32;
type CublasDdot = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, i32, *const f64, i32, *mut f64,
) -> i32;
type CublasSnrm2 = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, i32, *mut f32,
) -> i32;
type CublasDnrm2 = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, i32, *mut f64,
) -> i32;
type CublasSscal = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, *mut f32, i32,
) -> i32;
type CublasDscal = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, *mut f64, i32,
) -> i32;
type CublasScopy = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, i32, *mut f32, i32,
) -> i32;
type CublasDcopy = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, i32, *mut f64, i32,
) -> i32;
type CublasIsamax = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, i32, *mut i32,
) -> i32;
type CublasIdamax = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, i32, *mut i32,
) -> i32;
type CublasSasum = unsafe extern "C" fn(
    CublasHandle, i32, *const f32, i32, *mut f32,
) -> i32;
type CublasDasum = unsafe extern "C" fn(
    CublasHandle, i32, *const f64, i32, *mut f64,
) -> i32;

// Level 2 BLAS
type CublasSgemv = unsafe extern "C" fn(
    CublasHandle, i32, i32, i32, *const f32,
    *const f32, i32, *const f32, i32, *const f32, *mut f32, i32,
) -> i32;
type CublasDgemv = unsafe extern "C" fn(
    CublasHandle, i32, i32, i32, *const f64,
    *const f64, i32, *const f64, i32, *const f64, *mut f64, i32,
) -> i32;

// Level 3 BLAS
type CublasSgemm = unsafe extern "C" fn(
    CublasHandle, i32, i32, i32, i32, i32, *const f32,
    *const f32, i32, *const f32, i32, *const f32, *mut f32, i32,
) -> i32;
type CublasDgemm = unsafe extern "C" fn(
    CublasHandle, i32, i32, i32, i32, i32, *const f64,
    *const f64, i32, *const f64, i32, *const f64, *mut f64, i32,
) -> i32;
type CublasSgemmBatched = unsafe extern "C" fn(
    CublasHandle, i32, i32, i32, i32, i32, *const f32,
    *const *const f32, i32, *const *const f32, i32, *const f32,
    *const *mut f32, i32, i32,
) -> i32;
type CublasSgemmStridedBatched = unsafe extern "C" fn(
    CublasHandle, i32, i32, i32, i32, i32, *const f32,
    *const f32, i32, i64, *const f32, i32, i64, *const f32,
    *mut f32, i32, i64, i32,
) -> i32;

// =============================================================================
// cuBLAS Library
// =============================================================================

/// cuBLAS library handle.
pub struct CublasLib {
    _lib: Library,
    create: Symbol<'static, CublasCreate>,
    destroy: Symbol<'static, CublasDestroy>,
    set_stream: Symbol<'static, CublasSetStream>,
    set_pointer_mode: Symbol<'static, CublasSetPointerMode>,
    set_math_mode: Symbol<'static, CublasSetMathMode>,
    
    // Level 1
    saxpy: Symbol<'static, CublasSaxpy>,
    daxpy: Symbol<'static, CublasDaxpy>,
    sdot: Symbol<'static, CublasSdot>,
    ddot: Symbol<'static, CublasDdot>,
    snrm2: Symbol<'static, CublasSnrm2>,
    dnrm2: Symbol<'static, CublasDnrm2>,
    sscal: Symbol<'static, CublasSscal>,
    dscal: Symbol<'static, CublasDscal>,
    scopy: Symbol<'static, CublasScopy>,
    dcopy: Symbol<'static, CublasDcopy>,
    isamax: Symbol<'static, CublasIsamax>,
    idamax: Symbol<'static, CublasIdamax>,
    sasum: Symbol<'static, CublasSasum>,
    dasum: Symbol<'static, CublasDasum>,
    
    // Level 2
    sgemv: Symbol<'static, CublasSgemv>,
    dgemv: Symbol<'static, CublasDgemv>,
    
    // Level 3
    sgemm: Symbol<'static, CublasSgemm>,
    dgemm: Symbol<'static, CublasDgemm>,
    sgemm_batched: Symbol<'static, CublasSgemmBatched>,
    sgemm_strided_batched: Symbol<'static, CublasSgemmStridedBatched>,
}

impl CublasLib {
    /// Load the cuBLAS library.
    pub fn load() -> GpuResult<Self> {
        let lib_names = if cfg!(windows) {
            vec!["cublas64_12.dll", "cublas64_11.dll", "cublas.dll"]
        } else if cfg!(target_os = "macos") {
            vec!["libcublas.dylib"]
        } else {
            vec!["libcublas.so.12", "libcublas.so.11", "libcublas.so"]
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
            "cuBLAS library not found: {:?}",
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
            
            load_symbol!(create, CublasCreate, b"cublasCreate_v2\0");
            load_symbol!(destroy, CublasDestroy, b"cublasDestroy_v2\0");
            load_symbol!(set_stream, CublasSetStream, b"cublasSetStream_v2\0");
            load_symbol!(set_pointer_mode, CublasSetPointerMode, b"cublasSetPointerMode_v2\0");
            load_symbol!(set_math_mode, CublasSetMathMode, b"cublasSetMathMode\0");
            
            // Level 1
            load_symbol!(saxpy, CublasSaxpy, b"cublasSaxpy_v2\0");
            load_symbol!(daxpy, CublasDaxpy, b"cublasDaxpy_v2\0");
            load_symbol!(sdot, CublasSdot, b"cublasSdot_v2\0");
            load_symbol!(ddot, CublasDdot, b"cublasDdot_v2\0");
            load_symbol!(snrm2, CublasSnrm2, b"cublasSnrm2_v2\0");
            load_symbol!(dnrm2, CublasDnrm2, b"cublasDnrm2_v2\0");
            load_symbol!(sscal, CublasSscal, b"cublasSscal_v2\0");
            load_symbol!(dscal, CublasDscal, b"cublasDscal_v2\0");
            load_symbol!(scopy, CublasScopy, b"cublasScopy_v2\0");
            load_symbol!(dcopy, CublasDcopy, b"cublasDcopy_v2\0");
            load_symbol!(isamax, CublasIsamax, b"cublasIsamax_v2\0");
            load_symbol!(idamax, CublasIdamax, b"cublasIdamax_v2\0");
            load_symbol!(sasum, CublasSasum, b"cublasSasum_v2\0");
            load_symbol!(dasum, CublasDasum, b"cublasDasum_v2\0");
            
            // Level 2
            load_symbol!(sgemv, CublasSgemv, b"cublasSgemv_v2\0");
            load_symbol!(dgemv, CublasDgemv, b"cublasDgemv_v2\0");
            
            // Level 3
            load_symbol!(sgemm, CublasSgemm, b"cublasSgemm_v2\0");
            load_symbol!(dgemm, CublasDgemm, b"cublasDgemm_v2\0");
            load_symbol!(sgemm_batched, CublasSgemmBatched, b"cublasSgemmBatched\0");
            load_symbol!(sgemm_strided_batched, CublasSgemmStridedBatched, b"cublasSgemmStridedBatched\0");
            
            let lib_ref = &*(lib as *const Library);
            
            Ok(Self {
                _lib: ptr::read(lib_ref),
                create, destroy, set_stream, set_pointer_mode, set_math_mode,
                saxpy, daxpy, sdot, ddot, snrm2, dnrm2, sscal, dscal,
                scopy, dcopy, isamax, idamax, sasum, dasum,
                sgemv, dgemv, sgemm, dgemm, sgemm_batched, sgemm_strided_batched,
            })
        }
    }
    
    /// Create a cuBLAS handle.
    pub fn create_handle(&self) -> GpuResult<CublasHandle> {
        let mut handle = CublasHandle::null();
        let status = unsafe { (self.create)(&mut handle) };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(handle)
    }
    
    /// Destroy a cuBLAS handle.
    pub fn destroy_handle(&self, handle: CublasHandle) -> GpuResult<()> {
        let status = unsafe { (self.destroy)(handle) };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
}

// =============================================================================
// cuBLAS Context
// =============================================================================

/// cuBLAS context for BLAS operations.
pub struct Cublas {
    lib: Arc<CublasLib>,
    handle: CublasHandle,
}

impl Cublas {
    /// Create a new cuBLAS context.
    pub fn new() -> GpuResult<Self> {
        let lib = Arc::new(CublasLib::load()?);
        let handle = lib.create_handle()?;
        
        Ok(Self { lib, handle })
    }
    
    /// Set the CUDA stream.
    pub fn set_stream(&self, stream: usize) -> GpuResult<()> {
        let status = unsafe { (self.lib.set_stream)(self.handle, stream) };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// Set pointer mode.
    pub fn set_pointer_mode(&self, mode: CublasPointerMode) -> GpuResult<()> {
        let status = unsafe { (self.lib.set_pointer_mode)(self.handle, mode as i32) };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// Set math mode (enable Tensor Cores).
    pub fn set_math_mode(&self, mode: CublasMathMode) -> GpuResult<()> {
        let status = unsafe { (self.lib.set_math_mode)(self.handle, mode as i32) };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    // =========================================================================
    // Level 1 BLAS
    // =========================================================================
    
    /// SAXPY: y = alpha * x + y (single precision)
    pub fn saxpy(&self, n: i32, alpha: f32, x: usize, incx: i32, y: usize, incy: i32) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.saxpy)(
                self.handle, n, &alpha,
                x as *const f32, incx,
                y as *mut f32, incy,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// DAXPY: y = alpha * x + y (double precision)
    pub fn daxpy(&self, n: i32, alpha: f64, x: usize, incx: i32, y: usize, incy: i32) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.daxpy)(
                self.handle, n, &alpha,
                x as *const f64, incx,
                y as *mut f64, incy,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// SDOT: dot product (single precision)
    pub fn sdot(&self, n: i32, x: usize, incx: i32, y: usize, incy: i32) -> GpuResult<f32> {
        let mut result: f32 = 0.0;
        let status = unsafe {
            (self.lib.sdot)(
                self.handle, n,
                x as *const f32, incx,
                y as *const f32, incy,
                &mut result,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(result)
    }
    
    /// SNRM2: Euclidean norm (single precision)
    pub fn snrm2(&self, n: i32, x: usize, incx: i32) -> GpuResult<f32> {
        let mut result: f32 = 0.0;
        let status = unsafe {
            (self.lib.snrm2)(self.handle, n, x as *const f32, incx, &mut result)
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(result)
    }
    
    /// SSCAL: x = alpha * x (single precision)
    pub fn sscal(&self, n: i32, alpha: f32, x: usize, incx: i32) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.sscal)(self.handle, n, &alpha, x as *mut f32, incx)
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// SCOPY: y = x (single precision)
    pub fn scopy(&self, n: i32, x: usize, incx: i32, y: usize, incy: i32) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.scopy)(
                self.handle, n,
                x as *const f32, incx,
                y as *mut f32, incy,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// ISAMAX: index of max absolute value (single precision)
    pub fn isamax(&self, n: i32, x: usize, incx: i32) -> GpuResult<i32> {
        let mut result: i32 = 0;
        let status = unsafe {
            (self.lib.isamax)(self.handle, n, x as *const f32, incx, &mut result)
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(result - 1) // cuBLAS returns 1-based index
    }
    
    /// SASUM: sum of absolute values (single precision)
    pub fn sasum(&self, n: i32, x: usize, incx: i32) -> GpuResult<f32> {
        let mut result: f32 = 0.0;
        let status = unsafe {
            (self.lib.sasum)(self.handle, n, x as *const f32, incx, &mut result)
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(result)
    }
    
    // =========================================================================
    // Level 2 BLAS
    // =========================================================================
    
    /// SGEMV: y = alpha * A * x + beta * y (single precision)
    pub fn sgemv(
        &self,
        trans: CublasOperation,
        m: i32, n: i32,
        alpha: f32,
        a: usize, lda: i32,
        x: usize, incx: i32,
        beta: f32,
        y: usize, incy: i32,
    ) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.sgemv)(
                self.handle, trans as i32, m, n, &alpha,
                a as *const f32, lda,
                x as *const f32, incx,
                &beta,
                y as *mut f32, incy,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    // =========================================================================
    // Level 3 BLAS
    // =========================================================================
    
    /// SGEMM: C = alpha * A * B + beta * C (single precision)
    pub fn sgemm(
        &self,
        transa: CublasOperation,
        transb: CublasOperation,
        m: i32, n: i32, k: i32,
        alpha: f32,
        a: usize, lda: i32,
        b: usize, ldb: i32,
        beta: f32,
        c: usize, ldc: i32,
    ) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.sgemm)(
                self.handle, transa as i32, transb as i32,
                m, n, k, &alpha,
                a as *const f32, lda,
                b as *const f32, ldb,
                &beta,
                c as *mut f32, ldc,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// DGEMM: C = alpha * A * B + beta * C (double precision)
    pub fn dgemm(
        &self,
        transa: CublasOperation,
        transb: CublasOperation,
        m: i32, n: i32, k: i32,
        alpha: f64,
        a: usize, lda: i32,
        b: usize, ldb: i32,
        beta: f64,
        c: usize, ldc: i32,
    ) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.dgemm)(
                self.handle, transa as i32, transb as i32,
                m, n, k, &alpha,
                a as *const f64, lda,
                b as *const f64, ldb,
                &beta,
                c as *mut f64, ldc,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
    
    /// SGEMM strided batched: batched matrix multiplication
    pub fn sgemm_strided_batched(
        &self,
        transa: CublasOperation,
        transb: CublasOperation,
        m: i32, n: i32, k: i32,
        alpha: f32,
        a: usize, lda: i32, stride_a: i64,
        b: usize, ldb: i32, stride_b: i64,
        beta: f32,
        c: usize, ldc: i32, stride_c: i64,
        batch_count: i32,
    ) -> GpuResult<()> {
        let status = unsafe {
            (self.lib.sgemm_strided_batched)(
                self.handle, transa as i32, transb as i32,
                m, n, k, &alpha,
                a as *const f32, lda, stride_a,
                b as *const f32, ldb, stride_b,
                &beta,
                c as *mut f32, ldc, stride_c,
                batch_count,
            )
        };
        
        if !CublasStatus::from_code(status).is_success() {
            return Err(CublasStatus::from_code(status).to_error());
        }
        
        Ok(())
    }
}

impl Drop for Cublas {
    fn drop(&mut self) {
        let _ = self.lib.destroy_handle(self.handle);
    }
}

// =============================================================================
// High-Level BLAS Operations
// =============================================================================

/// High-level BLAS operations on tensors.
pub struct BlasOps {
    cublas: Cublas,
}

impl BlasOps {
    /// Create a new BLAS operations context.
    pub fn new() -> GpuResult<Self> {
        let cublas = Cublas::new()?;
        // Enable Tensor Cores by default
        cublas.set_math_mode(CublasMathMode::TensorOpMath)?;
        
        Ok(Self { cublas })
    }
    
    /// Matrix multiplication: C = A @ B
    pub fn matmul(&self, a: &Tensor, b: &Tensor, c: &mut Tensor) -> GpuResult<()> {
        // Validate shapes
        if a.ndim() != 2 || b.ndim() != 2 || c.ndim() != 2 {
            return Err(GpuError::InvalidShape("matmul requires 2D tensors".to_string()));
        }
        
        let m = a.shape().dim(0) as i32;
        let k = a.shape().dim(1) as i32;
        let n = b.shape().dim(1) as i32;
        
        if b.shape().dim(0) as i32 != k {
            return Err(GpuError::DimensionMismatch(
                "matmul: inner dimensions must match".to_string()
            ));
        }
        
        // cuBLAS uses column-major, but we use row-major
        // So we compute C^T = B^T @ A^T, which gives us C in row-major
        self.cublas.sgemm(
            CublasOperation::None,
            CublasOperation::None,
            n, m, k,
            1.0,
            b.data_ptr().ptr, n,
            a.data_ptr().ptr, k,
            0.0,
            c.data_ptr().ptr, n,
        )
    }
    
    /// Batched matrix multiplication
    pub fn bmm(&self, a: &Tensor, b: &Tensor, c: &mut Tensor) -> GpuResult<()> {
        if a.ndim() != 3 || b.ndim() != 3 || c.ndim() != 3 {
            return Err(GpuError::InvalidShape("bmm requires 3D tensors".to_string()));
        }
        
        let batch = a.shape().dim(0) as i32;
        let m = a.shape().dim(1) as i32;
        let k = a.shape().dim(2) as i32;
        let n = b.shape().dim(2) as i32;
        
        let stride_a = (m * k) as i64;
        let stride_b = (k * n) as i64;
        let stride_c = (m * n) as i64;
        
        self.cublas.sgemm_strided_batched(
            CublasOperation::None,
            CublasOperation::None,
            n, m, k,
            1.0,
            b.data_ptr().ptr, n, stride_b,
            a.data_ptr().ptr, k, stride_a,
            0.0,
            c.data_ptr().ptr, n, stride_c,
            batch,
        )
    }
    
    /// Vector dot product
    pub fn dot(&self, x: &Tensor, y: &Tensor) -> GpuResult<f32> {
        if x.ndim() != 1 || y.ndim() != 1 {
            return Err(GpuError::InvalidShape("dot requires 1D tensors".to_string()));
        }
        
        let n = x.numel() as i32;
        
        self.cublas.sdot(n, x.data_ptr().ptr, 1, y.data_ptr().ptr, 1)
    }
    
    /// L2 norm
    pub fn norm(&self, x: &Tensor) -> GpuResult<f32> {
        let n = x.numel() as i32;
        self.cublas.snrm2(n, x.data_ptr().ptr, 1)
    }
    
    /// Scale: x = alpha * x
    pub fn scale(&self, alpha: f32, x: &mut Tensor) -> GpuResult<()> {
        let n = x.numel() as i32;
        self.cublas.sscal(n, alpha, x.data_ptr().ptr, 1)
    }
    
    /// AXPY: y = alpha * x + y
    pub fn axpy(&self, alpha: f32, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        let n = x.numel() as i32;
        self.cublas.saxpy(n, alpha, x.data_ptr().ptr, 1, y.data_ptr().ptr, 1)
    }
    
    /// Copy: y = x
    pub fn copy(&self, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        let n = x.numel() as i32;
        self.cublas.scopy(n, x.data_ptr().ptr, 1, y.data_ptr().ptr, 1)
    }
    
    /// Index of max absolute value
    pub fn argmax(&self, x: &Tensor) -> GpuResult<i32> {
        let n = x.numel() as i32;
        self.cublas.isamax(n, x.data_ptr().ptr, 1)
    }
    
    /// Sum of absolute values
    pub fn asum(&self, x: &Tensor) -> GpuResult<f32> {
        let n = x.numel() as i32;
        self.cublas.sasum(n, x.data_ptr().ptr, 1)
    }
    
    /// Matrix-vector multiplication: y = A @ x
    pub fn gemv(&self, a: &Tensor, x: &Tensor, y: &mut Tensor) -> GpuResult<()> {
        if a.ndim() != 2 || x.ndim() != 1 || y.ndim() != 1 {
            return Err(GpuError::InvalidShape("gemv: requires 2D matrix, 1D vectors".to_string()));
        }
        
        let m = a.shape().dim(0) as i32;
        let n = a.shape().dim(1) as i32;
        
        // Column-major adjustment
        self.cublas.sgemv(
            CublasOperation::Transpose,
            n, m,
            1.0,
            a.data_ptr().ptr, n,
            x.data_ptr().ptr, 1,
            0.0,
            y.data_ptr().ptr, 1,
        )
    }
}

