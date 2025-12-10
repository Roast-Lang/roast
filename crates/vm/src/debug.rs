//! Debugger hooks for the VM.

use crate::interpreter::{VM, VMResult};

/// A hook that allows the debugger to inspect and control the VM.
pub trait DebugHook: Send + Sync {
    /// Called before executing an instruction.
    fn on_step(&self, vm: &VM) -> VMResult<()>;
    
    /// Called when entering a function.
    fn on_function_entry(&self, vm: &VM) -> VMResult<()>;
    
    /// Called when exiting a function.
    fn on_function_exit(&self, vm: &VM) -> VMResult<()>;
}
