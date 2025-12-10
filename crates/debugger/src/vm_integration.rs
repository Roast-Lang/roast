//! VM integration for the debugger.
//!
//! This module provides the bridge between the VM and the debugger,
//! allowing extraction of variables, stack frames, and expression evaluation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use roast_runtime::Value;
use roast_vm::frame::CallFrame;
use roast_vm::interpreter::VM;

use crate::session::DebugSession;
use crate::stack::Frame;
use crate::variables::VariableValue;
use crate::watch::EvaluationContext;

/// Converts a VM Value to a debugger VariableValue.
pub fn value_to_variable(value: &Value) -> VariableValue {
    match value {
        Value::None => VariableValue::None,
        Value::Bool(b) => VariableValue::Bool(*b),
        Value::Int(i) => VariableValue::Int(*i),
        Value::Float(f) => VariableValue::Float(*f),
        Value::Str(s) => VariableValue::String(s.to_string()),
        Value::Bytes(b) => VariableValue::Bytes(b.to_vec()),
        Value::List(items) => {
            let items_guard = items.lock().unwrap();
            let converted: Vec<VariableValue> = items_guard.iter()
                .map(|v| value_to_variable(v))
                .collect();
            VariableValue::List(converted)
        }
        Value::Tuple(items) => {
            let converted: Vec<VariableValue> = items.iter()
                .map(|v| value_to_variable(v))
                .collect();
            VariableValue::List(converted) // Treat tuple as list for debugging
        }
        Value::Dict(map) => {
            let converted: HashMap<String, VariableValue> = map.lock().unwrap().iter()
                .map(|(k, v)| (format!("{:?}", k), value_to_variable(v)))
                .collect();
            VariableValue::Dict(converted)
        }
        Value::Set(items) => {
            // Convert set to list for debugging
            let converted: Vec<VariableValue> = items.lock().unwrap().iter()
                .map(|v| value_key_to_variable(v))
                .collect();
            VariableValue::List(converted)
        }
        Value::Function(f) => {
            VariableValue::String(format!("<function {}>", f.name))
        }
        Value::Object(obj) => {
            // Generic object representation
            VariableValue::String(format!("<object {:?}>", Arc::as_ptr(obj)))
        }
        Value::Class(c) => {
            VariableValue::String(format!("<class '{}'>", c.name))
        }
        Value::Instance(inst) => {
            let inst = inst.lock().unwrap();
            let converted: HashMap<String, VariableValue> = inst.attrs.iter()
                .map(|(k, v)| (k.clone(), value_to_variable(v)))
                .collect();
            VariableValue::Dict(converted)
        }
        Value::Coroutine(coro) => {
            VariableValue::String(format!("<coroutine {}>", coro.function.name))
        }
        Value::Generator(gen) => {
            let gen_state = gen.lock().unwrap();
            VariableValue::String(format!("<generator {}>", gen_state.function.name))
        }
        Value::Slice { start, stop, step } => {
            VariableValue::String(format!("slice({:?}, {:?}, {:?})", start, stop, step))
        }
        Value::BoundMethod { receiver: _, method } => {
            VariableValue::String(format!("<bound method {}>", method))
        }
        Value::Iterator(_) => {
            VariableValue::String("<iterator>".to_string())
        }
        Value::Module { name, .. } => {
            VariableValue::String(format!("<module '{}'>", name))
        }
    }
}

/// Converts a ValueKey to a VariableValue.
fn value_key_to_variable(key: &roast_runtime::value::ValueKey) -> VariableValue {
    use roast_runtime::value::ValueKey;
    match key {
        ValueKey::None => VariableValue::None,
        ValueKey::Bool(b) => VariableValue::Bool(*b),
        ValueKey::Int(i) => VariableValue::Int(*i),
        ValueKey::Str(s) => VariableValue::String(s.to_string()),
        ValueKey::Bytes(b) => VariableValue::Bytes(b.to_vec()),
        ValueKey::Tuple(items) => {
            let converted: Vec<VariableValue> = items.iter()
                .map(|v| value_key_to_variable(v))
                .collect();
            VariableValue::List(converted)
        }
    }
}

