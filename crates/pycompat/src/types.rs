//! Python-compatible type representations.

use rustc_hash::FxHashMap;
use std::hash::{Hash, Hasher};

/// A Python-compatible value.
#[derive(Clone, Debug)]
pub enum PyValue {
    /// None/nil value.
    None,
    /// Boolean.
    Bool(bool),
    /// Integer (Python uses arbitrary precision, we use i64 for now).
    Int(i64),
    /// Float.
    Float(f64),
    /// String.
    Str(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// List.
    List(Vec<PyValue>),
    /// Tuple.
    Tuple(Vec<PyValue>),
    /// Dictionary.
    Dict(FxHashMap<PyKey, PyValue>),
    /// Set.
    Set(Vec<PyKey>),
    /// Callable (function, method).
    Callable(PyCallable),
    /// Class instance.
    Object(PyObject),
    /// Exception.
    Exception(PyException),
    /// Slice.
    Slice(PySlice),
}

impl PartialEq for PyValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PyValue::None, PyValue::None) => true,
            (PyValue::Bool(a), PyValue::Bool(b)) => a == b,
            (PyValue::Int(a), PyValue::Int(b)) => a == b,
            (PyValue::Float(a), PyValue::Float(b)) => a == b,
            (PyValue::Str(a), PyValue::Str(b)) => a == b,
            (PyValue::Bytes(a), PyValue::Bytes(b)) => a == b,
            (PyValue::List(a), PyValue::List(b)) => a == b,
            (PyValue::Tuple(a), PyValue::Tuple(b)) => a == b,
            _ => false,
        }
    }
}

impl PyValue {
    /// Returns true if the value is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            PyValue::None => false,
            PyValue::Bool(b) => *b,
            PyValue::Int(n) => *n != 0,
            PyValue::Float(f) => *f != 0.0,
            PyValue::Str(s) => !s.is_empty(),
            PyValue::Bytes(b) => !b.is_empty(),
            PyValue::List(l) => !l.is_empty(),
            PyValue::Tuple(t) => !t.is_empty(),
            PyValue::Dict(d) => !d.is_empty(),
            PyValue::Set(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// Gets the type name.
    pub fn type_name(&self) -> &str {
        match self {
            PyValue::None => "NoneType",
            PyValue::Bool(_) => "bool",
            PyValue::Int(_) => "int",
            PyValue::Float(_) => "float",
            PyValue::Str(_) => "str",
            PyValue::Bytes(_) => "bytes",
            PyValue::List(_) => "list",
            PyValue::Tuple(_) => "tuple",
            PyValue::Dict(_) => "dict",
            PyValue::Set(_) => "set",
            PyValue::Callable(_) => "function",
            PyValue::Object(o) => &o.class_name,
            PyValue::Exception(e) => &e.type_name,
            PyValue::Slice(_) => "slice",
        }
    }

    /// Converts to Roast Value.
    pub fn to_roast(&self) -> roast_runtime::Value {
        match self {
            PyValue::None => roast_runtime::Value::None,
            PyValue::Bool(b) => roast_runtime::Value::Bool(*b),
            PyValue::Int(n) => roast_runtime::Value::Int(*n),
            PyValue::Float(f) => roast_runtime::Value::Float(*f),
            PyValue::Str(s) => roast_runtime::Value::Str(s.as_str().into()),
            PyValue::Bytes(b) => roast_runtime::Value::Bytes(b.as_slice().into()),
            PyValue::List(l) => roast_runtime::Value::List(
                std::sync::Arc::new(std::sync::Mutex::new(l.iter().map(|v| v.to_roast()).collect()))
            ),
            PyValue::Tuple(t) => roast_runtime::Value::Tuple(
                std::sync::Arc::from(t.iter().map(|v| v.to_roast()).collect::<Vec<_>>().into_boxed_slice())
            ),
            _ => roast_runtime::Value::None,
        }
    }

    /// Converts from Roast Value.
    pub fn from_roast(value: &roast_runtime::Value) -> Self {
        match value {
            roast_runtime::Value::None => PyValue::None,
            roast_runtime::Value::Bool(b) => PyValue::Bool(*b),
            roast_runtime::Value::Int(n) => PyValue::Int(*n),
            roast_runtime::Value::Float(f) => PyValue::Float(*f),
            roast_runtime::Value::Str(s) => PyValue::Str(s.to_string()),
            roast_runtime::Value::Bytes(b) => PyValue::Bytes(b.to_vec()),
            roast_runtime::Value::List(l) => PyValue::List(
                l.lock().unwrap().iter().map(|v| PyValue::from_roast(v)).collect()
            ),
            roast_runtime::Value::Tuple(t) => PyValue::Tuple(
                t.iter().map(|v| PyValue::from_roast(v)).collect()
            ),
            _ => PyValue::None,
        }
    }
}

/// A hashable key for dictionaries and sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PyKey {
    None,
    Bool(bool),
    Int(i64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<PyKey>),
}

