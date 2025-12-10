//! Error handling utilities.

use std::fmt;
use std::error::Error;

/// A generic Roast error.
#[derive(Debug)]
pub struct RoastError {
    pub kind: ErrorKind,
    pub message: String,
    pub cause: Option<Box<dyn Error + Send + Sync>>,
}

/// Error kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Type error.
    TypeError,
    /// Value error.
    ValueError,
    /// Index out of bounds.
    IndexError,
    /// Key not found.
    KeyError,
    /// Attribute not found.
    AttributeError,
    /// Name not defined.
    NameError,
    /// Division by zero.
    ZeroDivisionError,
    /// Overflow error.
    OverflowError,
    /// Memory error.
    MemoryError,
    /// Runtime error.
    RuntimeError,
    /// IO error.
    IoError,
    /// Assertion error.
    AssertionError,
    /// Import error.
    ImportError,
    /// Timeout error.
    TimeoutError,
    /// Connection error.
    ConnectionError,
    /// Permission error.
    PermissionError,
    /// Not implemented.
    NotImplementedError,
    /// Custom error.
    Custom,
}

impl RoastError {
    /// Create a new error.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            cause: None,
        }
    }
    
    /// Create with a cause.
    pub fn with_cause<E: Error + Send + Sync + 'static>(
        kind: ErrorKind,
        message: impl Into<String>,
        cause: E,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            cause: Some(Box::new(cause)),
        }
    }
    
    /// Create a type error.
    pub fn type_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::TypeError, message)
    }
    
    /// Create a value error.
    pub fn value_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ValueError, message)
    }
    
    /// Create an index error.
    pub fn index_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::IndexError, message)
    }
    
    /// Create a key error.
    pub fn key_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::KeyError, message)
    }
    
    /// Create an IO error.
    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::IoError, message)
    }
    
    /// Create a runtime error.
    pub fn runtime_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::RuntimeError, message)
    }
    
    /// Get the error kind name.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ErrorKind::TypeError => "TypeError",
            ErrorKind::ValueError => "ValueError",
            ErrorKind::IndexError => "IndexError",
            ErrorKind::KeyError => "KeyError",
            ErrorKind::AttributeError => "AttributeError",
            ErrorKind::NameError => "NameError",
            ErrorKind::ZeroDivisionError => "ZeroDivisionError",
            ErrorKind::OverflowError => "OverflowError",
            ErrorKind::MemoryError => "MemoryError",
            ErrorKind::RuntimeError => "RuntimeError",
            ErrorKind::IoError => "IoError",
            ErrorKind::AssertionError => "AssertionError",
            ErrorKind::ImportError => "ImportError",
            ErrorKind::TimeoutError => "TimeoutError",
            ErrorKind::ConnectionError => "ConnectionError",
            ErrorKind::PermissionError => "PermissionError",
            ErrorKind::NotImplementedError => "NotImplementedError",
            ErrorKind::Custom => "Error",
        }
    }
}

impl fmt::Display for RoastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind_name(), self.message)
    }
}

impl Error for RoastError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_ref().map(|e| e.as_ref() as &(dyn Error + 'static))
    }
}

impl From<std::io::Error> for RoastError {
    fn from(err: std::io::Error) -> Self {
        Self::with_cause(ErrorKind::IoError, err.to_string(), err)
    }
}

impl From<std::fmt::Error> for RoastError {
    fn from(err: std::fmt::Error) -> Self {
        Self::new(ErrorKind::RuntimeError, err.to_string())
    }
}

impl From<std::num::ParseIntError> for RoastError {
    fn from(err: std::num::ParseIntError) -> Self {
        Self::new(ErrorKind::ValueError, err.to_string())
    }
}

impl From<std::num::ParseFloatError> for RoastError {
    fn from(err: std::num::ParseFloatError) -> Self {
        Self::new(ErrorKind::ValueError, err.to_string())
    }
}

/// Context extension for adding context to errors.
pub trait Context<T> {
    /// Add context to the error.
    fn context(self, message: impl Into<String>) -> Result<T, RoastError>;
    
    /// Add context with a closure.
    fn with_context<F, S>(self, f: F) -> Result<T, RoastError>
    where
        F: FnOnce() -> S,
        S: Into<String>;
}

impl<T, E: Error + Send + Sync + 'static> Context<T> for Result<T, E> {
    fn context(self, message: impl Into<String>) -> Result<T, RoastError> {
        self.map_err(|e| RoastError::with_cause(ErrorKind::RuntimeError, message, e))
    }
    
    fn with_context<F, S>(self, f: F) -> Result<T, RoastError>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.map_err(|e| RoastError::with_cause(ErrorKind::RuntimeError, f(), e))
    }
}

/// Ensure a condition is true, or return an error.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return Err($crate::error::RoastError::new(
                $crate::error::ErrorKind::RuntimeError,
                $msg,
            ));
        }
    };
}

