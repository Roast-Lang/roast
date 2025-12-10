//! The main bytecode interpreter.

use crate::frame::{CallFrame, ExceptionHandler, FrameState};
use crate::heap::{Heap, HeapId};
use crate::stack::{StackError, ValueStack};
use crate::async_rt::AsyncRuntime;
use roast_codegen::{Bytecode, Constant, Instruction, OpCode, Operand};
use roast_runtime::Value;
use roast_runtime::value::{ValueKey, Coroutine, CoroutineState, RoastFunction};
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::path::Path;
use thiserror::Error;
use crate::debug::DebugHook;

/// Trait for coverage collection.
pub trait CoverageHook: Send + Sync {
    /// Called when a line is executed.
    fn hit_line(&self, file: &Path, line: u32);
    /// Called when a branch is taken.
    fn hit_branch(&self, file: &Path, branch_id: u32, taken: bool);
    /// Called when a function is entered.
    fn hit_function(&self, file: &Path, name: &str);
}

/// VM configuration.
#[derive(Clone, Debug)]
pub struct VMConfig {
    /// Maximum stack size.
    pub max_stack_size: usize,
    /// Maximum call depth.
    pub max_call_depth: usize,
    /// Enable debug tracing.
    pub debug: bool,
    /// GC threshold in bytes.
    pub gc_threshold: usize,
    /// Enable coverage collection.
    pub coverage: bool,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            max_stack_size: 65536,
            max_call_depth: 1000,
            debug: false,
            gc_threshold: 1024 * 1024,
            coverage: false,
        }
    }
}

/// VM error type.
#[derive(Error, Debug)]
pub enum VMError {
    #[error("Stack error: {0}")]
    Stack(#[from] StackError),
    #[error("Type error: {0}")]
    TypeError(String),
    #[error("Name error: {0}")]
    NameError(String),
    #[error("Index error: {0}")]
    IndexError(String),
    #[error("Key error: {0}")]
    KeyError(String),
    #[error("Attribute error: {0}")]
    AttributeError(String),
    #[error("Value error: {0}")]
    ValueError(String),
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("Division by zero")]
    DivisionByZero,
    #[error("Maximum recursion depth exceeded")]
    RecursionLimit,
    #[error("Assertion failed: {0}")]
    AssertionError(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    #[error("Import error: {0}")]
    ImportError(String),
    #[error("Halt")]
    Halt,
}

/// VM result type.
pub type VMResult<T> = Result<T, VMError>;

/// The Roast Virtual Machine.
pub struct VM {
    /// Configuration.
    config: VMConfig,
    /// The operand stack.
    stack: ValueStack,
    /// Call frames.
    frames: Vec<CallFrame>,
    /// Global variables.
    globals: FxHashMap<String, Value>,
    /// Built-in functions.
    builtins: FxHashMap<String, BuiltinFn>,
    /// The heap.
    heap: Heap,
    /// Loaded modules.
    modules: FxHashMap<String, FxHashMap<String, Value>>,
    /// Debug hook.
    debug_hook: Option<Arc<dyn DebugHook>>,
    /// Coverage hook.
    coverage_hook: Option<Arc<dyn CoverageHook>>,
    /// JIT manager for adaptive compilation.
    jit_manager: crate::jit_integration::JitManager,
    /// Async runtime for coroutine scheduling.
    async_runtime: AsyncRuntime,
    /// Pending coroutines queue.
    pending_coroutines: VecDeque<Arc<Coroutine>>,
}

/// Built-in function type.
pub type BuiltinFn = fn(&mut VM, Vec<Value>) -> VMResult<Value>;

impl VM {
    /// Creates a new VM with default configuration.
    pub fn new() -> Self {
        Self::with_config(VMConfig::default())
    }

    /// Creates a new VM with custom configuration.
    pub fn with_config(config: VMConfig) -> Self {
        let mut vm = Self {
            stack: ValueStack::new(config.max_stack_size),
            frames: Vec::with_capacity(config.max_call_depth),
            globals: FxHashMap::default(),
            builtins: FxHashMap::default(),
            heap: Heap::with_threshold(config.gc_threshold),
            modules: FxHashMap::default(),
            config,
            debug_hook: None,
            coverage_hook: None,
            jit_manager: crate::jit_integration::JitManager::default(),
            async_runtime: AsyncRuntime::new(),
            pending_coroutines: VecDeque::new(),
        };
        vm.register_builtins();
        vm
    }

    /// Registers built-in functions.
    fn register_builtins(&mut self) {
        self.builtins.insert("print".into(), builtin_print);
        self.builtins.insert("len".into(), builtin_len);
        self.builtins.insert("type".into(), builtin_type);
        self.builtins.insert("int".into(), builtin_int);
        self.builtins.insert("float".into(), builtin_float);
        self.builtins.insert("str".into(), builtin_str);
        self.builtins.insert("bool".into(), builtin_bool);
        self.builtins.insert("list".into(), builtin_list);
        self.builtins.insert("dict".into(), builtin_dict);
        self.builtins.insert("set".into(), builtin_set);
        self.builtins.insert("tuple".into(), builtin_tuple);
        self.builtins.insert("range".into(), builtin_range);
        self.builtins.insert("abs".into(), builtin_abs);
        self.builtins.insert("min".into(), builtin_min);
        self.builtins.insert("max".into(), builtin_max);
        self.builtins.insert("sum".into(), builtin_sum);
        self.builtins.insert("sorted".into(), builtin_sorted);
        self.builtins.insert("reversed".into(), builtin_reversed);
        self.builtins.insert("enumerate".into(), builtin_enumerate);
        self.builtins.insert("zip".into(), builtin_zip);
        self.builtins.insert("map".into(), builtin_map);
        self.builtins.insert("filter".into(), builtin_filter);
        self.builtins.insert("input".into(), builtin_input);
        self.builtins.insert("ord".into(), builtin_ord);
        self.builtins.insert("chr".into(), builtin_chr);
        self.builtins.insert("repr".into(), builtin_repr);
        self.builtins.insert("hash".into(), builtin_hash);
        self.builtins.insert("id".into(), builtin_id);
        self.builtins.insert("isinstance".into(), builtin_isinstance);
        self.builtins.insert("hasattr".into(), builtin_hasattr);
        self.builtins.insert("getattr".into(), builtin_getattr);
        self.builtins.insert("setattr".into(), builtin_setattr);
    }

    /// Sets the debug hook.
    pub fn set_debug_hook(&mut self, hook: Option<Arc<dyn DebugHook>>) {
        self.debug_hook = hook;
    }

    /// Sets the coverage hook.
    pub fn set_coverage_hook(&mut self, hook: Option<Arc<dyn CoverageHook>>) {
        self.coverage_hook = hook;
    }

    /// Checks if coverage is enabled.
    pub fn is_coverage_enabled(&self) -> bool {
        self.config.coverage && self.coverage_hook.is_some()
    }

    /// Gets JIT statistics.
    pub fn jit_stats(&self) -> crate::jit_integration::JitStats {
        self.jit_manager.stats()
    }

    /// Configures JIT settings.
    pub fn configure_jit(&mut self, config: crate::jit_integration::VMJitConfig) {
        self.jit_manager = crate::jit_integration::JitManager::new(config);
    }

    /// Gets the current JIT tier for a function.
    pub fn get_jit_tier(&self, func_id: u32) -> crate::jit_integration::JitTier {
        self.jit_manager.get_tier(func_id)
    }

    /// Returns the current call frame.
    pub fn current_frame(&self) -> Option<&CallFrame> {
        self.frames.last()
    }

