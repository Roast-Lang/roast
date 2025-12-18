//! Bridge between Roast and Python runtimes.

use crate::types::PyValue;
use crate::modules::PyModule;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

/// Bridge errors.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Bridge not initialized")]
    NotInitialized,

    #[error("Module not found: {0}")]
    ModuleNotFound(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Python error: {0}")]
    PythonError(String),

    #[error("Import error: {0}")]
    ImportError(String),

    #[error("Attribute error: {0}")]
    AttributeError(String),
}

// ============================================================================

#[cfg(feature = "python-ffi")]
mod pyo3_impl {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::types::{PyDict, PyList, PyTuple, PyBytes, PyAnyMethods};

    /// The Python bridge handles communication with Python via PyO3.
    pub struct PythonBridge {
        /// Python path (for imports).
        python_path: Vec<PathBuf>,
        /// Whether the bridge is initialized.
        initialized: bool,
    }

    impl PythonBridge {
        /// Creates a new Python bridge.
        pub fn new() -> Self {
            Self {
                python_path: Vec::new(),
                initialized: false,
            }
        }

        /// Initializes the Python bridge.
        pub fn initialize(&mut self) -> Result<(), BridgeError> {
            if self.initialized {
                return Ok(());
            }

            // PyO3 auto-initializes Python when used
            Python::with_gil(|py| {
                // Add custom paths to sys.path if any
                if !self.python_path.is_empty() {
                    let sys = py.import_bound("sys")
                        .map_err(|e| BridgeError::PythonError(e.to_string()))?;
                    let path = sys.getattr("path")
                        .map_err(|e| BridgeError::PythonError(e.to_string()))?;
                    
                    for p in &self.python_path {
                        path.call_method1("insert", (0, p.to_string_lossy().to_string()))
                            .map_err(|e| BridgeError::PythonError(e.to_string()))?;
                    }
                }
                Ok::<_, BridgeError>(())
            })?;

            self.initialized = true;
            Ok(())
        }

        /// Adds a path to the Python import path.
        pub fn add_path(&mut self, path: PathBuf) {
            self.python_path.push(path);
        }

        /// Imports a Python module.
        pub fn import(&mut self, name: &str) -> Result<&crate::modules::PyModule, BridgeError> {
            if !self.initialized {
                self.initialize()?;
            }

            // For now, return a stub module since we can't easily return PyO3 refs
            // The actual calling happens through call_function
            Ok(crate::modules::PyModule::get_or_create_stub(name))
        }

        /// Calls a Python function.
        pub fn call(
            &mut self,
            module: &str,
            function: &str,
            args: Vec<PyValue>,
        ) -> Result<PyValue, BridgeError> {
            if !self.initialized {
                self.initialize()?;
            }

            Python::with_gil(|py| {
                // Import module
                let py_module = py.import_bound(module)
                    .map_err(|e| BridgeError::ModuleNotFound(format!("{}: {}", module, e)))?;
                
                // Get function
                let py_func = py_module.getattr(function)
                    .map_err(|e| BridgeError::FunctionNotFound(format!("{}.{}: {}", module, function, e)))?;
                
                // Convert args to Python
                let py_args: Vec<Py<PyAny>> = args.iter()
                    .map(|a| pyvalue_to_pyobject(py, a))
                    .collect();
                
                let args_tuple = PyTuple::new_bound(py, &py_args);
                
                // Call function
                let result = py_func.call1(args_tuple)
                    .map_err(|e| BridgeError::PythonError(e.to_string()))?;
                
                // Convert result back to PyValue
                pyobject_to_pyvalue(&result)
            })
        }

        /// Gets an attribute from a Python module.
        pub fn get_attr(&mut self, module: &str, attr: &str) -> Result<PyValue, BridgeError> {
            if !self.initialized {
                self.initialize()?;
            }

            Python::with_gil(|py| {
                let py_module = py.import_bound(module)
                    .map_err(|e| BridgeError::ModuleNotFound(format!("{}: {}", module, e)))?;
                
                let py_attr = py_module.getattr(attr)
                    .map_err(|e| BridgeError::AttributeError(format!("{}.{}: {}", module, attr, e)))?;
                
                pyobject_to_pyvalue(&py_attr)
            })
        }

