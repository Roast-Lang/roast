//! Runtime library for Roast programs.

pub mod gc;
pub mod object;
pub mod value;
pub mod executor;
pub mod native;
pub mod native_full;

pub use object::RoastObject;
pub use value::{Value, ValueKey, RoastFunction, RoastClass, RoastInstance, Coroutine, CoroutineState, GeneratorState, GeneratorStatus};
pub use executor::{Executor, ExecutorConfig, TaskHandle, TaskId};

/// Initialize the Roast runtime.
pub fn init() {
    // Initialize global state, GC, etc.
}

/// Shutdown the runtime.
pub fn shutdown() {
    // Clean up resources
}

/// Roast runtime error.
#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub kind: RuntimeErrorKind,
}

/// Runtime error kinds.
#[derive(Debug, Clone, Copy)]
pub enum RuntimeErrorKind {
    TypeError,
    ValueError,
    IndexError,
    KeyError,
    AttributeError,
    NameError,
    ZeroDivisionError,
    OverflowError,
    MemoryError,
    RuntimeError,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for RuntimeError {}
