//! Debug Adapter Protocol (DAP) implementation for Roast.
//!
//! This crate provides a debugger for Roast programs following the
//! Debug Adapter Protocol specification.

pub mod protocol;
pub mod adapter;
pub mod breakpoint;
pub mod stack;
pub mod variables;
pub mod session;
pub mod hook;
pub mod watch;
pub mod vm_integration;

pub use adapter::DebugAdapter;
pub use session::DebugSession;
pub use breakpoint::{Breakpoint, BreakpointManager};
pub use watch::{WatchExpression, WatchManager, EvaluationContext, EvaluationResult};
pub use vm_integration::{VMDebugState, StepMode, LineMap, value_to_variable, extract_locals};

