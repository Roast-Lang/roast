//! Python Compatibility Layer for Roast
//!
//! This crate provides comprehensive Python compatibility:
//! - Import and use Python modules (pure Python and C extensions)
//! - Call Python functions from Roast and vice versa
//! - Pass values between Roast and Python seamlessly
//! - Use Python's standard library
//! - Support for decorators and metaclasses
//! - FFI for C extensions
//! - Migration tools for Python-to-Roast conversion

pub mod bridge;
pub mod modules;
pub mod types;
pub mod builtins;
pub mod ffi;
pub mod decorators;
pub mod stdlib;
pub mod migrate;
pub mod interop;

pub use bridge::{PythonBridge, BridgeError};
pub use modules::{ModuleLoader, PyModule};
pub use types::{PyValue, PyKey, PyObject, PyCallable};
pub use ffi::{FFI, CFunction, CType};
pub use decorators::{Decorator, DecoratorRegistry};
pub use interop::{RoastToPython, PythonToRoast};