impl Hash for PyKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            PyKey::None => {}
            PyKey::Bool(b) => b.hash(state),
            PyKey::Int(n) => n.hash(state),
            PyKey::Str(s) => s.hash(state),
            PyKey::Bytes(b) => b.hash(state),
            PyKey::Tuple(t) => t.hash(state),
        }
    }
}

impl PyKey {
    /// Converts a PyValue to a PyKey (if hashable).
    pub fn from_value(value: &PyValue) -> Option<Self> {
        match value {
            PyValue::None => Some(PyKey::None),
            PyValue::Bool(b) => Some(PyKey::Bool(*b)),
            PyValue::Int(n) => Some(PyKey::Int(*n)),
            PyValue::Str(s) => Some(PyKey::Str(s.clone())),
            PyValue::Bytes(b) => Some(PyKey::Bytes(b.clone())),
            PyValue::Tuple(t) => {
                let keys: Option<Vec<_>> = t.iter().map(PyKey::from_value).collect();
                keys.map(PyKey::Tuple)
            }
            _ => None,
        }
    }
}

/// A Python callable.
#[derive(Clone, Debug)]
pub struct PyCallable {
    pub name: String,
    pub doc: Option<String>,
}

/// A Python object (class instance).
#[derive(Clone, Debug)]
pub struct PyObject {
    pub class_name: String,
    pub attributes: FxHashMap<String, PyValue>,
}

impl PyObject {
    pub fn new(class_name: impl Into<String>) -> Self {
        Self {
            class_name: class_name.into(),
            attributes: FxHashMap::default(),
        }
    }
}

/// A Python exception.
#[derive(Clone, Debug)]
pub struct PyException {
    pub type_name: String,
    pub message: String,
    pub traceback: Vec<String>,
}

impl PyException {
    pub fn new(type_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            message: message.into(),
            traceback: Vec::new(),
        }
    }
}

/// A Python slice object.
#[derive(Clone, Debug)]
pub struct PySlice {
    pub start: Option<i64>,
    pub stop: Option<i64>,
    pub step: Option<i64>,
}

impl PySlice {
    /// Creates a new slice.
    pub fn new(start: Option<i64>, stop: Option<i64>, step: Option<i64>) -> Self {
        Self { start, stop, step }
    }

    /// Computes indices for a sequence of given length.
    pub fn indices(&self, length: usize) -> (i64, i64, i64) {
        let len = length as i64;
        let step = self.step.unwrap_or(1);

        let (start, stop) = if step > 0 {
            let start = self.start.map(|s| {
                if s < 0 { (len + s).max(0) } else { s.min(len) }
            }).unwrap_or(0);
            let stop = self.stop.map(|s| {
                if s < 0 { (len + s).max(0) } else { s.min(len) }
            }).unwrap_or(len);
            (start, stop)
        } else {
            let start = self.start.map(|s| {
                if s < 0 { len + s } else { s.min(len - 1) }
            }).unwrap_or(len - 1);
            let stop = self.stop.map(|s| {
                if s < 0 { len + s } else { s }
            }).unwrap_or(-1);
            (start, stop)
        };

        (start, stop, step)
    }
}