/// Extracts local variables from a VM call frame.
pub fn extract_locals(frame: &CallFrame) -> HashMap<String, VariableValue> {
    let mut locals = HashMap::new();

    // Get variable names from bytecode names pool
    for (i, value) in frame.locals.iter().enumerate() {
        // Try to get the variable name from bytecode names pool
        // The names pool contains all identifiers including local variable names
        let name = frame.code.names.get(i)
            .cloned()
            .unwrap_or_else(|| format!("local_{}", i));

        // Skip internal/unnamed variables that are None
        if !name.starts_with("local_") || !matches!(value, Value::None) {
            locals.insert(name, value_to_variable(value));
        }
    }

    locals
}

/// Extracts the call stack from the VM.
pub fn extract_stack(vm: &VM) -> Vec<Frame> {
    let mut frames = Vec::new();

    // VM doesn't expose frames directly, but we have current_frame
    // For now, we'll create a single frame from current_frame
    if let Some(frame) = vm.current_frame() {
        let source = if frame.code.filename.is_empty() {
            None
        } else {
            Some(PathBuf::from(&frame.code.filename))
        };

        let line = frame.code.instructions
            .get(frame.ip)
            .map(|i| i.line as i64)
            .unwrap_or(1);

        frames.push(Frame {
            id: 1,
            name: frame.code.name.clone(),
            source,
            line,
            column: 1,
            end_line: None,
            end_column: None,
            locals_ref: 1,
            module: None,
        });
    }

    frames
}

/// Builds an evaluation context from VM state.
pub fn build_eval_context(vm: &VM, frame_id: i64) -> EvaluationContext {
    let mut context = EvaluationContext::new(frame_id);

    if let Some(frame) = vm.current_frame() {
        // Add locals - use names pool for variable names
        for (i, value) in frame.locals.iter().enumerate() {
            let name = frame.code.names.get(i)
                .cloned()
                .unwrap_or_else(|| format!("local_{}", i));

            context.add_local(name, value_to_variable(value));
        }
    }

    // Note: VM globals access would need to be added via a method
    // For now, just locals are available

    context
}

/// VM debugger state that tracks additional debug info.
pub struct VMDebugState {
    /// Current step mode.
    pub step_mode: StepMode,
    /// Target step depth (for step-over/step-out).
    pub step_depth: usize,
    /// Current call depth.
    pub current_depth: usize,
}

impl VMDebugState {
    pub fn new() -> Self {
        Self {
            step_mode: StepMode::None,
            step_depth: 0,
            current_depth: 0,
        }
    }

    /// Start stepping into.
    pub fn step_in(&mut self) {
        self.step_mode = StepMode::StepIn;
    }

    /// Start stepping over (stay at same depth).
    pub fn step_over(&mut self, depth: usize) {
        self.step_mode = StepMode::StepOver;
        self.step_depth = depth;
    }

    /// Start stepping out (go to lower depth).
    pub fn step_out(&mut self, depth: usize) {
        self.step_mode = StepMode::StepOut;
        self.step_depth = depth.saturating_sub(1);
    }

    /// Check if we should stop at current location.
    pub fn should_stop(&self) -> bool {
        match self.step_mode {
            StepMode::None => false,
            StepMode::StepIn => true, // Stop at next instruction
            StepMode::StepOver => self.current_depth <= self.step_depth,
            StepMode::StepOut => self.current_depth <= self.step_depth, // Step out stops when we've exited
        }
    }

    /// Called when entering a function.
    pub fn on_function_entry(&mut self) {
        self.current_depth += 1;
    }

    /// Called when exiting a function.
    pub fn on_function_exit(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// Reset step mode after stopping.
    pub fn reset(&mut self) {
        self.step_mode = StepMode::None;
    }
}

impl Default for VMDebugState {
    fn default() -> Self {
        Self::new()
    }
}

/// Step mode for the debugger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Not stepping.
    None,
    /// Step into (stop at next instruction).
    StepIn,
    /// Step over (stop when depth <= target).
    StepOver,
    /// Step out (stop when depth < target).
    StepOut,
}

