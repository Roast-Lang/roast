//! Foreign Function Interface for C extensions.
//!
//! Provides support for loading and calling C extensions,
//! including Python C-API compatible modules.

use crate::bridge::BridgeError;
use crate::types::PyValue;
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::path::Path;

/// C type representation for FFI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CType {
    Void,
    Bool,
    Char,
    Short,
    Int,
    Long,
    LongLong,
    UChar,
    UShort,
    UInt,
    ULong,
    ULongLong,
    Float,
    Double,
    Pointer(Box<CType>),
    Array(Box<CType>, usize),
    Struct(String, Vec<(String, CType)>),
    Function(Box<CType>, Vec<CType>),
}

impl CType {
    /// Size of this type in bytes.
    pub fn size(&self) -> usize {
        match self {
            CType::Void => 0,
            CType::Bool => 1,
            CType::Char | CType::UChar => 1,
            CType::Short | CType::UShort => 2,
            CType::Int | CType::UInt => 4,
            CType::Long | CType::ULong => 8,
            CType::LongLong | CType::ULongLong => 8,
            CType::Float => 4,
            CType::Double => 8,
            CType::Pointer(_) => 8, // 64-bit
            CType::Array(elem, count) => elem.size() * count,
            CType::Struct(_, fields) => {
                fields.iter().map(|(_, t)| t.size()).sum()
            }
            CType::Function(_, _) => 8, // Function pointer
        }
    }

    /// Alignment of this type.
    pub fn alignment(&self) -> usize {
        match self {
            CType::Void => 1,
            CType::Bool | CType::Char | CType::UChar => 1,
            CType::Short | CType::UShort => 2,
            CType::Int | CType::UInt | CType::Float => 4,
            CType::Long | CType::ULong | CType::LongLong | 
            CType::ULongLong | CType::Double | CType::Pointer(_) => 8,
            CType::Array(elem, _) => elem.alignment(),
            CType::Struct(_, fields) => {
                fields.iter().map(|(_, t)| t.alignment()).max().unwrap_or(1)
            }
            CType::Function(_, _) => 8,
        }
    }
}

/// A C function signature.
#[derive(Clone, Debug)]
pub struct CFunction {
    pub name: String,
    pub return_type: CType,
    pub param_types: Vec<CType>,
    pub variadic: bool,
}

impl CFunction {
    pub fn new(name: impl Into<String>, return_type: CType, param_types: Vec<CType>) -> Self {
        Self {
            name: name.into(),
            return_type,
            param_types,
            variadic: false,
        }
    }

    pub fn variadic(mut self) -> Self {
        self.variadic = true;
        self
    }
}

/// A loaded dynamic library.
pub struct Library {
    #[cfg(unix)]
    handle: *mut c_void,
    path: String,
    functions: HashMap<String, CFunction>,
}

impl Library {
    /// Loads a dynamic library from the given path.
    pub fn load(path: &Path) -> Result<Self, BridgeError> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            
            let path_cstr = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| BridgeError::ImportError("Invalid path".into()))?;
            
            let handle = unsafe {
                libc::dlopen(path_cstr.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL)
            };
            
            if handle.is_null() {
                let error = unsafe {
                    let err = libc::dlerror();
                    if err.is_null() {
                        "Unknown error".to_string()
                    } else {
                        CStr::from_ptr(err).to_string_lossy().to_string()
                    }
                };
                return Err(BridgeError::ImportError(format!("Failed to load library: {}", error)));
            }
            
            Ok(Self {
                handle,
                path: path.to_string_lossy().to_string(),
                functions: HashMap::new(),
            })
        }
        
        #[cfg(not(unix))]
        {
            Err(BridgeError::ImportError("FFI not supported on this platform".into()))
        }
    }

    /// Gets a function pointer from the library.
    pub fn get_symbol(&self, name: &str) -> Result<*mut c_void, BridgeError> {
        #[cfg(unix)]
        {
            let name_cstr = CString::new(name)
                .map_err(|_| BridgeError::ImportError("Invalid symbol name".into()))?;
            
            let symbol = unsafe {
                libc::dlsym(self.handle, name_cstr.as_ptr())
            };
            
            if symbol.is_null() {
                Err(BridgeError::FunctionNotFound(format!("Symbol not found: {}", name)))
            } else {
                Ok(symbol)
            }
        }
        
        #[cfg(not(unix))]
        {
            Err(BridgeError::ImportError("FFI not supported".into()))
        }
    }

    /// Registers a function signature for type checking.
    pub fn register_function(&mut self, func: CFunction) {
        self.functions.insert(func.name.clone(), func);
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            if !self.handle.is_null() {
                libc::dlclose(self.handle);
            }
        }
    }
}

