//! Python decorator compatibility.
//!
//! Supports common Python decorators and allows custom decorator definitions.

use crate::bridge::BridgeError;
use crate::types::{PyValue, PyCallable, PyObject};
use std::collections::HashMap;
use std::sync::Arc;

/// A decorator function.
pub type DecoratorFn = Arc<dyn Fn(PyValue, Vec<PyValue>) -> Result<PyValue, BridgeError> + Send + Sync>;

/// Represents a Python decorator.
#[derive(Clone)]
pub struct Decorator {
    pub name: String,
    pub handler: DecoratorFn,
    pub takes_args: bool,
}

impl Decorator {
    /// Creates a new decorator.
    pub fn new<F>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(PyValue, Vec<PyValue>) -> Result<PyValue, BridgeError> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            handler: Arc::new(handler),
            takes_args: false,
        }
    }

    /// Creates a decorator that takes arguments.
    pub fn with_args<F>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(PyValue, Vec<PyValue>) -> Result<PyValue, BridgeError> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            handler: Arc::new(handler),
            takes_args: true,
        }
    }

    /// Applies this decorator to a value.
    pub fn apply(&self, target: PyValue, args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
        (self.handler)(target, args)
    }
}

impl std::fmt::Debug for Decorator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decorator")
            .field("name", &self.name)
            .field("takes_args", &self.takes_args)
            .finish()
    }
}

/// Registry of available decorators.
pub struct DecoratorRegistry {
    decorators: HashMap<String, Decorator>,
}

impl DecoratorRegistry {
    /// Creates a new registry with standard decorators.
    pub fn new() -> Self {
        let mut registry = Self {
            decorators: HashMap::new(),
        };
        
        // Register standard Python decorators
        registry.register_standard_decorators();
        
        registry
    }

    /// Registers the standard Python decorators.
    fn register_standard_decorators(&mut self) {
        // @staticmethod
        self.register(Decorator::new("staticmethod", |target, _args| {
            match target {
                PyValue::Callable(c) => {
                    Ok(PyValue::Object(PyObject {
                        class_name: "staticmethod".into(),
                        attributes: {
                            let mut attrs = rustc_hash::FxHashMap::default();
                            attrs.insert("__func__".into(), PyValue::Callable(c));
                            attrs
                        },
                    }))
                }
                _ => Err(BridgeError::TypeError("staticmethod requires a callable".into())),
            }
        }));

        // @classmethod
        self.register(Decorator::new("classmethod", |target, _args| {
            match target {
                PyValue::Callable(c) => {
                    Ok(PyValue::Object(PyObject {
                        class_name: "classmethod".into(),
                        attributes: {
                            let mut attrs = rustc_hash::FxHashMap::default();
                            attrs.insert("__func__".into(), PyValue::Callable(c));
                            attrs
                        },
                    }))
                }
                _ => Err(BridgeError::TypeError("classmethod requires a callable".into())),
            }
        }));

        // @property
        self.register(Decorator::new("property", |target, _args| {
            match target {
                PyValue::Callable(c) => {
                    Ok(PyValue::Object(PyObject {
                        class_name: "property".into(),
                        attributes: {
                            let mut attrs = rustc_hash::FxHashMap::default();
                            attrs.insert("fget".into(), PyValue::Callable(c));
                            attrs.insert("fset".into(), PyValue::None);
                            attrs.insert("fdel".into(), PyValue::None);
                            attrs
                        },
                    }))
                }
                _ => Err(BridgeError::TypeError("property requires a callable".into())),
            }
        }));

        // @abstractmethod
        self.register(Decorator::new("abstractmethod", |target, _args| {
            match target {
                PyValue::Callable(mut c) => {
                    c.doc = Some("(abstract)".into());
                    Ok(PyValue::Callable(c))
                }
                _ => Err(BridgeError::TypeError("abstractmethod requires a callable".into())),
            }
        }));

        // @dataclass (with args)
        self.register(Decorator::with_args("dataclass", |target, args| {
            // Parse dataclass options
            let frozen = args.iter().any(|a| matches!(a, PyValue::Bool(true)));
            
            match target {
                PyValue::Object(mut obj) => {
                    obj.attributes.insert("__dataclass_fields__".into(), PyValue::Dict(Default::default()));
                    if frozen {
                        obj.attributes.insert("__frozen__".into(), PyValue::Bool(true));
                    }
                    Ok(PyValue::Object(obj))
                }
                _ => Err(BridgeError::TypeError("dataclass requires a class".into())),
            }
        }));

        // @functools.lru_cache
        self.register(Decorator::with_args("lru_cache", |target, args| {
            let maxsize = args.first()
                .and_then(|a| if let PyValue::Int(n) = a { Some(*n) } else { None })
                .unwrap_or(128);
            
            match target {
                PyValue::Callable(c) => {
                    Ok(PyValue::Object(PyObject {
                        class_name: "lru_cache_wrapper".into(),
                        attributes: {
                            let mut attrs = rustc_hash::FxHashMap::default();
                            attrs.insert("__wrapped__".into(), PyValue::Callable(c));
                            attrs.insert("maxsize".into(), PyValue::Int(maxsize));
                            attrs.insert("cache".into(), PyValue::Dict(Default::default()));
                            attrs
                        },
                    }))
                }
                _ => Err(BridgeError::TypeError("lru_cache requires a callable".into())),
            }
        }));

        // @contextmanager
        self.register(Decorator::new("contextmanager", |target, _args| {
            match target {
                PyValue::Callable(c) => {
                    Ok(PyValue::Object(PyObject {
                        class_name: "contextmanager".into(),
                        attributes: {
                            let mut attrs = rustc_hash::FxHashMap::default();
                            attrs.insert("__wrapped__".into(), PyValue::Callable(c));
                            attrs
                        },
                    }))
                }
                _ => Err(BridgeError::TypeError("contextmanager requires a callable".into())),
            }
        }));

        // @deprecated (custom)
        self.register(Decorator::with_args("deprecated", |target, args| {
            let message = args.first()
                .and_then(|a| if let PyValue::Str(s) = a { Some(s.clone()) } else { None })
                .unwrap_or_else(|| "This is deprecated".into());
            
            match target {
                PyValue::Callable(mut c) => {
                    c.doc = Some(format!("[DEPRECATED] {}", message));
                    Ok(PyValue::Callable(c))
                }
                _ => Ok(target),
            }
        }));

        // @override
        self.register(Decorator::new("override", |target, _args| {
            // Just a marker decorator, return as-is
            Ok(target)
        }));

        // @final
        self.register(Decorator::new("final", |target, _args| {
            match target {
                PyValue::Object(mut obj) => {
                    obj.attributes.insert("__final__".into(), PyValue::Bool(true));
                    Ok(PyValue::Object(obj))
                }
                _ => Ok(target),
            }
        }));
    }