    /// Calls a function by name.
    pub fn call_function_by_name(&mut self, name: &str, args: Vec<Value>) -> VMResult<Value> {
        // Find function in global scope
        let func = if let Some(frame) = self.frames.last() {
             // Look in current frame's globals (not implemented access yet)
             // Fallback to builtins or modules
             if let Some(module) = self.modules.get("__main__") {
                module.get(name).cloned()
             } else {
                None
             }
        } else {
            // Look in globals first
            if let Some(val) = self.globals.get(name) {
                Some(val.clone())
            } else if let Some(module) = self.modules.get("__main__") {
                module.get(name).cloned()
            } else {
                None
            }
        };

        let func = func.ok_or_else(|| VMError::NameError(name.to_string()))?;

        match func {
            Value::Function(f) => {
                let code = f.code.clone();
                let mut frame = CallFrame::new(code, 0);

                // Set arguments
                if args.len() != f.arity {
                    return Err(VMError::TypeError(format!(
                        "{}() takes {} arguments but {} were given",
                        name, f.arity, args.len()
                    )));
                }

                for (i, arg) in args.into_iter().enumerate() {
                    frame.set_local(i, arg);
                }

                self.frames.push(frame);
                self.run()
            }
            _ => self.call_builtin(func, args),
        }
    }

    /// Executes bytecode and returns the result.
    pub fn execute(&mut self, code: Arc<Bytecode>) -> VMResult<Value> {
        // Register with JIT manager and check for tier-up
        let func_id = self.jit_manager.get_func_id(&code);
        let action = self.jit_manager.on_function_entry(func_id, &code);

        // Handle tier-up actions
        #[cfg(feature = "jit")]
        match action {
            crate::jit_integration::TierUpAction::CompileBaseline => {
                if let Err(e) = self.jit_manager.compile_baseline(func_id, &code) {
                    if self.config.debug {
                        eprintln!("JIT baseline compilation failed: {}", e);
                    }
                }
            }
            crate::jit_integration::TierUpAction::CompileOptimizing => {
                if let Err(e) = self.jit_manager.compile_optimizing(func_id, &code) {
                    if self.config.debug {
                        eprintln!("JIT optimizing compilation failed: {}", e);
                    }
                }
            }
            _ => {}
        }

        #[cfg(not(feature = "jit"))]
        let _ = action;

        // Create initial frame
        let frame = CallFrame::new(code, 0);
        self.frames.push(frame);

        // Run the interpreter loop
        self.run()
    }

    /// Main interpreter loop.
    fn run(&mut self) -> VMResult<Value> {
        loop {
            // Check recursion limit
            if self.frames.len() > self.config.max_call_depth {
                return Err(VMError::RecursionLimit);
            }

            // Get current frame
            let frame_idx = self.frames.len() - 1;

            // Check if we're done
            if self.frames[frame_idx].ip >= self.frames[frame_idx].code.instructions.len() {
                // Implicit return None
                if self.frames.len() == 1 {
                    return Ok(Value::None);
                }
                self.frames.pop();
                self.stack.push(Value::None)?;
                continue;
            }

            // Fetch instruction
            let instruction = self.frames[frame_idx].code.instructions[self.frames[frame_idx].ip].clone();

            // Call debug hook
            if let Some(hook) = &self.debug_hook {
                hook.on_step(self)?;
            }

            if self.config.debug {
                eprintln!("[{:04}] {:?}", self.frames[frame_idx].ip, instruction.opcode);
            }

            // Advance IP before execution
            self.frames[frame_idx].ip += 1;

            // Execute
            match self.execute_instruction(&instruction)? {
                ExecResult::Continue => {}
                ExecResult::Return(value) => {
                    if self.frames.len() == 1 {
                        return Ok(value);
                    }
                    self.frames.pop();
                    self.stack.push(value)?;
                }
                ExecResult::Yield(value) => {
                    // For generators
                    return Ok(value);
                }
                ExecResult::Await(awaitable) => {
                    // In the main run loop, await blocks and executes the coroutine
                    match awaitable {
                        Value::Coroutine(coro) => {
                            let result = self.run_coroutine(coro)?;
                            self.stack.push(result)?;
                        }
                        other => {
                            // Non-coroutine awaitable is ready immediately
                            self.stack.push(other)?;
                        }
                    }
                }
                ExecResult::Halt => {
                    return Err(VMError::Halt);
                }
            }
        }
    }

