//! Roast Virtual Machine
//!
//! A stack-based bytecode interpreter for executing Roast programs.
//! The VM supports:
//! - Full Python-like semantics
//! - Ownership and borrowing at runtime
//! - Async/await execution
//! - Exception handling
//! - Garbage collection integration
//! - Optional JIT compilation (via `jit` feature)

pub mod interpreter;
pub mod frame;
pub mod stack;
pub mod heap;
pub mod builtins;
pub mod async_rt;
pub mod debug;
pub mod jit_integration;

pub use interpreter::{VM, VMConfig, VMError, VMResult};
pub use frame::{CallFrame, FrameState};
pub use stack::ValueStack;
pub use heap::{Heap, HeapObject};
pub use jit_integration::{JitManager, VMJitConfig, JitTier, TierUpAction, JitStats};

/// Prelude for the VM crate.
pub mod prelude {
    pub use crate::interpreter::{VM, VMConfig};
    pub use crate::frame::CallFrame;
    pub use crate::jit_integration::{JitManager, VMJitConfig};
}

