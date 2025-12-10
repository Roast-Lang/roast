//! Call frame management.

use roast_codegen::Bytecode;
use roast_runtime::Value;
use std::sync::Arc;

/// Pre-allocated array size for locals (avoids allocation for small functions)
const SMALL_FRAME_LOCALS: usize = 8;

/// A call frame represents a function invocation.
#[derive(Clone)]
pub struct CallFrame {
    /// The bytecode being executed.
    pub code: Arc<Bytecode>,
    /// Instruction pointer (current instruction index).
    pub ip: usize,
    /// Base pointer (start of this frame's locals on the stack).
    pub bp: usize,
    /// Local variables - using SmallVec-like optimization
    /// For small functions (<=8 locals), uses inline storage
    pub locals: LocalsStorage,
    /// The state of this frame.
    pub state: FrameState,
    /// Exception handler stack.
    pub handlers: Vec<ExceptionHandler>,
    /// Whether this is a generator frame.
    pub is_generator: bool,
    /// Saved stack for generators.
    pub saved_stack: Vec<Value>,
}

/// Optimized storage for local variables
/// Uses inline storage for small frames to avoid heap allocation
#[derive(Clone)]
pub enum LocalsStorage {
    /// Inline storage for small frames (up to 8 locals) - no heap allocation
    Small {
        data: [Value; SMALL_FRAME_LOCALS],
        len: usize,
    },
    /// Heap storage for larger frames
    Large(Vec<Value>),
}

impl LocalsStorage {
    #[inline]
    pub fn new(num_locals: usize) -> Self {
        if num_locals <= SMALL_FRAME_LOCALS {
            LocalsStorage::Small {
                data: [
                    Value::None, Value::None, Value::None, Value::None,
                    Value::None, Value::None, Value::None, Value::None,
                ],
                len: num_locals,
            }
        } else {
            LocalsStorage::Large(vec![Value::None; num_locals])
        }
    }

    #[inline]
    pub fn get(&self, slot: usize) -> Option<&Value> {
        match self {
            LocalsStorage::Small { data, len } => {
                if slot < *len { Some(&data[slot]) } else { None }
            }
            LocalsStorage::Large(v) => v.get(slot),
        }
    }

    #[inline]
    pub fn set(&mut self, slot: usize, value: Value) {
        match self {
            LocalsStorage::Small { data, len } => {
                if slot < *len {
                    data[slot] = value;
                }
            }
            LocalsStorage::Large(v) => {
                if slot < v.len() {
                    v[slot] = value;
                }
            }
        }
    }

    /// Convert to Vec<Value> (for coroutine suspension)
    #[inline]
    pub fn to_vec(&self) -> Vec<Value> {
        match self {
            LocalsStorage::Small { data, len } => data[..*len].to_vec(),
            LocalsStorage::Large(v) => v.clone(),
        }
    }

    /// Iterate over locals
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        match self {
            LocalsStorage::Small { data, len } => LocalsIter::Small(data[..*len].iter()),
            LocalsStorage::Large(v) => LocalsIter::Large(v.iter()),
        }
    }

    /// Get length
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            LocalsStorage::Small { len, .. } => *len,
            LocalsStorage::Large(v) => v.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Iterator wrapper for LocalsStorage
pub enum LocalsIter<'a> {
    Small(std::slice::Iter<'a, Value>),
    Large(std::slice::Iter<'a, Value>),
}

impl<'a> Iterator for LocalsIter<'a> {
    type Item = &'a Value;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LocalsIter::Small(iter) => iter.next(),
            LocalsIter::Large(iter) => iter.next(),
        }
    }
}

impl CallFrame {
    /// Creates a new call frame.
    #[inline]
    pub fn new(code: Arc<Bytecode>, bp: usize) -> Self {
        let num_locals = code.num_locals as usize;
        Self {
            code,
            ip: 0,
            bp,
            locals: LocalsStorage::new(num_locals),
            state: FrameState::Running,
            handlers: Vec::new(),
            is_generator: false,
            saved_stack: Vec::new(),
        }
    }

    /// Gets a local variable.
    #[inline]
    pub fn get_local(&self, slot: usize) -> Option<&Value> {
        self.locals.get(slot)
    }

    /// Sets a local variable.
    #[inline]
    pub fn set_local(&mut self, slot: usize, value: Value) {
        self.locals.set(slot, value);
    }

    /// Gets the current instruction.
    #[inline]
    pub fn current_instruction(&self) -> Option<&roast_codegen::Instruction> {
        self.code.instructions.get(self.ip)
    }

    /// Advances the instruction pointer.
    #[inline]
    pub fn advance(&mut self) {
        self.ip += 1;
    }

    /// Jumps to an offset.
    #[inline]
    pub fn jump(&mut self, offset: i16) {
        self.ip = (self.ip as isize + offset as isize) as usize;
    }

    /// Jumps to an absolute position.
    #[inline]
    pub fn jump_to(&mut self, target: usize) {
        self.ip = target;
    }

    /// Pushes an exception handler.
    pub fn push_handler(&mut self, handler: ExceptionHandler) {
        self.handlers.push(handler);
    }

    /// Pops an exception handler.
    pub fn pop_handler(&mut self) -> Option<ExceptionHandler> {
        self.handlers.pop()
    }

    /// Returns true if execution is complete.
    pub fn is_done(&self) -> bool {
        matches!(self.state, FrameState::Returned | FrameState::Raised(_))
    }
}

impl std::fmt::Debug for CallFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallFrame")
            .field("function", &self.code.name)
            .field("ip", &self.ip)
            .field("bp", &self.bp)
            .field("state", &self.state)
            .finish()
    }
}

/// State of a call frame.
#[derive(Clone, Debug)]
pub enum FrameState {
    /// Frame is currently executing.
    Running,
    /// Frame is suspended (generator/async).
    Suspended,
    /// Frame returned a value.
    Returned,
    /// Frame raised an exception.
    Raised(Value),
    /// Frame is awaiting.
    Awaiting,
}

/// An exception handler.
#[derive(Clone, Debug)]
pub struct ExceptionHandler {
    /// The instruction to jump to when an exception is caught.
    pub target: usize,
    /// The stack level to restore.
    pub stack_level: usize,
    /// The type of exception to catch (None = catch all).
    pub exception_type: Option<String>,
}

impl ExceptionHandler {
    pub fn new(target: usize, stack_level: usize) -> Self {
        Self {
            target,
            stack_level,
            exception_type: None,
        }
    }

    pub fn with_type(mut self, ty: String) -> Self {
        self.exception_type = Some(ty);
        self
    }
}