    /// Executes a single instruction.
    fn execute_instruction(&mut self, instr: &Instruction) -> VMResult<ExecResult> {
        match instr.opcode {
            // Stack operations
            OpCode::Nop => {}
            OpCode::Pop => { self.stack.pop()?; }
            OpCode::Dup => { self.stack.dup()?; }
            OpCode::Swap => { self.stack.swap()?; }
            OpCode::Rot3 => { self.stack.rot3()?; }

            // Constants
            OpCode::LoadNone => { self.stack.push(Value::None)?; }
            OpCode::LoadTrue => { self.stack.push(Value::Bool(true))?; }
            OpCode::LoadFalse => { self.stack.push(Value::Bool(false))?; }
            OpCode::LoadInt => {
                if let Operand::I64(n) = instr.operand {
                    self.stack.push(Value::Int(n))?;
                }
            }
            OpCode::LoadFloat => {
                if let Operand::F64(f) = instr.operand {
                    self.stack.push(Value::Float(f))?;
                }
            }
            OpCode::LoadConst => {
                if let Operand::U16(idx) = instr.operand {
                    let value = self.load_constant(idx as usize)?;
                    self.stack.push(value)?;
                }
            }

            // Locals
            OpCode::LoadFast | OpCode::LoadLocal => {
                let slot = match instr.operand {
                    Operand::U8(s) => s as usize,
                    Operand::U16(s) => s as usize,
                    _ => 0,
                };
                let frame = self.frames.last().unwrap();
                let value = frame.get_local(slot).cloned().unwrap_or(Value::None);
                eprintln!("LoadFast slot={} value={:?} num_locals={}", slot, value, frame.locals.len());
                self.stack.push(value)?;
            }
            OpCode::StoreFast | OpCode::StoreLocal => {
                let slot = match instr.operand {
                    Operand::U8(s) => s as usize,
                    Operand::U16(s) => s as usize,
                    _ => 0,
                };
                let value = self.stack.pop()?;
                let frame = self.frames.last_mut().unwrap();
                eprintln!("StoreFast slot={} value={:?} num_locals={}", slot, value, frame.locals.len());
                frame.set_local(slot, value);
            }

            // Globals
            OpCode::LoadGlobal | OpCode::LoadName => {
                if let Operand::U16(idx) = instr.operand {
                    let name = self.get_name(idx as usize)?;
                    let value = self.globals.get(&name)
                        .cloned()
                        .or_else(|| self.builtins.get(&name).map(|_| Value::Str(name.clone().into())))
                        .ok_or_else(|| VMError::NameError(format!("name '{}' is not defined", name)))?;
                    self.stack.push(value)?;
                }
            }
            OpCode::StoreGlobal | OpCode::StoreName => {
                if let Operand::U16(idx) = instr.operand {
                    let name = self.get_name(idx as usize)?;
                    let value = self.stack.pop()?;
                    self.globals.insert(name, value);
                }
            }

            // Arithmetic
            OpCode::Add => self.binary_op(|a, b| {
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x.wrapping_add(y))),
                    (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
                    (Value::Int(x), Value::Float(y)) => Ok(Value::Float(x as f64 + y)),
                    (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x + y as f64)),
                    (Value::Str(x), Value::Str(y)) => Ok(Value::Str(format!("{}{}", x, y).into())),
                    (a, b) => {
                        eprintln!("Add error: {:?} + {:?}", a, b);
                        Err(VMError::TypeError("unsupported operand types for +".into()))
                    }
                }
            })?,
            OpCode::Sub => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x.wrapping_sub(y))),
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
                (Value::Int(x), Value::Float(y)) => Ok(Value::Float(x as f64 - y)),
                (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x - y as f64)),
                _ => Err(VMError::TypeError("unsupported operand types for -".into())),
            })?,
            OpCode::Mul => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x.wrapping_mul(y))),
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
                (Value::Int(x), Value::Float(y)) => Ok(Value::Float(x as f64 * y)),
                (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x * y as f64)),
                (Value::Str(s), Value::Int(n)) => Ok(Value::Str(s.repeat(n.max(0) as usize).into())),
                _ => Err(VMError::TypeError("unsupported operand types for *".into())),
            })?,
            OpCode::Div => self.binary_op(|a, b| {
                let (x, y) = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => (x as f64, y as f64),
                    (Value::Float(x), Value::Float(y)) => (x, y),
                    (Value::Int(x), Value::Float(y)) => (x as f64, y),
                    (Value::Float(x), Value::Int(y)) => (x, y as f64),
                    _ => return Err(VMError::TypeError("unsupported operand types for /".into())),
                };
                if y == 0.0 {
                    return Err(VMError::DivisionByZero);
                }
                Ok(Value::Float(x / y))
            })?,
            OpCode::FloorDiv => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => {
                    if y == 0 { return Err(VMError::DivisionByZero); }
                    Ok(Value::Int(x / y))
                }
                _ => Err(VMError::TypeError("unsupported operand types for //".into())),
            })?,
            OpCode::Mod => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => {
                    if y == 0 { return Err(VMError::DivisionByZero); }
                    Ok(Value::Int(x % y))
                }
                _ => Err(VMError::TypeError("unsupported operand types for %".into())),
            })?,
            OpCode::Pow => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x.pow(y as u32))),
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x.powf(y))),
                _ => Err(VMError::TypeError("unsupported operand types for **".into())),
            })?,
            OpCode::Neg => {
                let value = self.stack.pop()?;
                let result = match value {
                    Value::Int(n) => Value::Int(-n),
                    Value::Float(f) => Value::Float(-f),
                    _ => return Err(VMError::TypeError("bad operand type for unary -".into())),
                };
                self.stack.push(result)?;
            }

            // Bitwise
            OpCode::BitAnd => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x & y)),
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x & y)),
                _ => Err(VMError::TypeError("unsupported operand types for &".into())),
            })?,
            OpCode::BitOr => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x | y)),
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x | y)),
                _ => Err(VMError::TypeError("unsupported operand types for |".into())),
            })?,
            OpCode::BitXor => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x ^ y)),
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x ^ y)),
                _ => Err(VMError::TypeError("unsupported operand types for ^".into())),
            })?,
            OpCode::BitNot => {
                let value = self.stack.pop()?;
                let result = match value {
                    Value::Int(n) => Value::Int(!n),
                    _ => return Err(VMError::TypeError("bad operand type for unary ~".into())),
                };
                self.stack.push(result)?;
            }
            OpCode::Shl => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x << y)),
                _ => Err(VMError::TypeError("unsupported operand types for <<".into())),
            })?,
            OpCode::Shr => self.binary_op(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x >> y)),
                _ => Err(VMError::TypeError("unsupported operand types for >>".into())),
            })?,

            // Comparison
            OpCode::Eq => self.binary_op(|a, b| {
                Ok(Value::Bool(a == b))
            })?,
            OpCode::Ne => self.binary_op(|a, b| Ok(Value::Bool(a != b)))?,
            OpCode::Lt => self.compare_op(|a, b| a < b)?,
            OpCode::Le => self.compare_op(|a, b| a <= b)?,
            OpCode::Gt => self.compare_op(|a, b| a > b)?,
            OpCode::Ge => self.compare_op(|a, b| a >= b)?,
            OpCode::Is => {
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                // Identity comparison (simplified)
                self.stack.push(Value::Bool(std::ptr::eq(&a as *const _, &b as *const _)))?;
            }
            OpCode::IsNot => {
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(Value::Bool(!std::ptr::eq(&a as *const _, &b as *const _)))?;
            }
            OpCode::In => {
                let container = self.stack.pop()?;
                let item = self.stack.pop()?;
                let result = self.contains(&container, &item)?;
                self.stack.push(Value::Bool(result))?;
            }
            OpCode::NotIn => {
                let container = self.stack.pop()?;
                let item = self.stack.pop()?;
                let result = !self.contains(&container, &item)?;
                self.stack.push(Value::Bool(result))?;
            }

            // Boolean
            OpCode::Not => {
                let value = self.stack.pop()?;
                self.stack.push(Value::Bool(!self.is_truthy(&value)))?;
            }
            OpCode::And | OpCode::Or => {
                // These are typically short-circuit, but simplified here
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                let result = if instr.opcode == OpCode::And {
                    if self.is_truthy(&a) { b } else { a }
                } else {
                    if self.is_truthy(&a) { a } else { b }
                };
                self.stack.push(result)?;
            }

            // Control flow
            OpCode::Jump => {
                if let Operand::I16(offset) = instr.operand {
                    let frame = self.frames.last_mut().unwrap();
                    frame.jump(offset);
                }
            }
            OpCode::JumpIfTrue => {
                if let Operand::I16(offset) = instr.operand {
                    let value = self.stack.pop()?;
                    if self.is_truthy(&value) {
                        let frame = self.frames.last_mut().unwrap();
                        frame.jump(offset);
                    }
                }
            }
            OpCode::JumpIfFalse => {
                if let Operand::I16(offset) = instr.operand {
                    let value = self.stack.pop()?;
                    if !self.is_truthy(&value) {
                        let frame = self.frames.last_mut().unwrap();
                        frame.jump(offset);
                    }
                }
            }
            OpCode::JumpIfTrueOrPop => {
                if let Operand::I16(offset) = instr.operand {
                    let value = self.stack.peek()?.clone();
                    if self.is_truthy(&value) {
                        let frame = self.frames.last_mut().unwrap();
                        frame.jump(offset);
                    } else {
                        self.stack.pop()?;
                    }
                }
            }
            OpCode::JumpIfFalseOrPop => {
                if let Operand::I16(offset) = instr.operand {
                    let value = self.stack.peek()?.clone();
                    if !self.is_truthy(&value) {
                        let frame = self.frames.last_mut().unwrap();
                        frame.jump(offset);
                    } else {
                        self.stack.pop()?;
                    }
                }
            }

            // Function calls
            OpCode::Call => {
                if let Operand::U8(argc) = instr.operand {
                    let args = self.stack.pop_n(argc as usize)?;
                    let func = self.stack.pop()?;

                    match func {
                        Value::Function(f) => {
                            if args.len() != f.arity {
                                return Err(VMError::TypeError(format!(
                                    "{}() takes {} arguments but {} were given",
                                    f.name, f.arity, args.len()
                                )));
                            }

                            // Check if this is an async function
                            if f.is_async {
                                // Create a coroutine object instead of executing directly
                                let coroutine = Arc::new(Coroutine::new(f, args));
                                self.stack.push(Value::Coroutine(coroutine))?;
                            } else {
                                let code = f.code.clone();
                                let mut frame = CallFrame::new(code, 0);

                                for (i, arg) in args.into_iter().enumerate() {
                                    frame.set_local(i, arg);
                                }

                                self.frames.push(frame);
                                // Do NOT push result, return Continue to execute new frame
                            }
                        }
                        Value::Coroutine(coro) => {
                            // Calling a coroutine resumes it (like .send() in Python)
                            // For now, just push it back - await will handle execution
                            self.stack.push(Value::Coroutine(coro))?;
                        }
                        _ => {
                            let result = self.call_builtin(func, args)?;
                            self.stack.push(result)?;
                        }
                    }
                }
            }
            OpCode::Return => {
                let value = if self.stack.is_empty() {
                    Value::None
                } else {
                    self.stack.pop()?
                };
                return Ok(ExecResult::Return(value));
            }
            OpCode::Yield => {
                let value = self.stack.pop()?;
                return Ok(ExecResult::Yield(value));
            }

            // Async operations
            OpCode::Await => {
                // Pop the awaitable from the stack
                let awaitable = self.stack.pop()?;

                // Check if it's a coroutine or has __await__
                match &awaitable {
                    Value::Coroutine(coro) => {
                        // Check if coroutine is already done
                        if coro.is_done() {
                            // Get the result and push it
                            let result = coro.result.lock().unwrap();
                            if let Some(val) = result.as_ref() {
                                self.stack.push(val.clone())?;
                            } else {
                                // Check for error
                                let error = coro.error.lock().unwrap();
                                if let Some(err) = error.as_ref() {
                                    return Err(VMError::RuntimeError(err.clone()));
                                }
                                self.stack.push(Value::None)?;
                            }
                        } else {
                            // Execute the coroutine
                            let result = self.run_coroutine(coro.clone())?;
                            self.stack.push(result)?;
                        }
                    }
                    Value::Generator(gen) => {
                        // For generators, get the next value
                        let mut gen_state = gen.lock().unwrap();
                        if gen_state.status == roast_runtime::GeneratorStatus::Exhausted {
                            return Err(VMError::RuntimeError("cannot await exhausted generator".into()));
                        }
                        // Would need to step the generator - for now return None
                        self.stack.push(Value::None)?;
                    }
                    _ => {
                        // Try to get __await__ method
                        // For now, treat non-coroutines as immediately ready
                        self.stack.push(awaitable)?;
                    }
                }
            }
            OpCode::GetAwaitable => {
                // Convert value to awaitable (calls __await__ if needed)
                let value = self.stack.pop()?;
                match &value {
                    Value::Coroutine(_) => {
                        // Already awaitable
                        self.stack.push(value)?;
                    }
                    Value::Generator(_) => {
                        // Generators are awaitable
                        self.stack.push(value)?;
                    }
                    _ => {
                        // Try to call __await__ method
                        // For now, just wrap in a ready coroutine
                        self.stack.push(value)?;
                    }
                }
            }
            OpCode::SetupAsyncWith | OpCode::EndAsyncFor => {
                // TODO: Implement async with and async for
                return Err(VMError::NotImplemented("async with/for not yet implemented".into()));
            }

            // Object creation
            OpCode::BuildList => {
                if let Operand::U16(count) = instr.operand {
                    let items = self.stack.pop_n(count as usize)?;
                    self.stack.push(Value::List(Arc::new(Mutex::new(items))))?;
                }
            }
            OpCode::BuildTuple => {
                if let Operand::U16(count) = instr.operand {
                    let items = self.stack.pop_n(count as usize)?;
                    self.stack.push(Value::Tuple(Arc::from(items.into_boxed_slice())))?;
                }
            }
            OpCode::BuildDict => {
                if let Operand::U16(count) = instr.operand {
                    let items = self.stack.pop_n((count * 2) as usize)?;
                    let mut map = HashMap::new();
                    for chunk in items.chunks(2) {
                        if let [key, value] = chunk {
                            if let Some(k) = self.value_to_key(key) {
                                map.insert(k, value.clone());
                            }
                        }
                    }
                    self.stack.push(Value::Dict(Arc::new(Mutex::new(map))))?;
                }
            }
            OpCode::BuildSet => {
                if let Operand::U16(count) = instr.operand {
                    let items = self.stack.pop_n(count as usize)?;
                    let set: HashSet<_> = items.into_iter()
                        .filter_map(|v| self.value_to_key(&v))
                        .collect();
                    self.stack.push(Value::Set(Arc::new(Mutex::new(set))))?;
                }
            }

            // Subscript
            OpCode::LoadSubscr => {
                let key = self.stack.pop()?;
                let obj = self.stack.pop()?;
                let value = self.get_item(&obj, &key)?;
                self.stack.push(value)?;
            }
            OpCode::StoreSubscr => {
                let key = self.stack.pop()?;
                let value = self.stack.pop()?;
                let obj = self.stack.pop()?;
                self.set_item(obj, key, value)?;
            }

            // Attributes
            OpCode::LoadAttr => {
                if let Operand::U16(idx) = instr.operand {
                    let name = self.get_name(idx as usize)?;
                    let obj = self.stack.pop()?;
                    let value = self.get_attr(&obj, &name)?;
                    self.stack.push(value)?;
                }
            }
            OpCode::StoreAttr => {
                if let Operand::U16(idx) = instr.operand {
                    let name = self.get_name(idx as usize)?;
                    let value = self.stack.pop()?;
                    let obj = self.stack.pop()?;
                    self.set_attr(obj, &name, value)?;
                }
            }

            // Iteration
            OpCode::GetIter => {
                let value = self.stack.pop()?;
                let iter = self.make_iterator(value)?;
                self.stack.push(iter)?;
            }
            OpCode::ForIter => {
                if let Operand::I16(offset) = instr.operand {
                    // Pop the iterator, get next item, push it back
                    let mut iter = self.stack.pop()?;
                    let next = self.next_item(&mut iter);
                    match next {
                        Some(item) => {
                            self.stack.push(iter)?; // Push iterator back
                            self.stack.push(item)?;
                        }
                        None => {
                            // Don't push exhausted iterator back
                            let frame = self.frames.last_mut().unwrap();
                            frame.jump(offset);
                        }
                    }
                }
            }

            // Unpacking
            OpCode::UnpackSequence => {
                if let Operand::U8(count) = instr.operand {
                    let value = self.stack.pop()?;
                    let items = self.unpack(&value, count as usize)?;
                    for item in items.into_iter().rev() {
                        self.stack.push(item)?;
                    }
                }
            }

            // Exception handling
            OpCode::SetupTry => {
                if let Operand::I16(offset) = instr.operand {
                    let frame = self.frames.last_mut().unwrap();
                    let target = (frame.ip as isize + offset as isize) as usize;
                    frame.push_handler(ExceptionHandler::new(target, self.stack.len()));
                }
            }
            OpCode::PopExcept => {
                let frame = self.frames.last_mut().unwrap();
                frame.pop_handler();
            }
            OpCode::Raise => {
                let value = self.stack.pop()?;
                return Err(VMError::RuntimeError(format!("{:?}", value)));
            }

            // Ownership operations
            OpCode::Ref | OpCode::RefMut => {
                // In the VM, references are the same as values (no borrow checking at runtime)
                // The value is already on the stack
            }
            OpCode::Deref => {
                // Dereference is a no-op in the VM
            }
            OpCode::Move => {
                // Move is the same as copy in the VM
            }
            OpCode::Copy => {
                let value = self.stack.peek()?.clone();
                self.stack.push(value)?;
            }
            OpCode::Drop => {
                self.stack.pop()?;
            }

            // Pattern matching
            OpCode::MatchSequence => {
                // Match against a sequence pattern
                // Stack: subject
                // Operand: expected length
                if let Operand::U8(expected_len) = instr.operand {
                    let subject = self.stack.peek()?;
                    let matches = match subject {
                        Value::List(l) => l.lock().unwrap().len() == expected_len as usize,
                        Value::Tuple(t) => t.len() == expected_len as usize,
                        _ => false,
                    };
                    self.stack.push(Value::Bool(matches))?;
                }
            }

            OpCode::MatchMapping => {
                // Match against a mapping pattern
                // Stack: subject
                // Operand: number of keys to check
                if let Operand::U8(key_count) = instr.operand {
                    let subject = self.stack.peek()?;
                    let matches = match subject {
                        Value::Dict(d) => d.lock().unwrap().len() >= key_count as usize,
                        _ => false,
                    };
                    self.stack.push(Value::Bool(matches))?;
                }
            }

            OpCode::MatchClass => {
                // Match against a class pattern
                // Stack: subject, class
                // Operand: number of positional patterns
                if let Operand::U8(_attr_count) = instr.operand {
                    let _class = self.stack.pop()?;
                    let subject = self.stack.peek()?;
                    // For now, just check if it's an object
                    let matches = matches!(subject, Value::Object { .. });
                    self.stack.push(Value::Bool(matches))?;
                }
            }

            OpCode::MatchAs => {
                // Bind matched value to a name
                // Stack: subject
                // Operand: name index
                if let Operand::U8(name_idx) = instr.operand {
                    let value = self.stack.peek()?.clone();
                    let frame = self.frames.last_mut().unwrap();
                    frame.set_local(name_idx as usize, value);
                    self.stack.push(Value::Bool(true))?;
                }
            }

            OpCode::MatchOr => {
                // Or pattern - check if any alternative matches
                // Stack: match_result1, match_result2, ...
                // Operand: number of alternatives
                if let Operand::U8(count) = instr.operand {
                    let results = self.stack.pop_n(count as usize)?;
                    let any_match = results.iter().any(|v| self.is_truthy(v));
                    self.stack.push(Value::Bool(any_match))?;
                }
            }

            OpCode::MatchStar => {
                // Star pattern for sequences (*rest)
                // Stack: subject, start_index
                // Operand: number of remaining items after star
                if let Operand::U8(remaining) = instr.operand {
                    let start = match self.stack.pop()? {
                        Value::Int(n) => n as usize,
                        _ => 0,
                    };
                    let subject = self.stack.pop()?;

                    let matched = match &subject {
                        Value::List(l) => {
                            let guard = l.lock().unwrap();
                            let end = if remaining as usize > 0 {
                                guard.len().saturating_sub(remaining as usize)
                            } else {
                                guard.len()
                            };
                            Value::List(Arc::new(Mutex::new(guard[start..end].to_vec())))
                        }
                        Value::Tuple(t) => {
                            let end = if remaining as usize > 0 {
                                t.len().saturating_sub(remaining as usize)
                            } else {
                                t.len()
                            };
                            Value::List(Arc::new(Mutex::new(t[start..end].to_vec())))
                        }
                        _ => Value::List(Arc::new(Mutex::new(vec![]))),
                    };
                    self.stack.push(matched)?;
                }
            }

            OpCode::MatchGuard => {
                // Evaluate a guard condition
                // Stack: guard_result, match_result
                // Result: combined result
                let guard = self.stack.pop()?;
                let match_result = self.stack.pop()?;
                let result = self.is_truthy(&match_result) && self.is_truthy(&guard);
                self.stack.push(Value::Bool(result))?;
            }

            OpCode::TryUnwrap => {
                // Try operator (? operator): unwrap Result/Option or return early
                // Stack: result_value
                // If Ok/Some: push unwrapped value
                // If Err/None: jump to error handler offset
                let offset = match instr.operand {
                    Operand::I16(o) => o,
                    _ => 0,
                };

                let value = self.stack.pop()?;

                // Check if value is Ok/Some or Err/None
                // For now, we treat tuples as Result: ("Ok", value) or ("Err", error)
                // And Option as: ("Some", value) or None
                match &value {
                    // Result.Ok case: unwrap and continue
                    Value::Tuple(items) if items.len() >= 2 => {
                        if let Value::Str(tag) = &items[0] {
                            if tag.as_ref() == "Ok" || tag.as_ref() == "Some" {
                                // Push the unwrapped value
                                self.stack.push(items[1].clone())?;
                            } else if tag.as_ref() == "Err" {
                                // Push the error value for early return
                                self.stack.push(value)?;
                                // Jump to error handler
                                let frame = self.frames.last_mut().unwrap();
                                frame.ip = (frame.ip as i64 + offset as i64) as usize;
                            } else {
                                // Unknown tag, treat as value
                                self.stack.push(value)?;
                            }
                        } else {
                            // Not a tagged tuple, push back
                            self.stack.push(value)?;
                        }
                    }
                    // None case: return None early
                    Value::None => {
                        self.stack.push(Value::None)?;
                        // Jump to error handler
                        let frame = self.frames.last_mut().unwrap();
                        frame.ip = (frame.ip as i64 + offset as i64) as usize;
                    }
                    // Any other value: just pass through (treat as already unwrapped)
                    _ => {
                        self.stack.push(value)?;
                    }
                }
            }

            OpCode::Halt => {
                return Ok(ExecResult::Halt);
            }

            _ => {
                return Err(VMError::NotImplemented(format!("{:?}", instr.opcode)));
            }
        }

        Ok(ExecResult::Continue)
    }

    // Coroutine execution methods

    /// Runs a coroutine to completion (blocking).
    fn run_coroutine(&mut self, coro: Arc<Coroutine>) -> VMResult<Value> {
        // Mark as running
        {
            let mut state = coro.state.lock().unwrap();
            *state = CoroutineState::Running;
        }

        // Get the function code
        let code = coro.function.code.clone();

        // Create a new frame for the coroutine
        let mut frame = CallFrame::new(code, 0);

        // Restore locals from coroutine state
        {
            let locals = coro.locals.lock().unwrap();
            for (i, val) in locals.iter().enumerate() {
                frame.set_local(i, val.clone());
            }
        }

        // Restore instruction pointer
        {
            let ip = coro.ip.lock().unwrap();
            frame.ip = *ip;
        }

        // Push frame and execute
        self.frames.push(frame);

        // Restore stack if we were suspended
        {
            let saved_stack = coro.stack.lock().unwrap();
            for val in saved_stack.iter() {
                self.stack.push(val.clone())?;
            }
        }

        // Run until completion or suspension
        let result = self.run_coroutine_loop(coro.clone());

        // Handle result
        match result {
            Ok(value) => {
                // Mark as completed
                {
                    let mut state = coro.state.lock().unwrap();
                    *state = CoroutineState::Completed;
                }
                {
                    let mut result_slot = coro.result.lock().unwrap();
                    *result_slot = Some(value.clone());
                }
                Ok(value)
            }
            Err(e) => {
                // Mark as failed
                {
                    let mut state = coro.state.lock().unwrap();
                    *state = CoroutineState::Failed;
                }
                {
                    let mut error = coro.error.lock().unwrap();
                    *error = Some(e.to_string());
                }
                Err(e)
            }
        }
    }

    /// Coroutine interpreter loop - similar to run() but handles suspension.
    fn run_coroutine_loop(&mut self, coro: Arc<Coroutine>) -> VMResult<Value> {
        loop {
            // Check recursion limit
            if self.frames.len() > self.config.max_call_depth {
                return Err(VMError::RecursionLimit);
            }

            let frame_idx = self.frames.len() - 1;

            // Check if we're done
            if self.frames[frame_idx].ip >= self.frames[frame_idx].code.instructions.len() {
                // Implicit return None
                if self.frames.len() == 1 {
                    self.frames.pop();
                    return Ok(Value::None);
                }
                self.frames.pop();
                self.stack.push(Value::None)?;
                continue;
            }

            // Fetch instruction
            let instruction = self.frames[frame_idx].code.instructions[self.frames[frame_idx].ip].clone();

            if self.config.debug {
                eprintln!("[CORO {:04}] {:?}", self.frames[frame_idx].ip, instruction.opcode);
            }

            // Advance IP before execution
            self.frames[frame_idx].ip += 1;

            // Execute
            match self.execute_instruction(&instruction)? {
                ExecResult::Continue => {}
                ExecResult::Return(value) => {
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(value);
                    }
                    self.stack.push(value)?;
                }
                ExecResult::Yield(value) => {
                    // Suspend the coroutine
                    self.suspend_coroutine(&coro)?;
                    return Ok(value);
                }
                ExecResult::Await(awaitable) => {
                    // Nested await - recursively await the inner coroutine
                    match awaitable {
                        Value::Coroutine(inner_coro) => {
                            let result = self.run_coroutine(inner_coro)?;
                            self.stack.push(result)?;
                        }
                        other => {
                            // Non-coroutine awaitable, push directly
                            self.stack.push(other)?;
                        }
                    }
                }
                ExecResult::Halt => {
                    return Err(VMError::Halt);
                }
            }
        }
    }

    /// Suspends a coroutine, saving its state.
    fn suspend_coroutine(&mut self, coro: &Arc<Coroutine>) -> VMResult<()> {
        // Save instruction pointer
        if let Some(frame) = self.frames.last() {
            let mut ip = coro.ip.lock().unwrap();
            *ip = frame.ip;

            // Save locals
            let mut locals = coro.locals.lock().unwrap();
            *locals = frame.locals.to_vec();
        }

        // Save stack (what's on the stack for this coroutine)
        // For simplicity, we don't save the full stack - just mark state
        {
            let mut state = coro.state.lock().unwrap();
            *state = CoroutineState::Suspended;
        }

        // Pop the coroutine's frame
        self.frames.pop();

        Ok(())
    }

    /// Executes an async function and returns when complete.
    /// This is the main entry point for running async code.
    pub fn run_async(&mut self, code: Arc<Bytecode>) -> VMResult<Value> {
        // Create an async function wrapper
        let func = Arc::new(RoastFunction {
            name: code.name.clone(),
            arity: 0,
            code: code.clone(),
            is_async: true,
        });

        // Create the coroutine
        let coro = Arc::new(Coroutine::new(func, vec![]));

        // Run it
        self.run_coroutine(coro)
    }

    /// Spawns a coroutine for later execution.
    pub fn spawn_coroutine(&mut self, coro: Arc<Coroutine>) -> u64 {
        let task_id = self.async_runtime.spawn();
        self.pending_coroutines.push_back(coro);
        task_id
    }

    /// Runs all pending coroutines until completion.
    pub fn run_pending_coroutines(&mut self) -> VMResult<()> {
        while let Some(coro) = self.pending_coroutines.pop_front() {
            if !coro.is_done() {
                match self.run_coroutine(coro.clone()) {
                    Ok(_) => {}
                    Err(e) => {
                        // Mark coroutine as failed
                        let mut state = coro.state.lock().unwrap();
                        *state = CoroutineState::Failed;
                        let mut error = coro.error.lock().unwrap();
                        *error = Some(e.to_string());
                    }
                }
            }
        }
        Ok(())
    }

    // Helper methods

    fn load_constant(&self, idx: usize) -> VMResult<Value> {
        let frame = self.frames.last().unwrap();
        let constant = frame.code.constants.get(idx)
            .ok_or_else(|| VMError::RuntimeError("constant index out of bounds".into()))?;
        Ok(self.constant_to_value(constant))
    }

    fn constant_to_value(&self, constant: &Constant) -> Value {
        match constant {
            Constant::None => Value::None,
            Constant::Bool(b) => Value::Bool(*b),
            Constant::Int(n) => Value::Int(*n),
            Constant::BigInt(s) => Value::Int(s.parse().unwrap_or(0)),
            Constant::Float(f) => Value::Float(*f),
            Constant::Str(s) => Value::Str(s.clone().into()),
            Constant::Bytes(b) => Value::Bytes(b.clone().into()),
            Constant::Tuple(items) => {
                let values: Vec<_> = items.iter().map(|c| self.constant_to_value(c)).collect();
                Value::Tuple(Arc::from(values.into_boxed_slice()))
            }
            Constant::Code(code) => Value::None, // Functions are more complex
        }
    }

    fn get_name(&self, idx: usize) -> VMResult<String> {
        let frame = self.frames.last().unwrap();
        frame.code.names.get(idx)
            .cloned()
            .ok_or_else(|| VMError::RuntimeError("name index out of bounds".into()))
    }

    fn binary_op<F>(&mut self, op: F) -> VMResult<()>
    where
        F: FnOnce(Value, Value) -> VMResult<Value>,
    {
        let b = self.stack.pop()?;
        let a = self.stack.pop()?;
        let result = op(a, b)?;
        self.stack.push(result)?;
        Ok(())
    }

    fn compare_op<F>(&mut self, op: F) -> VMResult<()>
    where
        F: Fn(i64, i64) -> bool,
    {
        let b = self.stack.pop()?;
        let a = self.stack.pop()?;
        let result = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => op(*x, *y),
            (Value::Float(x), Value::Float(y)) => {
                op((*x * 1000000.0) as i64, (*y * 1000000.0) as i64)
            }
            _ => return Err(VMError::TypeError("cannot compare".into())),
        };
        self.stack.push(Value::Bool(result))?;
        Ok(())
    }

    fn is_truthy(&self, value: &Value) -> bool {
        match value {
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

    fn contains(&self, container: &Value, item: &Value) -> VMResult<bool> {
        match container {
            Value::Str(s) => {
                if let Value::Str(sub) = item {
                    Ok(s.contains(sub.as_ref()))
                } else {
                    Err(VMError::TypeError("'in' requires string as left operand".into()))
                }
            }
            Value::List(l) => Ok(l.lock().unwrap().contains(item)),
            Value::Tuple(t) => Ok(t.contains(item)),
            Value::Set(s) => {
                if let Some(key) = self.value_to_key(item) {
                    Ok(s.lock().unwrap().contains(&key))
                } else {
                    Ok(false)
                }
            }
            Value::Dict(d) => {
                if let Some(key) = self.value_to_key(item) {
                    Ok(d.lock().unwrap().contains_key(&key))
                } else {
                    Ok(false)
                }
            }
            _ => Err(VMError::TypeError("argument of type is not iterable".into())),
        }
    }

    fn value_to_key(&self, value: &Value) -> Option<ValueKey> {
        match value {
            Value::None => Some(ValueKey::None),
            Value::Bool(b) => Some(ValueKey::Bool(*b)),
            Value::Int(n) => Some(ValueKey::Int(*n)),
            Value::Str(s) => Some(ValueKey::Str(s.clone())),
            Value::Bytes(b) => Some(ValueKey::Bytes(b.clone())),
            _ => None,
        }
    }

    fn get_item(&self, obj: &Value, key: &Value) -> VMResult<Value> {
        match obj {
            Value::List(l) => {
                if let Value::Int(idx) = key {
                    let guard = l.lock().unwrap();
                    let idx = self.normalize_index(*idx, guard.len())?;
                    guard.get(idx).cloned().ok_or_else(|| VMError::IndexError("list index out of range".into()))
                } else {
                    Err(VMError::TypeError("list indices must be integers".into()))
                }
            }
            Value::Tuple(t) => {
                if let Value::Int(idx) = key {
                    let idx = self.normalize_index(*idx, t.len())?;
                    t.get(idx).cloned().ok_or_else(|| VMError::IndexError("tuple index out of range".into()))
                } else {
                    Err(VMError::TypeError("tuple indices must be integers".into()))
                }
            }
            Value::Str(s) => {
                if let Value::Int(idx) = key {
                    let idx = self.normalize_index(*idx, s.len())?;
                    s.chars().nth(idx)
                        .map(|c| Value::Str(c.to_string().into()))
                        .ok_or_else(|| VMError::IndexError("string index out of range".into()))
                } else {
                    Err(VMError::TypeError("string indices must be integers".into()))
                }
            }
            Value::Dict(d) => {
                if let Some(k) = self.value_to_key(key) {
                    d.lock().unwrap().get(&k).cloned().ok_or_else(|| VMError::KeyError(format!("{:?}", key)))
                } else {
                    Err(VMError::TypeError("unhashable type".into()))
                }
            }
            _ => Err(VMError::TypeError("object is not subscriptable".into())),
        }
    }

    fn normalize_index(&self, idx: i64, len: usize) -> VMResult<usize> {
        let len = len as i64;
        let normalized = if idx < 0 { len + idx } else { idx };
        if normalized < 0 || normalized >= len {
            Err(VMError::IndexError("index out of range".into()))
        } else {
            Ok(normalized as usize)
        }
    }

    fn set_item(&mut self, _obj: Value, _key: Value, _value: Value) -> VMResult<()> {
        // Mutation of containers would require Arc<Mutex<...>> or similar
        Err(VMError::NotImplemented("item assignment".into()))
    }

    fn get_attr(&self, obj: &Value, name: &str) -> VMResult<Value> {
        match obj {
            Value::Str(s) => match name {
                "upper" | "lower" | "strip" | "split" => {
                    Ok(Value::Str(format!("<method '{}'>", name).into()))
                }
                _ => Err(VMError::AttributeError(format!("'str' has no attribute '{}'", name))),
            }
            _ => Err(VMError::AttributeError(format!("object has no attribute '{}'", name))),
        }
    }

    fn set_attr(&mut self, _obj: Value, _name: &str, _value: Value) -> VMResult<()> {
        Err(VMError::NotImplemented("attribute assignment".into()))
    }

    fn make_iterator(&self, value: Value) -> VMResult<Value> {
        // Simplified: just return the value itself for iteration
        match value {
            Value::List(_) | Value::Tuple(_) | Value::Str(_) | Value::Set(_) | Value::Dict(_) => {
                Ok(value)
            }
            _ => Err(VMError::TypeError("object is not iterable".into())),
        }
    }

    fn next_item(&mut self, _iter: &mut Value) -> Option<Value> {
        // Simplified iteration - would need actual iterator state
        None
    }

    fn unpack(&self, value: &Value, count: usize) -> VMResult<Vec<Value>> {
        let items: Vec<Value> = match value {
            Value::List(l) => l.lock().unwrap().iter().cloned().collect(),
            Value::Tuple(t) => t.iter().cloned().collect(),
            _ => return Err(VMError::TypeError("cannot unpack non-sequence".into())),
        };
        if items.len() != count {
            return Err(VMError::ValueError(format!(
                "not enough values to unpack (expected {}, got {})",
                count, items.len()
            )));
        }
        Ok(items)
    }

    fn call_builtin(&mut self, func: Value, args: Vec<Value>) -> VMResult<Value> {
        // Check if it's a builtin
        if let Value::Str(name) = &func {
            if let Some(builtin) = self.builtins.get(name.as_ref()).cloned() {
                return builtin(self, args);
            }
        }
        Err(VMError::TypeError("object is not callable".into()))
    }

    /// Sets a global variable.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.globals.insert(name.to_string(), value);
    }

    /// Gets a global variable.
    pub fn get_global(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of executing an instruction.
enum ExecResult {
    Continue,
    Return(Value),
    Yield(Value),
    /// Await a coroutine - suspends execution and returns the awaitable.
    Await(Value),
    Halt,
}

// Built-in functions

fn builtin_print(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    let output: Vec<String> = args.iter().map(|v| {
        match v {
            Value::Str(s) => s.to_string(),
            Value::None => "None".to_string(),
            Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{:.1}", f)
                } else {
                    format!("{}", f)
                }
            }
            other => format!("{:?}", other),
        }
    }).collect();
    println!("{}", output.join(" "));
    Ok(Value::None)
}

