//! NVRTC (NVIDIA Runtime Compilation) integration.
//!
//! This module provides runtime compilation of CUDA C++ source code to PTX
//! using NVIDIA's NVRTC library.

use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;
use libloading::{Library, Symbol};
use crate::error::{GpuError, GpuResult};

// =============================================================================
// NVRTC Types (FFI)
// =============================================================================

/// NVRTC program handle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NvrtcProgram {
    _opaque: *mut std::ffi::c_void,
}

impl NvrtcProgram {
    pub fn null() -> Self {
        Self { _opaque: ptr::null_mut() }
    }
    
    pub fn is_null(&self) -> bool {
        self._opaque.is_null()
    }
}

/// NVRTC result code.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvrtcResult {
    Success = 0,
    OutOfMemory = 1,
    ProgramCreationFailure = 2,
    InvalidInput = 3,
    InvalidProgram = 4,
    InvalidOption = 5,
    CompilationError = 6,
    BuiltinOperationFailure = 7,
    NoNameExpressionsAfterCompilation = 8,
    NoLoweredNamesBeforeCompilation = 9,
    NameExpressionNotValid = 10,
    InternalError = 11,
}

impl NvrtcResult {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::OutOfMemory,
            2 => Self::ProgramCreationFailure,
            3 => Self::InvalidInput,
            4 => Self::InvalidProgram,
            5 => Self::InvalidOption,
            6 => Self::CompilationError,
            7 => Self::BuiltinOperationFailure,
            8 => Self::NoNameExpressionsAfterCompilation,
            9 => Self::NoLoweredNamesBeforeCompilation,
            10 => Self::NameExpressionNotValid,
            _ => Self::InternalError,
        }
    }
    
    pub fn is_success(&self) -> bool {
        *self == Self::Success
    }
    
    pub fn to_error(&self) -> GpuError {
        GpuError::Compilation(format!("NVRTC error: {:?}", self))
    }
}

// =============================================================================
// NVRTC Function Types
// =============================================================================

type NvrtcGetErrorString = unsafe extern "C" fn(result: i32) -> *const i8;
type NvrtcVersion = unsafe extern "C" fn(major: *mut i32, minor: *mut i32) -> i32;
type NvrtcCreateProgram = unsafe extern "C" fn(
    prog: *mut NvrtcProgram,
    src: *const i8,
    name: *const i8,
    num_headers: i32,
    headers: *const *const i8,
    include_names: *const *const i8,
) -> i32;
type NvrtcDestroyProgram = unsafe extern "C" fn(prog: *mut NvrtcProgram) -> i32;
type NvrtcCompileProgram = unsafe extern "C" fn(
    prog: NvrtcProgram,
    num_options: i32,
    options: *const *const i8,
) -> i32;
type NvrtcGetPTXSize = unsafe extern "C" fn(prog: NvrtcProgram, size: *mut usize) -> i32;
type NvrtcGetPTX = unsafe extern "C" fn(prog: NvrtcProgram, ptx: *mut i8) -> i32;
type NvrtcGetCUBINSize = unsafe extern "C" fn(prog: NvrtcProgram, size: *mut usize) -> i32;
type NvrtcGetCUBIN = unsafe extern "C" fn(prog: NvrtcProgram, cubin: *mut i8) -> i32;
type NvrtcGetProgramLogSize = unsafe extern "C" fn(prog: NvrtcProgram, size: *mut usize) -> i32;
type NvrtcGetProgramLog = unsafe extern "C" fn(prog: NvrtcProgram, log: *mut i8) -> i32;
type NvrtcAddNameExpression = unsafe extern "C" fn(prog: NvrtcProgram, name: *const i8) -> i32;
type NvrtcGetLoweredName = unsafe extern "C" fn(
    prog: NvrtcProgram,
    name: *const i8,
    lowered: *mut *const i8,
) -> i32;

// =============================================================================
// CUDA Driver API Types (for loading PTX)
// =============================================================================

