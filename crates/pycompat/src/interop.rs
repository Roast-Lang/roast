//! Roast-Python interoperability.
//!
//! Provides seamless value conversion between Roast and Python.

use crate::bridge::BridgeError;
use crate::types::{PyValue, PyKey, PyObject, PyCallable, PySlice};
use roast_runtime::{Value, ValueKey, RoastFunction};
use roast_codegen::Bytecode;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::collections::HashMap;

/// Trait for converting Roast values to Python values.
pub trait RoastToPython {
    fn to_python(&self) -> PyValue;
}

/// Trait for converting Python values to Roast values.
pub trait PythonToRoast {
    fn to_roast(&self) -> Value;
}

impl RoastToPython for Value {
    fn to_python(&self) -> PyValue {
        match self {
            Value::None => PyValue::None,
            Value::Bool(b) => PyValue::Bool(*b),
            Value::Int(n) => PyValue::Int(*n),
            Value::Float(f) => PyValue::Float(*f),
            Value::Str(s) => PyValue::Str(s.to_string()),
            Value::Bytes(b) => PyValue::Bytes(b.to_vec()),
            Value::List(l) => PyValue::List(l.lock().unwrap().iter().map(|v| v.to_python()).collect()),
            Value::Tuple(t) => PyValue::Tuple(t.iter().map(|v| v.to_python()).collect()),
            Value::Dict(d) => {
                let mut py_dict = FxHashMap::default();
                for (k, v) in d.lock().unwrap().iter() {
                    if let Some(py_key) = key_to_python(k) {
                        py_dict.insert(py_key, v.to_python());
                    }
                }
                PyValue::Dict(py_dict)
            }
            Value::Set(s) => {
                let py_set: Vec<PyKey> = s.lock().unwrap().iter()
                    .filter_map(key_to_python)
                    .collect();
                PyValue::Set(py_set)
            }
            Value::Function(f) => PyValue::Callable(PyCallable {
                name: f.name.clone(),
                doc: None,
            }),
            Value::Object(_) => PyValue::Object(PyObject {
                class_name: "object".into(),
                attributes: FxHashMap::default(),
            }),
            Value::Class(c) => PyValue::Callable(PyCallable {
                name: format!("<class '{}'>", c.name),
                doc: None,
            }),
            Value::Instance(inst) => {
                let inst = inst.lock().unwrap();
                let mut attrs = FxHashMap::default();
                for (k, v) in inst.attrs.iter() {
                    if let Some(py_key) = PyKey::from_value(&PyValue::Str(k.clone())) {
                        attrs.insert(py_key, v.to_python());
                    }
                }
                PyValue::Object(PyObject {
                    class_name: inst.class.name.clone(),
                    attributes: FxHashMap::default(),
                })
            }
            Value::Coroutine(_) => PyValue::Callable(PyCallable {
                name: "<coroutine>".into(),
                doc: None,
            }),
            Value::Generator(_) => PyValue::Callable(PyCallable {
                name: "<generator>".into(),
                doc: None,
            }),
            Value::Slice { .. } => PyValue::None, // Slices aren't directly representable in PyValue
            Value::BoundMethod { receiver, method } => PyValue::Callable(PyCallable {
                name: format!("<bound method {}>", method),
                doc: None,
            }),
            Value::Iterator(_) => PyValue::Callable(PyCallable {
                name: "<iterator>".into(),
                doc: None,
            }),
            Value::Module { name, .. } => PyValue::Object(PyObject {
                class_name: format!("<module '{}'>", name),
                attributes: FxHashMap::default(),
            }),
        }
    }
}
fn key_to_python(key: &ValueKey) -> Option<PyKey> {
    match key {
        ValueKey::None => Some(PyKey::None),
        ValueKey::Bool(b) => Some(PyKey::Bool(*b)),
        ValueKey::Int(n) => Some(PyKey::Int(*n)),
        ValueKey::Str(s) => Some(PyKey::Str(s.to_string())),
        ValueKey::Bytes(b) => Some(PyKey::Bytes(b.to_vec())),
        ValueKey::Tuple(t) => {
            let keys: Option<Vec<PyKey>> = t.iter().map(key_to_python).collect();
            keys.map(PyKey::Tuple)
        }
    }
}