fn builtin_len(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("len() takes exactly 1 argument".into()));
    }
    let len = match &args[0] {
        Value::Str(s) => s.len(),
        Value::Bytes(b) => b.len(),
        Value::List(l) => l.lock().unwrap().len(),
        Value::Tuple(t) => t.len(),
        Value::Dict(d) => d.lock().unwrap().len(),
        Value::Set(s) => s.lock().unwrap().len(),
        _ => return Err(VMError::TypeError("object has no len()".into())),
    };
    Ok(Value::Int(len as i64))
}

fn builtin_type(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("type() takes exactly 1 argument".into()));
    }
    let type_name = match &args[0] {
        Value::None => "NoneType",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "str",
        Value::Bytes(_) => "bytes",
        Value::List(_) => "list",
        Value::Tuple(_) => "tuple",
        Value::Dict(_) => "dict",
        Value::Set(_) => "set",
        _ => "object",
    };
    Ok(Value::Str(format!("<class '{}'>", type_name).into()))
}

fn builtin_int(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::Int(0));
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        Value::Str(s) => s.parse::<i64>()
            .map(Value::Int)
            .map_err(|_| VMError::ValueError(format!("invalid literal for int(): '{}'", s))),
        Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
        _ => Err(VMError::TypeError("int() argument must be a string or number".into())),
    }
}