/// Bail with an error.
#[macro_export]
macro_rules! bail {
    ($msg:expr) => {
        return Err($crate::error::RoastError::runtime_error($msg))
    };
}

// =============================================================================
// Error Chain
// =============================================================================

/// An error chain that preserves the full error history.
#[derive(Debug)]
pub struct ErrorChain {
    errors: Vec<RoastError>,
}

impl ErrorChain {
    /// Create a new error chain.
    pub fn new(error: RoastError) -> Self {
        Self {
            errors: vec![error],
        }
    }
    
    /// Add an error to the chain.
    pub fn chain(mut self, error: RoastError) -> Self {
        self.errors.push(error);
        self
    }
    
    /// Get the root cause.
    pub fn root(&self) -> Option<&RoastError> {
        self.errors.last()
    }
    
    /// Get the most recent error.
    pub fn current(&self) -> Option<&RoastError> {
        self.errors.first()
    }
    
    /// Iterate over errors (most recent first).
    pub fn iter(&self) -> impl Iterator<Item = &RoastError> {
        self.errors.iter()
    }
    
    /// Get the full backtrace as a string.
    pub fn backtrace(&self) -> String {
        let mut s = String::new();
        for (i, err) in self.errors.iter().enumerate() {
            if i > 0 {
                s.push_str("\nCaused by: ");
            }
            s.push_str(&err.to_string());
        }
        s
    }
}

impl fmt::Display for ErrorChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.backtrace())
    }
}

impl Error for ErrorChain {}

// =============================================================================
// Stack Trace
// =============================================================================

/// Source location information.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub function: Option<String>,
}

impl SourceLocation {
    pub fn new(file: &str, line: u32, column: u32) -> Self {
        Self {
            file: file.to_string(),
            line,
            column,
            function: None,
        }
    }
    
    pub fn with_function(mut self, func: &str) -> Self {
        self.function = Some(func.to_string());
        self
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref func) = self.function {
            write!(f, "{}:{}:{} in {}", self.file, self.line, self.column, func)
        } else {
            write!(f, "{}:{}:{}", self.file, self.line, self.column)
        }
    }
}

/// Stack trace for errors.
#[derive(Debug, Clone, Default)]
pub struct StackTrace {
    frames: Vec<SourceLocation>,
}

impl StackTrace {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }
    
    pub fn push(&mut self, loc: SourceLocation) {
        self.frames.push(loc);
    }
    
    pub fn frames(&self) -> &[SourceLocation] {
        &self.frames
    }
    
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

impl fmt::Display for StackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Traceback (most recent call last):")?;
        for frame in self.frames.iter().rev() {
            writeln!(f, "  File \"{}\", line {}", frame.file, frame.line)?;
            if let Some(ref func) = frame.function {
                writeln!(f, "    in {}", func)?;
            }
        }
        Ok(())
    }
}

/// Error with stack trace.
#[derive(Debug)]
pub struct TracedError {
    pub error: RoastError,
    pub trace: StackTrace,
}

impl TracedError {
    pub fn new(error: RoastError) -> Self {
        Self {
            error,
            trace: StackTrace::new(),
        }
    }
    
    pub fn with_trace(error: RoastError, trace: StackTrace) -> Self {
        Self { error, trace }
    }
    
    pub fn add_frame(&mut self, loc: SourceLocation) {
        self.trace.push(loc);
    }
}

impl fmt::Display for TracedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.trace, self.error)
    }
}

impl Error for TracedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.error.source()
    }
}

// =============================================================================
// Error Helpers
// =============================================================================

/// Format an error with its cause chain.
pub fn format_error_chain<E: Error>(err: &E) -> String {
    let mut s = err.to_string();
    let mut source = err.source();
    
    while let Some(cause) = source {
        s.push_str("\nCaused by: ");
        s.push_str(&cause.to_string());
        source = cause.source();
    }
    
    s
}

/// Convert any error to a RoastError.
pub fn into_roast_error<E: Error + Send + Sync + 'static>(err: E) -> RoastError {
    RoastError::with_cause(ErrorKind::RuntimeError, err.to_string(), err)
}

/// Try block that catches panics.
pub fn try_catch<T, F: FnOnce() -> Result<T, RoastError>>(f: F) -> Result<T, RoastError> {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let message = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            Err(RoastError::runtime_error(format!("Panic: {}", message)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_chain() {
        let root = RoastError::io_error("file not found");
        let chain = ErrorChain::new(RoastError::runtime_error("failed to load"))
            .chain(root);
        
        assert!(chain.backtrace().contains("failed to load"));
        assert!(chain.backtrace().contains("file not found"));
    }
    
    #[test]
    fn test_stack_trace() {
        let mut trace = StackTrace::new();
        trace.push(SourceLocation::new("main.roast", 10, 5).with_function("main"));
        trace.push(SourceLocation::new("lib.roast", 20, 10).with_function("helper"));
        
        let s = trace.to_string();
        assert!(s.contains("main.roast"));
        assert!(s.contains("lib.roast"));
    }
}