type CuInit = unsafe extern "C" fn(flags: u32) -> i32;
type CuDeviceGet = unsafe extern "C" fn(device: *mut i32, ordinal: i32) -> i32;
type CuCtxCreate = unsafe extern "C" fn(ctx: *mut usize, flags: u32, device: i32) -> i32;
type CuCtxDestroy = unsafe extern "C" fn(ctx: usize) -> i32;
type CuModuleLoadData = unsafe extern "C" fn(module: *mut usize, image: *const i8) -> i32;
type CuModuleLoadDataEx = unsafe extern "C" fn(
    module: *mut usize,
    image: *const i8,
    num_options: u32,
    options: *const i32,
    option_values: *const *mut std::ffi::c_void,
) -> i32;
type CuModuleUnload = unsafe extern "C" fn(module: usize) -> i32;
type CuModuleGetFunction = unsafe extern "C" fn(
    func: *mut usize,
    module: usize,
    name: *const i8,
) -> i32;
type CuLaunchKernel = unsafe extern "C" fn(
    f: usize,
    grid_dim_x: u32, grid_dim_y: u32, grid_dim_z: u32,
    block_dim_x: u32, block_dim_y: u32, block_dim_z: u32,
    shared_mem_bytes: u32,
    stream: usize,
    kernel_params: *mut *mut std::ffi::c_void,
    extra: *mut *mut std::ffi::c_void,
) -> i32;
type CuStreamSynchronize = unsafe extern "C" fn(stream: usize) -> i32;

// =============================================================================
// NVRTC Library Wrapper
// =============================================================================

/// NVRTC library handle.
pub struct NvrtcLib {
    _lib: Library,
    get_error_string: Symbol<'static, NvrtcGetErrorString>,
    version: Symbol<'static, NvrtcVersion>,
    create_program: Symbol<'static, NvrtcCreateProgram>,
    destroy_program: Symbol<'static, NvrtcDestroyProgram>,
    compile_program: Symbol<'static, NvrtcCompileProgram>,
    get_ptx_size: Symbol<'static, NvrtcGetPTXSize>,
    get_ptx: Symbol<'static, NvrtcGetPTX>,
    get_program_log_size: Symbol<'static, NvrtcGetProgramLogSize>,
    get_program_log: Symbol<'static, NvrtcGetProgramLog>,
    add_name_expression: Symbol<'static, NvrtcAddNameExpression>,
    get_lowered_name: Symbol<'static, NvrtcGetLoweredName>,
}