fn builtin_float(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::Float(0.0));
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Str(s) => s.parse::<f64>()
            .map(Value::Float)
            .map_err(|_| VMError::ValueError(format!("could not convert string to float: '{}'", s))),
        _ => Err(VMError::TypeError("float() argument must be a string or number".into())),
    }
}

fn builtin_str(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::Str("".into()));
    }
    Ok(Value::Str(format!("{:?}", args[0]).into()))
}

fn builtin_bool(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::Bool(false));
    }
    let truthy = match &args[0] {
        Value::None => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::List(l) => !l.lock().unwrap().is_empty(),
        Value::Tuple(t) => !t.is_empty(),
        Value::Dict(d) => !d.lock().unwrap().is_empty(),
        Value::Set(s) => !s.lock().unwrap().is_empty(),
        _ => true,
    };
    Ok(Value::Bool(truthy))
}

fn builtin_list(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::List(Arc::new(Mutex::new(vec![]))));
    }
    match &args[0] {
        Value::List(l) => Ok(Value::List(Arc::new(Mutex::new(l.lock().unwrap().clone())))),
        Value::Tuple(t) => Ok(Value::List(Arc::new(Mutex::new(t.to_vec())))),
        Value::Str(s) => {
            let chars: Vec<Value> = s.chars().map(|c| Value::Str(c.to_string().into())).collect();
            Ok(Value::List(Arc::new(Mutex::new(chars))))
        }
        _ => Err(VMError::TypeError("list() argument must be iterable".into())),
    }
}