/// FFI manager for loading and calling C functions.
pub struct FFI {
    libraries: HashMap<String, Library>,
    type_aliases: HashMap<String, CType>,
}

impl FFI {
    pub fn new() -> Self {
        let mut ffi = Self {
            libraries: HashMap::new(),
            type_aliases: HashMap::new(),
        };
        
        // Register common type aliases
        ffi.type_aliases.insert("size_t".into(), CType::ULong);
        ffi.type_aliases.insert("ssize_t".into(), CType::Long);
        ffi.type_aliases.insert("intptr_t".into(), CType::Long);
        ffi.type_aliases.insert("uintptr_t".into(), CType::ULong);
        ffi.type_aliases.insert("int8_t".into(), CType::Char);
        ffi.type_aliases.insert("uint8_t".into(), CType::UChar);
        ffi.type_aliases.insert("int16_t".into(), CType::Short);
        ffi.type_aliases.insert("uint16_t".into(), CType::UShort);
        ffi.type_aliases.insert("int32_t".into(), CType::Int);
        ffi.type_aliases.insert("uint32_t".into(), CType::UInt);
        ffi.type_aliases.insert("int64_t".into(), CType::LongLong);
        ffi.type_aliases.insert("uint64_t".into(), CType::ULongLong);
        
        ffi
    }

    /// Loads a library.
    pub fn load_library(&mut self, name: &str, path: &Path) -> Result<(), BridgeError> {
        let lib = Library::load(path)?;
        self.libraries.insert(name.to_string(), lib);
        Ok(())
    }

    /// Resolves a type alias.
    pub fn resolve_type(&self, name: &str) -> Option<&CType> {
        self.type_aliases.get(name)
    }

    /// Adds a type alias.
    pub fn add_type_alias(&mut self, name: impl Into<String>, ty: CType) {
        self.type_aliases.insert(name.into(), ty);
    }

    /// Gets a library by name.
    pub fn get_library(&self, name: &str) -> Option<&Library> {
        self.libraries.get(name)
    }

    /// Calls a C function.
    pub fn call(
        &self,
        library: &str,
        function: &str,
        args: Vec<PyValue>,
    ) -> Result<PyValue, BridgeError> {
        let lib = self.libraries.get(library)
            .ok_or_else(|| BridgeError::ModuleNotFound(library.to_string()))?;
        
        let _func = lib.functions.get(function)
            .ok_or_else(|| BridgeError::FunctionNotFound(function.to_string()))?;
        
        let _symbol = lib.get_symbol(function)?;
        
        // In a full implementation, this would:
        // 1. Convert PyValue args to C values
        // 2. Use libffi to call the function
        // 3. Convert return value back to PyValue
        
        // For now, return a placeholder
        Ok(PyValue::None)
    }
}

impl Default for FFI {
    fn default() -> Self {
        Self::new()
    }
}

/// Python C-API compatibility layer.
pub mod cpython {
    use super::*;

    /// Python object reference (opaque pointer).
    pub type PyObjectPtr = *mut c_void;

    /// Python C-API function signatures.
    pub struct CPythonAPI {
        py_initialize: Option<extern "C" fn()>,
        py_finalize: Option<extern "C" fn()>,
        py_incref: Option<extern "C" fn(PyObjectPtr)>,
        py_decref: Option<extern "C" fn(PyObjectPtr)>,
        py_run_string: Option<extern "C" fn(*const i8, i32, PyObjectPtr, PyObjectPtr) -> PyObjectPtr>,
    }

    impl CPythonAPI {
        /// Loads the Python C-API from libpython.
        pub fn load() -> Result<Self, BridgeError> {
            // In a full implementation, this would dlopen libpython
            // and resolve all the C-API symbols
            Ok(Self {
                py_initialize: None,
                py_finalize: None,
                py_incref: None,
                py_decref: None,
                py_run_string: None,
            })
        }

        /// Initializes the Python interpreter.
        pub fn initialize(&self) -> Result<(), BridgeError> {
            if let Some(init) = self.py_initialize {
                init();
                Ok(())
            } else {
                Err(BridgeError::NotInitialized)
            }
        }

        /// Finalizes the Python interpreter.
        pub fn finalize(&self) -> Result<(), BridgeError> {
            if let Some(finalize) = self.py_finalize {
                finalize();
                Ok(())
            } else {
                Err(BridgeError::NotInitialized)
            }
        }
    }
}

#[cfg(unix)]
mod libc {
    use std::ffi::c_void;
    
    pub const RTLD_NOW: i32 = 0x2;
    pub const RTLD_LOCAL: i32 = 0x0;
    
    extern "C" {
        pub fn dlopen(filename: *const i8, flags: i32) -> *mut c_void;
        pub fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
        pub fn dlclose(handle: *mut c_void) -> i32;
        pub fn dlerror() -> *mut i8;
    }
}