        /// Evaluates Python code.
        pub fn eval(&mut self, code: &str) -> Result<PyValue, BridgeError> {
            if !self.initialized {
                self.initialize()?;
            }

            Python::with_gil(|py| {
                let result = py.eval_bound(code, None, None)
                    .map_err(|e| BridgeError::PythonError(e.to_string()))?;
                pyobject_to_pyvalue(&result)
            })
        }

        /// Executes Python code.
        pub fn exec(&mut self, code: &str) -> Result<(), BridgeError> {
            if !self.initialized {
                self.initialize()?;
            }

            Python::with_gil(|py| {
                py.run_bound(code, None, None)
                    .map_err(|e| BridgeError::PythonError(e.to_string()))
            })
        }

        /// Checks if a module is available.
        pub fn has_module(&self, name: &str) -> bool {
            Python::with_gil(|py| {
                py.import_bound(name).is_ok()
            })
        }

        /// Gets the Python version.
        pub fn python_version(&self) -> String {
            Python::with_gil(|py| {
                let sys = py.import_bound("sys").ok();
                sys.and_then(|s| s.getattr("version").ok())
                    .and_then(|v| v.extract::<String>().ok())
                    .unwrap_or_else(|| "unknown".to_string())
            })
        }
    }

    impl Default for PythonBridge {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Convert PyValue to Py<PyAny>
    fn pyvalue_to_pyobject(py: Python<'_>, value: &PyValue) -> Py<PyAny> {
        match value {
            PyValue::None => py.None(),
            PyValue::Bool(b) => b.into_py(py),
            PyValue::Int(n) => n.into_py(py),
            PyValue::Float(f) => f.into_py(py),
            PyValue::Str(s) => s.into_py(py),
            PyValue::Bytes(b) => PyBytes::new_bound(py, b).into(),
            PyValue::List(items) => {
                let py_items: Vec<Py<PyAny>> = items.iter()
                    .map(|i| pyvalue_to_pyobject(py, i))
                    .collect();
                PyList::new_bound(py, &py_items).into()
            }
            PyValue::Tuple(items) => {
                let py_items: Vec<Py<PyAny>> = items.iter()
                    .map(|i| pyvalue_to_pyobject(py, i))
                    .collect();
                PyTuple::new_bound(py, &py_items).into()
            }
            PyValue::Dict(map) => {
                let dict = PyDict::new_bound(py);
                for (k, v) in map {
                    let py_k = pykey_to_pyobject(py, k);
                    let py_v = pyvalue_to_pyobject(py, v);
                    dict.set_item(py_k, py_v).ok();
                }
                dict.into()
            }
            _ => py.None(),
        }
    }

    /// Convert PyKey to Py<PyAny>
    fn pykey_to_pyobject(py: Python<'_>, key: &crate::types::PyKey) -> Py<PyAny> {
        match key {
            crate::types::PyKey::None => py.None(),
            crate::types::PyKey::Bool(b) => b.into_py(py),
            crate::types::PyKey::Int(n) => n.into_py(py),
            crate::types::PyKey::Str(s) => s.into_py(py),
            crate::types::PyKey::Bytes(b) => PyBytes::new_bound(py, b).into(),
            crate::types::PyKey::Tuple(items) => {
                let py_items: Vec<Py<PyAny>> = items.iter()
                    .map(|i| pykey_to_pyobject(py, i))
                    .collect();
                PyTuple::new_bound(py, &py_items).into()
            }
        }
    }