fn builtin_dict(_vm: &mut VM, _args: Vec<Value>) -> VMResult<Value> {
    Ok(Value::Dict(Arc::new(Mutex::new(HashMap::new()))))
}

fn builtin_set(_vm: &mut VM, _args: Vec<Value>) -> VMResult<Value> {
    Ok(Value::Set(Arc::new(Mutex::new(HashSet::new()))))
}

fn builtin_tuple(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::Tuple(Arc::new([])));
    }
    match &args[0] {
        Value::List(l) => {
            let items = l.lock().unwrap().clone();
            Ok(Value::Tuple(Arc::from(items.into_boxed_slice())))
        }
        Value::Tuple(t) => Ok(Value::Tuple(t.clone())),
        _ => Err(VMError::TypeError("tuple() argument must be iterable".into())),
    }
}

fn builtin_range(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    let (start, stop, step) = match args.len() {
        1 => {
            if let Value::Int(stop) = args[0] {
                (0, stop, 1)
            } else {
                return Err(VMError::TypeError("range() integer expected".into()));
            }
        }
        2 => {
            if let (Value::Int(start), Value::Int(stop)) = (&args[0], &args[1]) {
                (*start, *stop, 1)
            } else {
                return Err(VMError::TypeError("range() integer expected".into()));
            }
        }
        3 => {
            if let (Value::Int(start), Value::Int(stop), Value::Int(step)) = (&args[0], &args[1], &args[2]) {
                (*start, *stop, *step)
            } else {
                return Err(VMError::TypeError("range() integer expected".into()));
            }
        }
        _ => return Err(VMError::TypeError("range() takes 1 to 3 arguments".into())),
    };

    let mut items = Vec::new();
    let mut i = start;
    while (step > 0 && i < stop) || (step < 0 && i > stop) {
        items.push(Value::Int(i));
        i += step;
    }
    Ok(Value::List(Arc::new(Mutex::new(items))))
}