    /// Registers a decorator.
    pub fn register(&mut self, decorator: Decorator) {
        self.decorators.insert(decorator.name.clone(), decorator);
    }

    /// Gets a decorator by name.
    pub fn get(&self, name: &str) -> Option<&Decorator> {
        self.decorators.get(name)
    }

    /// Applies a decorator to a target.
    pub fn apply(
        &self,
        name: &str,
        target: PyValue,
        args: Vec<PyValue>,
    ) -> Result<PyValue, BridgeError> {
        let decorator = self.get(name)
            .ok_or_else(|| BridgeError::FunctionNotFound(format!("Unknown decorator: {}", name)))?;
        
        decorator.apply(target, args)
    }

    /// Lists all registered decorators.
    pub fn list(&self) -> Vec<&str> {
        self.decorators.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for DecoratorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Decorator stack for applying multiple decorators.
pub struct DecoratorStack {
    decorators: Vec<(String, Vec<PyValue>)>,
}

impl DecoratorStack {
    pub fn new() -> Self {
        Self { decorators: Vec::new() }
    }

    /// Pushes a decorator onto the stack.
    pub fn push(&mut self, name: impl Into<String>, args: Vec<PyValue>) {
        self.decorators.push((name.into(), args));
    }

    /// Applies all decorators in order (bottom-up).
    pub fn apply(
        &self,
        registry: &DecoratorRegistry,
        target: PyValue,
    ) -> Result<PyValue, BridgeError> {
        let mut result = target;
        
        // Apply decorators in reverse order (like Python)
        for (name, args) in self.decorators.iter().rev() {
            result = registry.apply(name, result, args.clone())?;
        }
        
        Ok(result)
    }
}

impl Default for DecoratorStack {
    fn default() -> Self {
        Self::new()
    }
}

