//! Python module representation.

use crate::bridge::BridgeError;
use crate::types::PyValue;
use rustc_hash::FxHashMap;

/// A Python module.
#[derive(Clone, Debug)]
pub struct PyModule {
    /// Module name.
    pub name: String,
    /// Module attributes.
    pub attributes: FxHashMap<String, PyValue>,
    /// Module functions.
    pub functions: FxHashMap<String, PyFunction>,
    /// Whether this is a stub module.
    pub is_stub: bool,
}

impl PyModule {
    /// Creates a new module.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: FxHashMap::default(),
            functions: FxHashMap::default(),
            is_stub: false,
        }
    }

    /// Creates a stub module (for compatibility without Python).
    pub fn stub(name: impl Into<String>) -> Self {
        let name = name.into();
        let mut module = Self::new(&name);
        module.is_stub = true;
        
        // Add standard attributes
        module.attributes.insert("__name__".into(), PyValue::Str(name.clone()));
        module.attributes.insert("__doc__".into(), PyValue::None);
        
        // Add stub functions for common modules
        match name.as_str() {
            "json" => {
                module.add_function("loads", |args| {
                    if let Some(PyValue::Str(s)) = args.first() {
                        // Very basic JSON parsing
                        if s.starts_with('{') {
                            Ok(PyValue::Dict(FxHashMap::default()))
                        } else if s.starts_with('[') {
                            Ok(PyValue::List(vec![]))
                        } else if s == "null" {
                            Ok(PyValue::None)
                        } else if s == "true" {
                            Ok(PyValue::Bool(true))
                        } else if s == "false" {
                            Ok(PyValue::Bool(false))
                        } else if let Ok(n) = s.parse::<i64>() {
                            Ok(PyValue::Int(n))
                        } else if let Ok(f) = s.parse::<f64>() {
                            Ok(PyValue::Float(f))
                        } else {
                            Ok(PyValue::Str(s.trim_matches('"').to_string()))
                        }
                    } else {
                        Err(BridgeError::TypeError("expected string".into()))
                    }
                });
                module.add_function("dumps", |args| {
                    if let Some(v) = args.first() {
                        Ok(PyValue::Str(format!("{:?}", v)))
                    } else {
                        Err(BridgeError::TypeError("expected value".into()))
                    }
                });
            }
            "math" => {
                module.attributes.insert("pi".into(), PyValue::Float(std::f64::consts::PI));
                module.attributes.insert("e".into(), PyValue::Float(std::f64::consts::E));
                module.attributes.insert("inf".into(), PyValue::Float(f64::INFINITY));
                module.attributes.insert("nan".into(), PyValue::Float(f64::NAN));
                
                module.add_function("sqrt", |args| {
                    if let Some(PyValue::Float(f)) = args.first().cloned().or_else(|| args.first().and_then(|v| match v { PyValue::Int(n) => Some(PyValue::Float(*n as f64)), _ => None })) {
                        Ok(PyValue::Float(f.sqrt()))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
                module.add_function("sin", |args| {
                    if let Some(n) = get_float(&args, 0) {
                        Ok(PyValue::Float(n.sin()))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
                module.add_function("cos", |args| {
                    if let Some(n) = get_float(&args, 0) {
                        Ok(PyValue::Float(n.cos()))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
                module.add_function("tan", |args| {
                    if let Some(n) = get_float(&args, 0) {
                        Ok(PyValue::Float(n.tan()))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
                module.add_function("floor", |args| {
                    if let Some(n) = get_float(&args, 0) {
                        Ok(PyValue::Int(n.floor() as i64))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
                module.add_function("ceil", |args| {
                    if let Some(n) = get_float(&args, 0) {
                        Ok(PyValue::Int(n.ceil() as i64))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
                module.add_function("abs", |args| {
                    match args.first() {
                        Some(PyValue::Int(n)) => Ok(PyValue::Int(n.abs())),
                        Some(PyValue::Float(f)) => Ok(PyValue::Float(f.abs())),
                        _ => Err(BridgeError::TypeError("expected number".into())),
                    }
                });
                module.add_function("pow", |args| {
                    if let (Some(base), Some(exp)) = (get_float(&args, 0), get_float(&args, 1)) {
                        Ok(PyValue::Float(base.powf(exp)))
                    } else {
                        Err(BridgeError::TypeError("expected two numbers".into()))
                    }
                });
                module.add_function("log", |args| {
                    if let Some(n) = get_float(&args, 0) {
                        let base = get_float(&args, 1).unwrap_or(std::f64::consts::E);
                        Ok(PyValue::Float(n.log(base)))
                    } else {
                        Err(BridgeError::TypeError("expected number".into()))
                    }
                });
            }
            "os" => {
                module.add_function("getcwd", |_| {
                    Ok(PyValue::Str(std::env::current_dir()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| ".".to_string())))
                });
                module.add_function("getenv", |args| {
                    if let Some(PyValue::Str(name)) = args.first() {
                        Ok(std::env::var(name)
                            .map(PyValue::Str)
                            .unwrap_or(PyValue::None))
                    } else {
                        Err(BridgeError::TypeError("expected string".into()))
                    }
                });
                module.add_function("listdir", |args| {
                    let path = match args.first() {
                        Some(PyValue::Str(p)) => p.clone(),
                        None => ".".to_string(),
                        _ => return Err(BridgeError::TypeError("expected string".into())),
                    };
                    match std::fs::read_dir(&path) {
                        Ok(entries) => {
                            let items: Vec<_> = entries
                                .filter_map(|e| e.ok())
                                .map(|e| PyValue::Str(e.file_name().to_string_lossy().to_string()))
                                .collect();
                            Ok(PyValue::List(items))
                        }
                        Err(e) => Err(BridgeError::PythonError(e.to_string())),
                    }
                });
            }
            "re" => {
                module.add_function("match", |args| {
                    // Stub - would need regex crate
                    Ok(PyValue::None)
                });
                module.add_function("search", |args| {
                    Ok(PyValue::None)
                });
                module.add_function("findall", |args| {
                    Ok(PyValue::List(vec![]))
                });
            }
            "sys" => {
                module.attributes.insert("version".into(), 
                    PyValue::Str("3.11.0 (Roast compat)".into()));
                module.attributes.insert("platform".into(),
                    PyValue::Str(std::env::consts::OS.into()));
                module.attributes.insert("argv".into(),
                    PyValue::List(std::env::args().map(PyValue::Str).collect()));
                module.attributes.insert("path".into(),
                    PyValue::List(vec![]));
                
                module.add_function("exit", |args| {
                    let code = match args.first() {
                        Some(PyValue::Int(n)) => *n as i32,
                        _ => 0,
                    };
                    std::process::exit(code);
                });
            }
            _ => {}
        }
        
        module
    }

    /// Adds a function to the module.
    pub fn add_function<F>(&mut self, name: &str, func: F)
    where
        F: Fn(Vec<PyValue>) -> Result<PyValue, BridgeError> + 'static,
    {
        self.functions.insert(name.to_string(), PyFunction {
            name: name.to_string(),
            handler: Box::new(func),
        });
    }

    /// Gets an attribute.
    pub fn get_attr(&self, name: &str) -> Option<&PyValue> {
        self.attributes.get(name)
    }

    /// Calls a function in the module.
    pub fn call(&self, name: &str, args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
        if let Some(func) = self.functions.get(name) {
            (func.handler)(args)
        } else {
            Err(BridgeError::FunctionNotFound(format!("{}.{}", self.name, name)))
        }
    }
}

fn get_float(args: &[PyValue], idx: usize) -> Option<f64> {
    match args.get(idx) {
        Some(PyValue::Float(f)) => Some(*f),
        Some(PyValue::Int(n)) => Some(*n as f64),
        _ => None,
    }
}

/// A Python function.
pub struct PyFunction {
    pub name: String,
    pub handler: Box<dyn Fn(Vec<PyValue>) -> Result<PyValue, BridgeError>>,
}

impl Clone for PyFunction {
    fn clone(&self) -> Self {
        // Can't clone the handler, create a stub
        Self {
            name: self.name.clone(),
            handler: Box::new(|_| Ok(PyValue::None)),
        }
    }
}

impl std::fmt::Debug for PyFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<function {}>", self.name)
    }
}

/// Module loader for finding and loading Python modules.
pub struct ModuleLoader {
    /// Search paths for modules.
    paths: Vec<std::path::PathBuf>,
}

impl ModuleLoader {
    /// Creates a new module loader.
    pub fn new() -> Self {
        Self { paths: Vec::new() }
    }

    /// Adds a search path.
    pub fn add_path(&mut self, path: std::path::PathBuf) {
        self.paths.push(path);
    }

    /// Finds a module by name.
    pub fn find(&self, name: &str) -> Option<std::path::PathBuf> {
        let parts: Vec<_> = name.split('.').collect();
        let filename = format!("{}.py", parts.last().unwrap_or(&""));
        
        for base in &self.paths {
            let mut path = base.clone();
            for part in &parts[..parts.len().saturating_sub(1)] {
                path.push(part);
            }
            path.push(&filename);
            
            if path.exists() {
                return Some(path);
            }
        }
        
        None
    }
}

impl Default for ModuleLoader {
    fn default() -> Self {
        Self::new()
    }
}