fn builtin_abs(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("abs() takes exactly 1 argument".into()));
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Err(VMError::TypeError("bad operand type for abs()".into())),
    }
}

fn builtin_min(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Err(VMError::ValueError("min() arg is an empty sequence".into()));
    }
    let items = if args.len() == 1 {
        match &args[0] {
            Value::List(l) => l.lock().unwrap().clone(),
            Value::Tuple(t) => t.to_vec(),
            _ => return Err(VMError::TypeError("min() argument must be iterable".into())),
        }
    } else {
        args
    };
    items.into_iter()
        .reduce(|a, b| {
            match (&a, &b) {
                (Value::Int(x), Value::Int(y)) if x <= y => a,
                (Value::Float(x), Value::Float(y)) if x <= y => a,
                _ => b,
            }
        })
        .ok_or_else(|| VMError::ValueError("min() arg is empty".into()))
}

fn builtin_max(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Err(VMError::ValueError("max() arg is an empty sequence".into()));
    }
    let items = if args.len() == 1 {
        match &args[0] {
            Value::List(l) => l.lock().unwrap().clone(),
            Value::Tuple(t) => t.to_vec(),
            _ => return Err(VMError::TypeError("max() argument must be iterable".into())),
        }
    } else {
        args
    };
    items.into_iter()
        .reduce(|a, b| {
            match (&a, &b) {
                (Value::Int(x), Value::Int(y)) if x >= y => a,
                (Value::Float(x), Value::Float(y)) if x >= y => a,
                _ => b,
            }
        })
        .ok_or_else(|| VMError::ValueError("max() arg is empty".into()))
}

