//! Runtime value representation.

use std::sync::Arc;
use std::sync::Mutex;
use crate::object::RoastObject;
use roast_codegen::Bytecode;

/// A Roast runtime value.
#[derive(Clone, Debug)]
pub enum Value {
    /// None/nil value
    None,
    /// Boolean
    Bool(bool),
    /// Integer (arbitrary precision for now)
    Int(i64),
    /// Float
    Float(f64),
    /// String
    Str(Arc<str>),
    /// Bytes
    Bytes(Arc<[u8]>),
    /// List (mutable via interior mutability)
    List(Arc<Mutex<Vec<Value>>>),
    /// Tuple
    Tuple(Arc<[Value]>),
    /// Dict (mutable via interior mutability)
    Dict(Arc<Mutex<std::collections::HashMap<ValueKey, Value>>>),
    /// Set (mutable via interior mutability)
    Set(Arc<Mutex<std::collections::HashSet<ValueKey>>>),
    /// Object
    Object(Arc<dyn RoastObject>),
    /// Class definition
    Class(Arc<RoastClass>),
    /// Class instance
    Instance(Arc<Mutex<RoastInstance>>),
    /// Function
    Function(Arc<RoastFunction>),
    /// Coroutine (async function result)
    Coroutine(Arc<Coroutine>),
    /// Generator (yield-based iterator)
    Generator(Arc<Mutex<GeneratorState>>),
    /// Slice object for sequence slicing
    Slice {
        start: Option<i64>,
        stop: Option<i64>,
        step: Option<i64>,
    },
    /// Bound method (method bound to an instance)
    BoundMethod {
        receiver: Box<Value>,
        method: Arc<str>,
    },
    /// Iterator object with internal state
    Iterator(Arc<Mutex<IteratorState>>),
    /// Module namespace (for `import x` style imports)
    Module {
        name: Arc<str>,
        exports: Arc<std::collections::HashMap<String, Value>>,
    },
}

/// Iterator state for iteration support
#[derive(Clone, Debug)]
pub enum IteratorState {
    /// Range iterator
    Range { current: i64, stop: i64, step: i64 },
    /// List iterator (stores cloned items to avoid holding lock during iteration)
    List { items: Vec<Value>, index: usize },
    /// String iterator (character by character)
    Str { chars: Vec<char>, index: usize },
    /// Tuple iterator
    Tuple { items: Arc<[Value]>, index: usize },
    /// Set iterator
    Set { items: Vec<ValueKey>, index: usize },
    /// Dict iterator (iterates keys)
    Dict { keys: Vec<ValueKey>, index: usize },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::None, Value::None) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::List(a), Value::List(b)) => {
                let a_guard = a.lock().unwrap();
                let b_guard = b.lock().unwrap();
                *a_guard == *b_guard
            },
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => {
                let a_guard = a.lock().unwrap();
                let b_guard = b.lock().unwrap();
                *a_guard == *b_guard
            },
            (Value::Dict(a), Value::Dict(b)) => {
                let a_guard = a.lock().unwrap();
                let b_guard = b.lock().unwrap();
                *a_guard == *b_guard
            },
            (Value::Module { name: a, .. }, Value::Module { name: b, .. }) => a == b,
            _ => false,
        }
    }
}

/// A hashable value (for dict keys and sets).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueKey {
    None,
    Bool(bool),
    Int(i64),
    Str(Arc<str>),
    Bytes(Arc<[u8]>),
    Tuple(Arc<[ValueKey]>),
}

impl From<ValueKey> for Value {
    fn from(key: ValueKey) -> Self {
        match key {
            ValueKey::None => Value::None,
            ValueKey::Bool(b) => Value::Bool(b),
            ValueKey::Int(n) => Value::Int(n),
            ValueKey::Str(s) => Value::Str(s),
            ValueKey::Bytes(b) => Value::Bytes(b),
            ValueKey::Tuple(t) => {
                let values: Vec<Value> = t.iter().cloned().map(Value::from).collect();
                Value::Tuple(Arc::from(values.into_boxed_slice()))
            }
        }
    }
}