impl PythonToRoast for PyValue {
    fn to_roast(&self) -> Value {
        match self {
            PyValue::None => Value::None,
            PyValue::Bool(b) => Value::Bool(*b),
            PyValue::Int(n) => Value::Int(*n),
            PyValue::Float(f) => Value::Float(*f),
            PyValue::Str(s) => Value::Str(Arc::from(s.as_str())),
            PyValue::Bytes(b) => Value::Bytes(Arc::from(b.as_slice())),
            PyValue::List(l) => Value::List(Arc::new(std::sync::Mutex::new(l.iter().map(|v| v.to_roast()).collect()))),
            PyValue::Tuple(t) => Value::Tuple(Arc::from(t.iter().map(|v| v.to_roast()).collect::<Vec<_>>().into_boxed_slice())),
            PyValue::Dict(d) => {
                let mut roast_dict = HashMap::default();
                for (k, v) in d.iter() {
                    if let Some(roast_key) = key_to_roast(k) {
                        roast_dict.insert(roast_key, v.to_roast());
                    }
                }
                Value::Dict(Arc::new(std::sync::Mutex::new(roast_dict)))
            }
            PyValue::Set(s) => {
                let roast_set: std::collections::HashSet<ValueKey> = s.iter()
                    .filter_map(key_to_roast)
                    .collect();
                Value::Set(Arc::new(std::sync::Mutex::new(roast_set)))
            }
            PyValue::Callable(c) => Value::Function(Arc::new(RoastFunction {
                name: c.name.clone(),
                arity: 0,
                code: Arc::new(Bytecode::default()),
                is_async: false,
            })),
            PyValue::Object(_) | PyValue::Exception(_) | PyValue::Slice(_) => Value::None,
        }
    }
}

fn key_to_roast(key: &PyKey) -> Option<ValueKey> {
    match key {
        PyKey::None => Some(ValueKey::None),
        PyKey::Bool(b) => Some(ValueKey::Bool(*b)),
        PyKey::Int(n) => Some(ValueKey::Int(*n)),
        PyKey::Str(s) => Some(ValueKey::Str(Arc::from(s.as_str()))),
        PyKey::Bytes(b) => Some(ValueKey::Bytes(Arc::from(b.as_slice()))),
        PyKey::Tuple(t) => {
            let keys: Option<Vec<ValueKey>> = t.iter().map(key_to_roast).collect();
            keys.map(|k| ValueKey::Tuple(Arc::from(k.into_boxed_slice())))
        }
    }
}

/// Interop context for managing conversions.
pub struct InteropContext {
    /// Cached Python modules.
    cached_modules: FxHashMap<String, PyValue>,
    /// Type mappings.
    type_mappings: FxHashMap<String, String>,
}

impl InteropContext {
    pub fn new() -> Self {
        let mut ctx = Self {
            cached_modules: FxHashMap::default(),
            type_mappings: FxHashMap::default(),
        };

        // Register default type mappings
        ctx.type_mappings.insert("list".into(), "List".into());
        ctx.type_mappings.insert("dict".into(), "Dict".into());
        ctx.type_mappings.insert("set".into(), "Set".into());
        ctx.type_mappings.insert("tuple".into(), "Tuple".into());
        ctx.type_mappings.insert("str".into(), "str".into());
        ctx.type_mappings.insert("int".into(), "int".into());
        ctx.type_mappings.insert("float".into(), "float".into());
        ctx.type_mappings.insert("bool".into(), "bool".into());
        ctx.type_mappings.insert("NoneType".into(), "None".into());
        ctx.type_mappings.insert("bytes".into(), "bytes".into());

        ctx
    }