/// Line mapping for source code to bytecode instructions.
pub struct LineMap {
    /// Maps source line numbers to instruction indices.
    line_to_instructions: HashMap<i64, Vec<usize>>,
    /// Maps instruction indices to source line numbers.
    instruction_to_line: HashMap<usize, i64>,
}

impl LineMap {
    /// Creates a new empty line map.
    pub fn new() -> Self {
        Self {
            line_to_instructions: HashMap::new(),
            instruction_to_line: HashMap::new(),
        }
    }

    /// Builds a line map from bytecode.
    pub fn from_bytecode(code: &roast_codegen::Bytecode) -> Self {
        let mut map = Self::new();

        for (idx, instr) in code.instructions.iter().enumerate() {
            let line = instr.line as i64;

            map.instruction_to_line.insert(idx, line);
            map.line_to_instructions
                .entry(line)
                .or_insert_with(Vec::new)
                .push(idx);
        }

        map
    }

    /// Gets the first instruction index for a source line.
    pub fn line_to_first_instruction(&self, line: i64) -> Option<usize> {
        self.line_to_instructions.get(&line).and_then(|v| v.first().copied())
    }

    /// Gets the source line for an instruction index.
    pub fn instruction_to_line(&self, idx: usize) -> Option<i64> {
        self.instruction_to_line.get(&idx).copied()
    }

    /// Gets all lines that have instructions.
    pub fn all_lines(&self) -> Vec<i64> {
        let mut lines: Vec<_> = self.line_to_instructions.keys().copied().collect();
        lines.sort();
        lines
    }
}

impl Default for LineMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to validate breakpoint locations.
pub fn validate_breakpoint_line(code: &roast_codegen::Bytecode, line: i64) -> Option<i64> {
    // Check if this exact line has code
    for instr in &code.instructions {
        if instr.line as i64 == line {
            return Some(line);
        }
    }

    // Find the nearest line with code after the requested line
    let mut nearest: Option<i64> = None;
    for instr in &code.instructions {
        let instr_line = instr.line as i64;
        if instr_line >= line {
            match nearest {
                None => nearest = Some(instr_line),
                Some(n) if instr_line < n => nearest = Some(instr_line),
                _ => {}
            }
        }
    }

    nearest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_value_conversion() {
        assert!(matches!(value_to_variable(&Value::None), VariableValue::None));
        assert!(matches!(value_to_variable(&Value::Bool(true)), VariableValue::Bool(true)));
        assert!(matches!(value_to_variable(&Value::Int(42)), VariableValue::Int(42)));

        if let VariableValue::Float(f) = value_to_variable(&Value::Float(3.14)) {
            assert!((f - 3.14).abs() < 0.001);
        } else {
            panic!("Expected Float");
        }

        if let VariableValue::String(s) = value_to_variable(&Value::Str("hello".into())) {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_list_conversion() {
        let list = Value::List(Arc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)]));
        if let VariableValue::List(items) = value_to_variable(&list) {
            assert_eq!(items.len(), 3);
            assert!(matches!(&items[0], VariableValue::Int(1)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_step_mode() {
        let mut state = VMDebugState::new();

        // Step in should always stop
        state.step_in();
        assert!(state.should_stop());

        // Step over at depth 2
        state.step_over(2);
        state.current_depth = 2;
        assert!(state.should_stop()); // At target depth

        state.current_depth = 3;
        assert!(!state.should_stop()); // Too deep

        state.current_depth = 1;
        assert!(state.should_stop()); // Above target

        // Step out from depth 2
        state.step_out(2);
        state.current_depth = 2;
        assert!(!state.should_stop()); // Still at original depth

        state.current_depth = 1;
        assert!(state.should_stop()); // Exited to lower depth
    }
}