impl Value {
    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
            Value::List(l) => !l.lock().unwrap().is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::Dict(d) => !d.lock().unwrap().is_empty(),
            Value::Set(s) => !s.lock().unwrap().is_empty(),
            _ => true,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::None
    }
}

/// A Roast class definition.
pub struct RoastClass {
    pub name: String,
    /// Methods defined on this class (name -> bytecode)
    pub methods: std::collections::HashMap<String, Arc<RoastFunction>>,
}

impl std::fmt::Debug for RoastClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class '{}'>", self.name)
    }
}

/// A Roast class instance.
pub struct RoastInstance {
    pub class: Arc<RoastClass>,
    pub attrs: std::collections::HashMap<String, Value>,
}

impl std::fmt::Debug for RoastInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{} instance>", self.class.name)
    }
}

/// A Roast function.
pub struct RoastFunction {
    pub name: String,
    pub arity: usize,
    pub code: Arc<Bytecode>,
    /// Whether this is an async function.
    pub is_async: bool,
}

impl std::fmt::Debug for RoastFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_async {
            write!(f, "<async function {}>", self.name)
        } else {
            write!(f, "<function {}>", self.name)
        }
    }
}

/// State of a coroutine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoroutineState {
    /// Created but not started.
    Created,
    /// Running (currently executing).
    Running,
    /// Suspended at an await point.
    Suspended,
    /// Completed with a result.
    Completed,
    /// Failed with an exception.
    Failed,
}

/// A coroutine (async function invocation).
pub struct Coroutine {
    /// The async function being executed.
    pub function: Arc<RoastFunction>,
    /// Current state.
    pub state: Mutex<CoroutineState>,
    /// Local variables snapshot.
    pub locals: Mutex<Vec<Value>>,
    /// Instruction pointer.
    pub ip: Mutex<usize>,
    /// Stack snapshot when suspended.
    pub stack: Mutex<Vec<Value>>,
    /// Result value (when completed).
    pub result: Mutex<Option<Value>>,
    /// Error message (when failed).
    pub error: Mutex<Option<String>>,
}

impl std::fmt::Debug for Coroutine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<coroutine {}>", self.function.name)
    }
}

impl Coroutine {
    /// Create a new coroutine from an async function.
    pub fn new(function: Arc<RoastFunction>, args: Vec<Value>) -> Self {
        Self {
            function,
            state: Mutex::new(CoroutineState::Created),
            locals: Mutex::new(args),
            ip: Mutex::new(0),
            stack: Mutex::new(Vec::new()),
            result: Mutex::new(None),
            error: Mutex::new(None),
        }
    }

    /// Returns true if the coroutine is done.
    pub fn is_done(&self) -> bool {
        let state = self.state.lock().unwrap();
        matches!(*state, CoroutineState::Completed | CoroutineState::Failed)
    }
}

/// State of a generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratorStatus {
    /// Created but not started.
    Created,
    /// Running (currently executing).
    Running,
    /// Suspended at a yield point.
    Suspended,
    /// Exhausted (no more values).
    Exhausted,
}

/// A generator state (for yield-based iterators).
pub struct GeneratorState {
    /// The generator function.
    pub function: Arc<RoastFunction>,
    /// Current status.
    pub status: GeneratorStatus,
    /// Local variables snapshot.
    pub locals: Vec<Value>,
    /// Instruction pointer.
    pub ip: usize,
    /// Stack snapshot when suspended.
    pub stack: Vec<Value>,
}

impl std::fmt::Debug for GeneratorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<generator {}>", self.function.name)
    }
}

impl GeneratorState {
    /// Create a new generator from a function.
    pub fn new(function: Arc<RoastFunction>, args: Vec<Value>) -> Self {
        Self {
            function,
            status: GeneratorStatus::Created,
            locals: args,
            ip: 0,
            stack: Vec::new(),
        }
    }
}