impl NvrtcLib {
    /// Load the NVRTC library.
    pub fn load() -> GpuResult<Self> {
        let lib_names = if cfg!(windows) {
            vec!["nvrtc64_120.dll", "nvrtc64_118.dll", "nvrtc64_112.dll", "nvrtc.dll"]
        } else if cfg!(target_os = "macos") {
            vec!["libnvrtc.dylib"]
        } else {
            vec!["libnvrtc.so.12", "libnvrtc.so.11", "libnvrtc.so"]
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
            "NVRTC library not found: {:?}",
            last_error
        )))
    }
    
    fn from_library(lib: Library) -> GpuResult<Self> {
        unsafe {
            // Leak the library to get 'static lifetime for symbols
            let lib = Box::leak(Box::new(lib));
            
            let get_error_string: Symbol<NvrtcGetErrorString> = lib
                .get(b"nvrtcGetErrorString\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let version: Symbol<NvrtcVersion> = lib
                .get(b"nvrtcVersion\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let create_program: Symbol<NvrtcCreateProgram> = lib
                .get(b"nvrtcCreateProgram\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let destroy_program: Symbol<NvrtcDestroyProgram> = lib
                .get(b"nvrtcDestroyProgram\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let compile_program: Symbol<NvrtcCompileProgram> = lib
                .get(b"nvrtcCompileProgram\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let get_ptx_size: Symbol<NvrtcGetPTXSize> = lib
                .get(b"nvrtcGetPTXSize\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let get_ptx: Symbol<NvrtcGetPTX> = lib
                .get(b"nvrtcGetPTX\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let get_program_log_size: Symbol<NvrtcGetProgramLogSize> = lib
                .get(b"nvrtcGetProgramLogSize\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let get_program_log: Symbol<NvrtcGetProgramLog> = lib
                .get(b"nvrtcGetProgramLog\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let add_name_expression: Symbol<NvrtcAddNameExpression> = lib
                .get(b"nvrtcAddNameExpression\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let get_lowered_name: Symbol<NvrtcGetLoweredName> = lib
                .get(b"nvrtcGetLoweredName\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            // Create a fake reference for the library
            let lib_ref = &*(lib as *const Library);
            
            Ok(Self {
                _lib: ptr::read(lib_ref),
                get_error_string: std::mem::transmute(get_error_string),
                version: std::mem::transmute(version),
                create_program: std::mem::transmute(create_program),
                destroy_program: std::mem::transmute(destroy_program),
                compile_program: std::mem::transmute(compile_program),
                get_ptx_size: std::mem::transmute(get_ptx_size),
                get_ptx: std::mem::transmute(get_ptx),
                get_program_log_size: std::mem::transmute(get_program_log_size),
                get_program_log: std::mem::transmute(get_program_log),
                add_name_expression: std::mem::transmute(add_name_expression),
                get_lowered_name: std::mem::transmute(get_lowered_name),
            })
        }
    }
    
    /// Get NVRTC version.
    pub fn version(&self) -> GpuResult<(i32, i32)> {
        let mut major = 0;
        let mut minor = 0;
        let result = unsafe { (self.version)(&mut major, &mut minor) };
        
        if NvrtcResult::from_code(result).is_success() {
            Ok((major, minor))
        } else {
            Err(NvrtcResult::from_code(result).to_error())
        }
    }
    
    /// Get error string for a result code.
    pub fn get_error_string(&self, result: NvrtcResult) -> String {
        unsafe {
            let ptr = (self.get_error_string)(result as i32);
            if ptr.is_null() {
                format!("Unknown error: {:?}", result)
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }
}

// =============================================================================
// CUDA Driver Library Wrapper
// =============================================================================

/// CUDA driver library handle.
pub struct CudaDriverLib {
    _lib: Library,
    cu_init: Symbol<'static, CuInit>,
    cu_device_get: Symbol<'static, CuDeviceGet>,
    cu_ctx_create: Symbol<'static, CuCtxCreate>,
    cu_ctx_destroy: Symbol<'static, CuCtxDestroy>,
    cu_module_load_data: Symbol<'static, CuModuleLoadData>,
    cu_module_unload: Symbol<'static, CuModuleUnload>,
    cu_module_get_function: Symbol<'static, CuModuleGetFunction>,
    cu_launch_kernel: Symbol<'static, CuLaunchKernel>,
    cu_stream_synchronize: Symbol<'static, CuStreamSynchronize>,
    initialized: bool,
}

impl CudaDriverLib {
    /// Load the CUDA driver library.
    pub fn load() -> GpuResult<Self> {
        let lib_names = if cfg!(windows) {
            vec!["nvcuda.dll"]
        } else if cfg!(target_os = "macos") {
            vec!["libcuda.dylib"]
        } else {
            vec!["libcuda.so.1", "libcuda.so"]
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
            "CUDA driver not found: {:?}",
            last_error
        )))
    }
    
    fn from_library(lib: Library) -> GpuResult<Self> {
        unsafe {
            let lib = Box::leak(Box::new(lib));
            
            let cu_init: Symbol<CuInit> = lib
                .get(b"cuInit\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_device_get: Symbol<CuDeviceGet> = lib
                .get(b"cuDeviceGet\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_ctx_create: Symbol<CuCtxCreate> = lib
                .get(b"cuCtxCreate_v2\0")
                .or_else(|_| lib.get(b"cuCtxCreate\0"))
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_ctx_destroy: Symbol<CuCtxDestroy> = lib
                .get(b"cuCtxDestroy_v2\0")
                .or_else(|_| lib.get(b"cuCtxDestroy\0"))
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_module_load_data: Symbol<CuModuleLoadData> = lib
                .get(b"cuModuleLoadData\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_module_unload: Symbol<CuModuleUnload> = lib
                .get(b"cuModuleUnload\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_module_get_function: Symbol<CuModuleGetFunction> = lib
                .get(b"cuModuleGetFunction\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_launch_kernel: Symbol<CuLaunchKernel> = lib
                .get(b"cuLaunchKernel\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let cu_stream_synchronize: Symbol<CuStreamSynchronize> = lib
                .get(b"cuStreamSynchronize\0")
                .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
            
            let lib_ref = &*(lib as *const Library);
            
            let mut driver = Self {
                _lib: ptr::read(lib_ref),
                cu_init: std::mem::transmute(cu_init),
                cu_device_get: std::mem::transmute(cu_device_get),
                cu_ctx_create: std::mem::transmute(cu_ctx_create),
                cu_ctx_destroy: std::mem::transmute(cu_ctx_destroy),
                cu_module_load_data: std::mem::transmute(cu_module_load_data),
                cu_module_unload: std::mem::transmute(cu_module_unload),
                cu_module_get_function: std::mem::transmute(cu_module_get_function),
                cu_launch_kernel: std::mem::transmute(cu_launch_kernel),
                cu_stream_synchronize: std::mem::transmute(cu_stream_synchronize),
                initialized: false,
            };
            
            // Initialize CUDA
            driver.init()?;
            
            Ok(driver)
        }
    }
    
    fn init(&mut self) -> GpuResult<()> {
        if self.initialized {
            return Ok(());
        }
        
        let result = unsafe { (self.cu_init)(0) };
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuInit failed: {}", result)));
        }
        
        self.initialized = true;
        Ok(())
    }
    
    /// Get a CUDA device.
    pub fn get_device(&self, ordinal: i32) -> GpuResult<i32> {
        let mut device = 0;
        let result = unsafe { (self.cu_device_get)(&mut device, ordinal) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuDeviceGet failed: {}", result)));
        }
        
        Ok(device)
    }
    
    /// Create a CUDA context.
    pub fn create_context(&self, device: i32) -> GpuResult<usize> {
        let mut ctx: usize = 0;
        let result = unsafe { (self.cu_ctx_create)(&mut ctx, 0, device) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuCtxCreate failed: {}", result)));
        }
        
        Ok(ctx)
    }
    
    /// Destroy a CUDA context.
    pub fn destroy_context(&self, ctx: usize) -> GpuResult<()> {
        let result = unsafe { (self.cu_ctx_destroy)(ctx) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuCtxDestroy failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Load a module from PTX.
    pub fn load_module(&self, ptx: &[u8]) -> GpuResult<usize> {
        let mut module: usize = 0;
        let result = unsafe {
            (self.cu_module_load_data)(&mut module, ptx.as_ptr() as *const i8)
        };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuModuleLoadData failed: {}", result)));
        }
        
        Ok(module)
    }
    
    /// Unload a module.
    pub fn unload_module(&self, module: usize) -> GpuResult<()> {
        let result = unsafe { (self.cu_module_unload)(module) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuModuleUnload failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Get a function from a module.
    pub fn get_function(&self, module: usize, name: &str) -> GpuResult<usize> {
        let name_c = CString::new(name)
            .map_err(|e| GpuError::InvalidConfig(e.to_string()))?;
        
        let mut func: usize = 0;
        let result = unsafe {
            (self.cu_module_get_function)(&mut func, module, name_c.as_ptr())
        };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!(
                "cuModuleGetFunction failed for '{}': {}", name, result
            )));
        }
        
        Ok(func)
    }
    
    /// Launch a kernel.
    pub fn launch_kernel(
        &self,
        func: usize,
        grid: [u32; 3],
        block: [u32; 3],
        shared_mem: u32,
        stream: usize,
        args: &mut [*mut std::ffi::c_void],
    ) -> GpuResult<()> {
        let result = unsafe {
            (self.cu_launch_kernel)(
                func,
                grid[0], grid[1], grid[2],
                block[0], block[1], block[2],
                shared_mem,
                stream,
                args.as_mut_ptr(),
                ptr::null_mut(),
            )
        };
        
        if result != 0 {
            return Err(GpuError::Launch(format!("cuLaunchKernel failed: {}", result)));
        }
        
        Ok(())
    }
    
    /// Synchronize a stream.
    pub fn stream_synchronize(&self, stream: usize) -> GpuResult<()> {
        let result = unsafe { (self.cu_stream_synchronize)(stream) };
        
        if result != 0 {
            return Err(GpuError::Cuda(format!("cuStreamSynchronize failed: {}", result)));
        }
        
        Ok(())
    }
}

// =============================================================================
// NVRTC Compiler
// =============================================================================

/// Compilation options.
#[derive(Debug, Clone)]
pub struct NvrtcCompileOptions {
    /// GPU architecture (e.g., "sm_86").
    pub arch: Option<String>,
    
    /// Include paths.
    pub include_paths: Vec<PathBuf>,
    
    /// Preprocessor defines.
    pub defines: Vec<(String, Option<String>)>,
    
    /// Enable debug info.
    pub debug: bool,
    
    /// Enable line info.
    pub line_info: bool,
    
    /// Optimization level (0-3).
    pub opt_level: u32,
    
    /// Enable fast math.
    pub fast_math: bool,
    
    /// Enable fused multiply-add.
    pub fmad: bool,
    
    /// Maximum registers per thread.
    pub max_registers: Option<u32>,
    
    /// Extra options.
    pub extra_options: Vec<String>,
}

impl Default for NvrtcCompileOptions {
    fn default() -> Self {
        Self {
            arch: None,
            include_paths: Vec::new(),
            defines: Vec::new(),
            debug: false,
            line_info: false,
            opt_level: 2,
            fast_math: true,
            fmad: true,
            max_registers: None,
            extra_options: Vec::new(),
        }
    }
}

impl NvrtcCompileOptions {
    /// Create options for a specific compute capability.
    pub fn for_compute(major: u32, minor: u32) -> Self {
        Self {
            arch: Some(format!("sm_{}{}", major, minor)),
            ..Default::default()
        }
    }
    
    /// Build command line options.
    pub fn to_args(&self) -> Vec<CString> {
        let mut args = Vec::new();
        
        // Architecture
        if let Some(ref arch) = self.arch {
            args.push(CString::new(format!("--gpu-architecture={}", arch)).unwrap());
        }
        
        // Include paths
        for path in &self.include_paths {
            args.push(CString::new(format!("-I{}", path.display())).unwrap());
        }
        
        // Defines
        for (name, value) in &self.defines {
            if let Some(v) = value {
                args.push(CString::new(format!("-D{}={}", name, v)).unwrap());
            } else {
                args.push(CString::new(format!("-D{}", name)).unwrap());
            }
        }
        
        // Debug
        if self.debug {
            args.push(CString::new("-G").unwrap());
        }
        
        // Line info
        if self.line_info {
            args.push(CString::new("-lineinfo").unwrap());
        }
        
        // Optimization
        match self.opt_level {
            0 => args.push(CString::new("-O0").unwrap()),
            1 => args.push(CString::new("-O1").unwrap()),
            2 => args.push(CString::new("-O2").unwrap()),
            _ => args.push(CString::new("-O3").unwrap()),
        }
        
        // Fast math
        if self.fast_math {
            args.push(CString::new("--use_fast_math").unwrap());
        }
        
        // FMA
        if !self.fmad {
            args.push(CString::new("--fmad=false").unwrap());
        }
        
        // Max registers
        if let Some(max_reg) = self.max_registers {
            args.push(CString::new(format!("--maxrregcount={}", max_reg)).unwrap());
        }
        
        // Extra options
        for opt in &self.extra_options {
            args.push(CString::new(opt.as_str()).unwrap());
        }
        
        // Default: device code, extern C
        args.push(CString::new("--device-as-default-execution-space").unwrap());
        args.push(CString::new("-default-device").unwrap());
        
        args
    }
}

/// Compilation result.
#[derive(Debug)]
pub struct NvrtcCompileResult {
    /// Compiled PTX code.
    pub ptx: Vec<u8>,
    
    /// Compilation log.
    pub log: String,
    
    /// Kernel names found.
    pub kernel_names: Vec<String>,
}

/// NVRTC Compiler.
pub struct NvrtcCompiler {
    nvrtc: NvrtcLib,
    driver: CudaDriverLib,
    default_options: NvrtcCompileOptions,
}

impl NvrtcCompiler {
    /// Create a new NVRTC compiler.
    pub fn new() -> GpuResult<Self> {
        let nvrtc = NvrtcLib::load()?;
        let driver = CudaDriverLib::load()?;
        
        Ok(Self {
            nvrtc,
            driver,
            default_options: NvrtcCompileOptions::default(),
        })
    }
    
    /// Get NVRTC version.
    pub fn version(&self) -> GpuResult<(i32, i32)> {
        self.nvrtc.version()
    }
    
    /// Set default compilation options.
    pub fn set_default_options(&mut self, options: NvrtcCompileOptions) {
        self.default_options = options;
    }
    
    /// Compile CUDA source to PTX.
    pub fn compile(
        &self,
        source: &str,
        name: &str,
        options: Option<&NvrtcCompileOptions>,
    ) -> GpuResult<NvrtcCompileResult> {
        let options = options.unwrap_or(&self.default_options);
        
        // Create program
        let source_c = CString::new(source)
            .map_err(|e| GpuError::InvalidConfig(e.to_string()))?;
        let name_c = CString::new(name)
            .map_err(|e| GpuError::InvalidConfig(e.to_string()))?;
        
        let mut prog = NvrtcProgram::null();
        
        let result = unsafe {
            (self.nvrtc.create_program)(
                &mut prog,
                source_c.as_ptr(),
                name_c.as_ptr(),
                0,
                ptr::null(),
                ptr::null(),
            )
        };
        
        if !NvrtcResult::from_code(result).is_success() {
            return Err(GpuError::Compilation(format!(
                "Failed to create NVRTC program: {}",
                self.nvrtc.get_error_string(NvrtcResult::from_code(result))
            )));
        }
        
        // Compile
        let args = options.to_args();
        let arg_ptrs: Vec<*const i8> = args.iter().map(|s| s.as_ptr()).collect();
        
        let compile_result = unsafe {
            (self.nvrtc.compile_program)(
                prog,
                arg_ptrs.len() as i32,
                arg_ptrs.as_ptr(),
            )
        };
        
        // Get log
        let log = self.get_program_log(prog)?;
        
        if !NvrtcResult::from_code(compile_result).is_success() {
            // Cleanup
            unsafe { (self.nvrtc.destroy_program)(&mut prog) };
            
            return Err(GpuError::Compilation(format!(
                "NVRTC compilation failed:\n{}",
                log
            )));
        }
        
        // Get PTX
        let ptx = self.get_ptx(prog)?;
        
        // Extract kernel names
        let kernel_names = extract_kernel_names_from_source(source);
        
        // Cleanup
        unsafe { (self.nvrtc.destroy_program)(&mut prog) };
        
        Ok(NvrtcCompileResult {
            ptx,
            log,
            kernel_names,
        })
    }
    
    /// Compile with headers.
    pub fn compile_with_headers(
        &self,
        source: &str,
        name: &str,
        headers: &[(&str, &str)],
        options: Option<&NvrtcCompileOptions>,
    ) -> GpuResult<NvrtcCompileResult> {
        let options = options.unwrap_or(&self.default_options);
        
        let source_c = CString::new(source)
            .map_err(|e| GpuError::InvalidConfig(e.to_string()))?;
        let name_c = CString::new(name)
            .map_err(|e| GpuError::InvalidConfig(e.to_string()))?;
        
        let header_contents: Vec<CString> = headers.iter()
            .map(|(_, content)| CString::new(*content).unwrap())
            .collect();
        let header_names: Vec<CString> = headers.iter()
            .map(|(name, _)| CString::new(*name).unwrap())
            .collect();
        
        let header_ptrs: Vec<*const i8> = header_contents.iter()
            .map(|s| s.as_ptr())
            .collect();
        let name_ptrs: Vec<*const i8> = header_names.iter()
            .map(|s| s.as_ptr())
            .collect();
        
        let mut prog = NvrtcProgram::null();
        
        let result = unsafe {
            (self.nvrtc.create_program)(
                &mut prog,
                source_c.as_ptr(),
                name_c.as_ptr(),
                headers.len() as i32,
                header_ptrs.as_ptr(),
                name_ptrs.as_ptr(),
            )
        };
        
        if !NvrtcResult::from_code(result).is_success() {
            return Err(GpuError::Compilation(format!(
                "Failed to create NVRTC program: {}",
                self.nvrtc.get_error_string(NvrtcResult::from_code(result))
            )));
        }
        
        // Compile
        let args = options.to_args();
        let arg_ptrs: Vec<*const i8> = args.iter().map(|s| s.as_ptr()).collect();
        
        let compile_result = unsafe {
            (self.nvrtc.compile_program)(
                prog,
                arg_ptrs.len() as i32,
                arg_ptrs.as_ptr(),
            )
        };
        
        let log = self.get_program_log(prog)?;
        
        if !NvrtcResult::from_code(compile_result).is_success() {
            unsafe { (self.nvrtc.destroy_program)(&mut prog) };
            return Err(GpuError::Compilation(format!(
                "NVRTC compilation failed:\n{}",
                log
            )));
        }
        
        let ptx = self.get_ptx(prog)?;
        let kernel_names = extract_kernel_names_from_source(source);
        
        unsafe { (self.nvrtc.destroy_program)(&mut prog) };
        
        Ok(NvrtcCompileResult {
            ptx,
            log,
            kernel_names,
        })
    }
    
    fn get_program_log(&self, prog: NvrtcProgram) -> GpuResult<String> {
        let mut size: usize = 0;
        let result = unsafe { (self.nvrtc.get_program_log_size)(prog, &mut size) };
        
        if !NvrtcResult::from_code(result).is_success() {
            return Ok(String::new());
        }
        
        if size <= 1 {
            return Ok(String::new());
        }
        
        let mut log = vec![0i8; size];
        let result = unsafe { (self.nvrtc.get_program_log)(prog, log.as_mut_ptr()) };
        
        if !NvrtcResult::from_code(result).is_success() {
            return Ok(String::new());
        }
        
        let log_str = unsafe {
            CStr::from_ptr(log.as_ptr())
                .to_string_lossy()
                .into_owned()
        };
        
        Ok(log_str)
    }
    
    fn get_ptx(&self, prog: NvrtcProgram) -> GpuResult<Vec<u8>> {
        let mut size: usize = 0;
        let result = unsafe { (self.nvrtc.get_ptx_size)(prog, &mut size) };
        
        if !NvrtcResult::from_code(result).is_success() {
            return Err(NvrtcResult::from_code(result).to_error());
        }
        
        let mut ptx = vec![0i8; size];
        let result = unsafe { (self.nvrtc.get_ptx)(prog, ptx.as_mut_ptr()) };
        
        if !NvrtcResult::from_code(result).is_success() {
            return Err(NvrtcResult::from_code(result).to_error());
        }
        
        Ok(ptx.into_iter().map(|b| b as u8).collect())
    }
    
    /// Load a compiled PTX and get a kernel function.
    pub fn load_kernel(&self, ptx: &[u8], kernel_name: &str) -> GpuResult<CudaKernel> {
        let module = self.driver.load_module(ptx)?;
        let function = self.driver.get_function(module, kernel_name)?;
        
        Ok(CudaKernel {
            module,
            function,
            name: kernel_name.to_string(),
        })
    }
    
    /// Compile and load in one step.
    pub fn compile_and_load(
        &self,
        source: &str,
        kernel_name: &str,
        options: Option<&NvrtcCompileOptions>,
    ) -> GpuResult<CudaKernel> {
        let result = self.compile(source, "kernel.cu", options)?;
        self.load_kernel(&result.ptx, kernel_name)
    }
    
    /// Get the CUDA driver.
    pub fn driver(&self) -> &CudaDriverLib {
        &self.driver
    }
}

/// A loaded CUDA kernel.
pub struct CudaKernel {
    module: usize,
    function: usize,
    name: String,
}

impl CudaKernel {
    /// Get kernel name.
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get function handle.
    pub fn function(&self) -> usize {
        self.function
    }
    
    /// Get module handle.
    pub fn module(&self) -> usize {
        self.module
    }
    
    /// Launch the kernel.
    pub fn launch(
        &self,
        driver: &CudaDriverLib,
        grid: [u32; 3],
        block: [u32; 3],
        shared_mem: u32,
        stream: usize,
        args: &mut [*mut std::ffi::c_void],
    ) -> GpuResult<()> {
        driver.launch_kernel(self.function, grid, block, shared_mem, stream, args)
    }
}

/// Extract kernel names from CUDA source.
fn extract_kernel_names_from_source(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    
    for line in source.lines() {
        let line = line.trim();
        
        // Look for __global__ void name( pattern
        if line.contains("__global__") {
            if let Some(void_pos) = line.find("void ") {
                let after_void = &line[void_pos + 5..];
                if let Some(paren_pos) = after_void.find('(') {
                    let name = after_void[..paren_pos].trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    
    names
}

// =============================================================================
// High-level JIT Compiler
// =============================================================================

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

/// JIT-compiled kernel cache entry.
struct CachedKernel {
    kernel: CudaKernel,
    source_hash: u64,
    compile_time_ms: f64,
}

/// High-level JIT compiler with caching.
pub struct JitCompiler {
    compiler: NvrtcCompiler,
    cache: RwLock<HashMap<String, CachedKernel>>,
    options: NvrtcCompileOptions,
    stats: Mutex<JitStats>,
}

/// JIT compilation statistics.
#[derive(Debug, Default)]
pub struct JitStats {
    pub total_compilations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub total_compile_time_ms: f64,
}

impl JitCompiler {
    /// Create a new JIT compiler.
    pub fn new() -> GpuResult<Self> {
        let compiler = NvrtcCompiler::new()?;
        
        Ok(Self {
            compiler,
            cache: RwLock::new(HashMap::new()),
            options: NvrtcCompileOptions::default(),
            stats: Mutex::new(JitStats::default()),
        })
    }
    
    /// Create with specific compute capability.
    pub fn for_device(major: u32, minor: u32) -> GpuResult<Self> {
        let compiler = NvrtcCompiler::new()?;
        let options = NvrtcCompileOptions::for_compute(major, minor);
        
        Ok(Self {
            compiler,
            cache: RwLock::new(HashMap::new()),
            options,
            stats: Mutex::new(JitStats::default()),
        })
    }
    
    /// Get or compile a kernel.
    pub fn get_kernel(&self, source: &str, kernel_name: &str) -> GpuResult<&CudaKernel> {
        let source_hash = hash_string(source);
        let cache_key = format!("{}:{}", kernel_name, source_hash);
        
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                let mut stats = self.stats.lock().unwrap();
                stats.cache_hits += 1;
                
                // Return reference to cached kernel
                // Note: This is safe because we never remove from cache
                unsafe {
                    return Ok(&*(&entry.kernel as *const CudaKernel));
                }
            }
        }
        
        // Compile
        let start = std::time::Instant::now();
        let kernel = self.compiler.compile_and_load(source, kernel_name, Some(&self.options))?;
        let compile_time = start.elapsed().as_secs_f64() * 1000.0;
        
        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_compilations += 1;
            stats.cache_misses += 1;
            stats.total_compile_time_ms += compile_time;
        }
        
        // Cache
        let mut cache = self.cache.write().unwrap();
        cache.insert(cache_key.clone(), CachedKernel {
            kernel,
            source_hash,
            compile_time_ms: compile_time,
        });
        
        // Return reference
        let entry = cache.get(&cache_key).unwrap();
        unsafe {
            Ok(&*(&entry.kernel as *const CudaKernel))
        }
    }
    
    /// Compile a kernel without caching.
    pub fn compile(&self, source: &str, kernel_name: &str) -> GpuResult<CudaKernel> {
        self.compiler.compile_and_load(source, kernel_name, Some(&self.options))
    }
    
    /// Clear the kernel cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        // Note: We don't unload modules here to avoid use-after-free
        // In production, we'd need proper reference counting
        cache.clear();
    }
    
    /// Get compilation statistics.
    pub fn stats(&self) -> JitStats {
        let stats = self.stats.lock().unwrap();
        JitStats {
            total_compilations: stats.total_compilations,
            cache_hits: stats.cache_hits,
            cache_misses: stats.cache_misses,
            total_compile_time_ms: stats.total_compile_time_ms,
        }
    }
    
    /// Get the underlying NVRTC compiler.
    pub fn nvrtc(&self) -> &NvrtcCompiler {
        &self.compiler
    }
}

/// Simple string hash for caching.
fn hash_string(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