    /// Convert Bound<PyAny> to PyValue
    fn pyobject_to_pyvalue(obj: &Bound<'_, PyAny>) -> Result<PyValue, BridgeError> {
        if obj.is_none() {
            return Ok(PyValue::None);
        }
        
        if let Ok(b) = obj.extract::<bool>() {
            return Ok(PyValue::Bool(b));
        }
        
        if let Ok(n) = obj.extract::<i64>() {
            return Ok(PyValue::Int(n));
        }
        
        if let Ok(f) = obj.extract::<f64>() {
            return Ok(PyValue::Float(f));
        }
        
        if let Ok(s) = obj.extract::<String>() {
            return Ok(PyValue::Str(s));
        }
        
        if let Ok(b) = obj.extract::<Vec<u8>>() {
            return Ok(PyValue::Bytes(b));
        }
        
        if let Ok(list) = obj.downcast::<PyList>() {
            let items: Result<Vec<PyValue>, _> = list.iter()
                .map(|item| pyobject_to_pyvalue(&item))
                .collect();
            return Ok(PyValue::List(items?));
        }
        
        if let Ok(tuple) = obj.downcast::<PyTuple>() {
            let items: Result<Vec<PyValue>, _> = tuple.iter()
                .map(|item| pyobject_to_pyvalue(&item))
                .collect();
            return Ok(PyValue::Tuple(items?));
        }
        
        // Default: represent as string
        let repr = obj.repr()
            .map(|r| r.to_string())
            .unwrap_or_else(|_| "<python object>".to_string());
        Ok(PyValue::Str(repr))
    }
}

// ============================================================================
// Stub Implementation (when python-ffi feature is NOT enabled)
// ============================================================================

#[cfg(not(feature = "python-ffi"))]
mod stub_impl {
    use super::*;

    /// The Python bridge - stub version without PyO3.
    pub struct PythonBridge {
        modules: FxHashMap<String, crate::modules::PyModule>,
        python_path: Vec<PathBuf>,
        initialized: bool,
    }

    impl PythonBridge {
        pub fn new() -> Self {
            Self {
                modules: FxHashMap::default(),
                python_path: Vec::new(),
                initialized: false,
            }
        }

        pub fn initialize(&mut self) -> Result<(), BridgeError> {
            self.initialized = true;
            Ok(())
        }

        pub fn add_path(&mut self, path: PathBuf) {
            self.python_path.push(path);
        }

        pub fn import(&mut self, name: &str) -> Result<&crate::modules::PyModule, BridgeError> {
            if !self.modules.contains_key(name) {
                let module = crate::modules::PyModule::stub(name);
                self.modules.insert(name.to_string(), module);
            }
            Ok(self.modules.get(name).unwrap())
        }

        pub fn call(
            &mut self,
            module: &str,
            function: &str,
            args: Vec<PyValue>,
        ) -> Result<PyValue, BridgeError> {
            Err(BridgeError::PythonError(
                "Python FFI not enabled. Rebuild with --features python-ffi".to_string()
            ))
        }

        pub fn get_attr(&mut self, module: &str, attr: &str) -> Result<PyValue, BridgeError> {
            Err(BridgeError::PythonError(
                "Python FFI not enabled. Rebuild with --features python-ffi".to_string()
            ))
        }

        pub fn eval(&mut self, _code: &str) -> Result<PyValue, BridgeError> {
            Err(BridgeError::PythonError(
                "Python FFI not enabled. Rebuild with --features python-ffi".to_string()
            ))
        }

        pub fn exec(&mut self, _code: &str) -> Result<(), BridgeError> {
            Err(BridgeError::PythonError(
                "Python FFI not enabled. Rebuild with --features python-ffi".to_string()
            ))
        }

        pub fn has_module(&self, name: &str) -> bool {
            matches!(name, 
                "builtins" | "sys" | "os" | "json" | "re" | "math" | 
                "collections" | "itertools" | "functools" | "typing"
            )
        }

        pub fn python_version(&self) -> String {
            "N/A (Python FFI not enabled)".to_string()
        }
    }

    impl Default for PythonBridge {
        fn default() -> Self {
            Self::new()
        }
    }
}

// Re-export based on feature
#[cfg(feature = "python-ffi")]
pub use pyo3_impl::PythonBridge;

#[cfg(not(feature = "python-ffi"))]
pub use stub_impl::PythonBridge;