fn builtin_sum(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Err(VMError::TypeError("sum() takes at least 1 argument".into()));
    }
    let items = match &args[0] {
        Value::List(l) => l.lock().unwrap().clone(),
        Value::Tuple(t) => t.to_vec(),
        _ => return Err(VMError::TypeError("sum() argument must be iterable".into())),
    };
    let mut total: i64 = 0;
    for item in items {
        if let Value::Int(n) = item {
            total += n;
        } else {
            return Err(VMError::TypeError("unsupported operand type for sum()".into()));
        }
    }
    Ok(Value::Int(total))
}

fn builtin_sorted(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Err(VMError::TypeError("sorted() takes at least 1 argument".into()));
    }
    let mut items = match &args[0] {
        Value::List(l) => l.lock().unwrap().clone(),
        Value::Tuple(t) => t.to_vec(),
        _ => return Err(VMError::TypeError("sorted() argument must be iterable".into())),
    };
    items.sort_by(|a, b| {
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
            (Value::Str(x), Value::Str(y)) => x.cmp(y),
            _ => std::cmp::Ordering::Equal,
        }
    });
    Ok(Value::List(Arc::new(Mutex::new(items))))
}

fn builtin_reversed(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("reversed() takes exactly 1 argument".into()));
    }
    let mut items = match &args[0] {
        Value::List(l) => l.lock().unwrap().clone(),
        Value::Tuple(t) => t.to_vec(),
        Value::Str(s) => s.chars().rev().map(|c| Value::Str(c.to_string().into())).collect(),
        _ => return Err(VMError::TypeError("argument to reversed() must be a sequence".into())),
    };
    items.reverse();
    Ok(Value::List(Arc::new(Mutex::new(items))))
}

fn builtin_enumerate(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Err(VMError::TypeError("enumerate() takes at least 1 argument".into()));
    }
    let items = match &args[0] {
        Value::List(l) => l.lock().unwrap().clone(),
        Value::Tuple(t) => t.to_vec(),
        _ => return Err(VMError::TypeError("enumerate() argument must be iterable".into())),
    };
    let start = if args.len() > 1 {
        if let Value::Int(n) = args[1] { n } else { 0 }
    } else {
        0
    };
    let result: Vec<Value> = items.into_iter()
        .enumerate()
        .map(|(i, v)| Value::Tuple(Arc::new([Value::Int(start + i as i64), v])))
        .collect();
    Ok(Value::List(Arc::new(Mutex::new(result))))
}

fn builtin_zip(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.is_empty() {
        return Ok(Value::List(Arc::new(Mutex::new(vec![]))));
    }
    let lists: Vec<Vec<Value>> = args.iter()
        .map(|a| match a {
            Value::List(l) => l.lock().unwrap().clone(),
            Value::Tuple(t) => t.to_vec(),
            _ => vec![],
        })
        .collect();
    let min_len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let mut result = Vec::new();
    for i in 0..min_len {
        let tuple: Vec<Value> = lists.iter().map(|l| l[i].clone()).collect();
        result.push(Value::Tuple(Arc::from(tuple.into_boxed_slice())));
    }
    Ok(Value::List(Arc::new(Mutex::new(result))))
}

fn builtin_map(_vm: &mut VM, _args: Vec<Value>) -> VMResult<Value> {
    Err(VMError::NotImplemented("map() requires callable support".into()))
}

fn builtin_filter(_vm: &mut VM, _args: Vec<Value>) -> VMResult<Value> {
    Err(VMError::NotImplemented("filter() requires callable support".into()))
}

fn builtin_input(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if !args.is_empty() {
        if let Value::Str(prompt) = &args[0] {
            print!("{}", prompt);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).map_err(|e| VMError::RuntimeError(e.to_string()))?;
    Ok(Value::Str(line.trim_end().to_string().into()))
}

fn builtin_ord(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("ord() takes exactly 1 argument".into()));
    }
    if let Value::Str(s) = &args[0] {
        if s.len() != 1 {
            return Err(VMError::TypeError("ord() expected a character".into()));
        }
        Ok(Value::Int(s.chars().next().unwrap() as i64))
    } else {
        Err(VMError::TypeError("ord() expected string of length 1".into()))
    }
}

fn builtin_chr(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("chr() takes exactly 1 argument".into()));
    }
    if let Value::Int(n) = args[0] {
        if n < 0 || n > 0x10FFFF {
            return Err(VMError::ValueError("chr() arg not in range".into()));
        }
        Ok(Value::Str(char::from_u32(n as u32).unwrap_or('\0').to_string().into()))
    } else {
        Err(VMError::TypeError("chr() integer expected".into()))
    }
}

fn builtin_repr(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("repr() takes exactly 1 argument".into()));
    }
    Ok(Value::Str(format!("{:?}", args[0]).into()))
}

fn builtin_hash(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("hash() takes exactly 1 argument".into()));
    }
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    match &args[0] {
        Value::Int(n) => n.hash(&mut hasher),
        Value::Str(s) => s.hash(&mut hasher),
        Value::Bool(b) => b.hash(&mut hasher),
        _ => return Err(VMError::TypeError("unhashable type".into())),
    }
    Ok(Value::Int(hasher.finish() as i64))
}

fn builtin_id(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 1 {
        return Err(VMError::TypeError("id() takes exactly 1 argument".into()));
    }
    // Return a pseudo-id based on the value's address
    Ok(Value::Int(&args[0] as *const _ as i64))
}

fn builtin_isinstance(_vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() != 2 {
        return Err(VMError::TypeError("isinstance() takes exactly 2 arguments".into()));
    }
    // Simplified type checking
    let type_name = match &args[1] {
        Value::Str(s) => s.as_ref(),
        _ => return Err(VMError::TypeError("isinstance() arg 2 must be a type".into())),
    };
    let matches = match (&args[0], type_name) {
        (Value::Int(_), "int") => true,
        (Value::Float(_), "float") => true,
        (Value::Str(_), "str") => true,
        (Value::Bool(_), "bool") => true,
        (Value::List(_), "list") => true,
        (Value::Tuple(_), "tuple") => true,
        (Value::Dict(_), "dict") => true,
        (Value::Set(_), "set") => true,
        (Value::None, "NoneType") => true,
        _ => false,
    };
    Ok(Value::Bool(matches))
}

fn builtin_hasattr(_vm: &mut VM, _args: Vec<Value>) -> VMResult<Value> {
    // Simplified - always returns false
    Ok(Value::Bool(false))
}

fn builtin_getattr(vm: &mut VM, args: Vec<Value>) -> VMResult<Value> {
    if args.len() < 2 {
        return Err(VMError::TypeError("getattr() takes at least 2 arguments".into()));
    }
    if let Value::Str(name) = &args[1] {
        vm.get_attr(&args[0], name)
            .or_else(|_| if args.len() > 2 { Ok(args[2].clone()) } else { Err(VMError::AttributeError("attribute not found".into())) })
    } else {
        Err(VMError::TypeError("attribute name must be string".into()))
    }
}

fn builtin_setattr(_vm: &mut VM, _args: Vec<Value>) -> VMResult<Value> {
    Err(VMError::NotImplemented("setattr()".into()))
}
