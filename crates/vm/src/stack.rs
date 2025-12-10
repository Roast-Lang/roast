//! Value stack for the VM.

use roast_runtime::Value;
use std::fmt;

/// The operand stack for the VM.
pub struct ValueStack {
    values: Vec<Value>,
    capacity: usize,
}

impl ValueStack {
    /// Creates a new stack with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Pushes a value onto the stack.
    #[inline]
    pub fn push(&mut self, value: Value) -> Result<(), StackError> {
        if self.values.len() >= self.capacity {
            return Err(StackError::Overflow);
        }
        self.values.push(value);
        Ok(())
    }

    /// Pops a value from the stack.
    #[inline]
    pub fn pop(&mut self) -> Result<Value, StackError> {
        self.values.pop().ok_or(StackError::Underflow)
    }

    /// Peeks at the top value without removing it.
    #[inline]
    pub fn peek(&self) -> Result<&Value, StackError> {
        self.values.last().ok_or(StackError::Underflow)
    }

    /// Peeks at a value at a specific offset from the top.
    #[inline]
    pub fn peek_at(&self, offset: usize) -> Result<&Value, StackError> {
        if offset >= self.values.len() {
            return Err(StackError::Underflow);
        }
        Ok(&self.values[self.values.len() - 1 - offset])
    }

    /// Gets a mutable reference to the top value.
    #[inline]
    pub fn peek_mut(&mut self) -> Result<&mut Value, StackError> {
        self.values.last_mut().ok_or(StackError::Underflow)
    }

    /// Duplicates the top value.
    #[inline]
    pub fn dup(&mut self) -> Result<(), StackError> {
        let value = self.peek()?.clone();
        self.push(value)
    }

    /// Swaps the top two values.
    #[inline]
    pub fn swap(&mut self) -> Result<(), StackError> {
        let len = self.values.len();
        if len < 2 {
            return Err(StackError::Underflow);
        }
        self.values.swap(len - 1, len - 2);
        Ok(())
    }

    /// Rotates the top three values: [a, b, c] -> [b, c, a]
    #[inline]
    pub fn rot3(&mut self) -> Result<(), StackError> {
        let len = self.values.len();
        if len < 3 {
            return Err(StackError::Underflow);
        }
        let a = self.values.remove(len - 3);
        self.values.push(a);
        Ok(())
    }

    /// Pops n values from the stack.
    #[inline]
    pub fn pop_n(&mut self, n: usize) -> Result<Vec<Value>, StackError> {
        if self.values.len() < n {
            return Err(StackError::Underflow);
        }
        let start = self.values.len() - n;
        Ok(self.values.drain(start..).collect())
    }

    /// Pops n values from the stack without returning them (for when values are already copied elsewhere)
    #[inline]
    pub fn pop_n_discard(&mut self, n: usize) -> Result<(), StackError> {
        if self.values.len() < n {
            return Err(StackError::Underflow);
        }
        self.values.truncate(self.values.len() - n);
        Ok(())
    }

    /// Returns the current stack size.
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true if the stack is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Gets a mutable reference to a value at a specific index.
    #[inline]
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Value> {
        self.values.get_mut(idx)
    }

    /// Truncates the stack to the given size.
    #[inline]
    pub fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }

    /// Clears the stack.
    #[inline]
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Gets a slice of the stack values.
    pub fn as_slice(&self) -> &[Value] {
        &self.values
    }

    /// Gets a value at an absolute index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.values.get(index)
    }

    /// Sets a value at an absolute index.
    #[inline]
    pub fn set(&mut self, index: usize, value: Value) -> Result<(), StackError> {
        if index >= self.values.len() {
            return Err(StackError::IndexOutOfBounds(index));
        }
        self.values[index] = value;
        Ok(())
    }
}

impl Default for ValueStack {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl fmt::Debug for ValueStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Stack({:?})", self.values)
    }
}

/// Stack operation errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StackError {
    #[error("stack overflow")]
    Overflow,
    #[error("stack underflow")]
    Underflow,
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(usize),
}