    /// Converts a Python type name to a Roast type name.
    pub fn python_to_roast_type(&self, py_type: &str) -> String {
        self.type_mappings.get(py_type)
            .cloned()
            .unwrap_or_else(|| py_type.to_string())
    }

    /// Caches a Python module.
    pub fn cache_module(&mut self, name: &str, module: PyValue) {
        self.cached_modules.insert(name.to_string(), module);
    }

    /// Gets a cached module.
    pub fn get_cached(&self, name: &str) -> Option<&PyValue> {
        self.cached_modules.get(name)
    }
}

impl Default for InteropContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps a Roast function to be callable from Python.
pub struct RoastCallable {
    pub name: String,
    pub func: Arc<dyn Fn(Vec<PyValue>) -> Result<PyValue, BridgeError> + Send + Sync>,
}

impl RoastCallable {
    pub fn new<F>(name: impl Into<String>, func: F) -> Self
    where
        F: Fn(Vec<PyValue>) -> Result<PyValue, BridgeError> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            func: Arc::new(func),
        }
    }

    pub fn call(&self, args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
        (self.func)(args)
    }
}

/// Exports Roast values to Python.
pub struct Exporter {
    exports: FxHashMap<String, PyValue>,
}

impl Exporter {
    pub fn new() -> Self {
        Self {
            exports: FxHashMap::default(),
        }
    }

    /// Exports a value with a given name.
    pub fn export(&mut self, name: impl Into<String>, value: PyValue) {
        self.exports.insert(name.into(), value);
    }

    /// Exports a Roast value.
    pub fn export_roast(&mut self, name: impl Into<String>, value: &Value) {
        self.exports.insert(name.into(), value.to_python());
    }

    /// Gets all exports as a module.
    pub fn as_module(&self, name: &str) -> crate::modules::PyModule {
        let mut module = crate::modules::PyModule::new(name);
        for (k, v) in &self.exports {
            module.attributes.insert(k.clone(), v.clone());
        }
        module
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Imports Python values into Roast.
pub struct Importer {
    context: InteropContext,
}

impl Importer {
    pub fn new() -> Self {
        Self {
            context: InteropContext::new(),
        }
    }

    /// Imports a Python value as a Roast value.
    pub fn import_value(&self, py_value: &PyValue) -> Value {
        py_value.to_roast()
    }

    /// Imports a Python module.
    pub fn import_module(&mut self, _name: &str, module: &crate::modules::PyModule) -> FxHashMap<String, Value> {
        let mut roast_module = FxHashMap::default();

        for (key, value) in &module.attributes {
            roast_module.insert(key.clone(), value.to_roast());
        }

        roast_module
    }
}

impl Default for Importer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roast_to_python() {
        let roast_int = Value::Int(42);
        let py_int = roast_int.to_python();
        assert!(matches!(py_int, PyValue::Int(42)));

        let roast_str = Value::Str(Arc::from("hello"));
        let py_str = roast_str.to_python();
        assert!(matches!(py_str, PyValue::Str(s) if s == "hello"));
    }

    #[test]
    fn test_python_to_roast() {
        let py_int = PyValue::Int(42);
        let roast_int = py_int.to_roast();
        assert!(matches!(roast_int, Value::Int(42)));

        let py_str = PyValue::Str("hello".into());
        let roast_str = py_str.to_roast();
        assert!(matches!(roast_str, Value::Str(s) if &*s == "hello"));
    }

    #[test]
    fn test_roundtrip() {
        // Int roundtrip
        let original = Value::Int(42);
        let converted = original.to_python().to_roast();
        assert!(matches!(converted, Value::Int(42)));

        // List roundtrip
        let original = Value::List(Arc::new(vec![Value::Int(1), Value::Int(2)]));
        let converted = original.to_python().to_roast();
        if let Value::List(l) = converted {
            assert_eq!(l.len(), 2);
        } else {
            panic!("Expected list");
        }
    }
}
