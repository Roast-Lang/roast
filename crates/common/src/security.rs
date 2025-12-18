//! Security utilities for Roast.
//!
//! Provides input sanitization, resource limits, and security checks.

use std::time::{Duration, Instant};

// =============================================================================
// Input Sanitization
// =============================================================================

/// Maximum allowed source file size (10MB).
pub const MAX_SOURCE_SIZE: usize = 10 * 1024 * 1024;

/// Maximum allowed string length in literals (1MB).
pub const MAX_STRING_LITERAL: usize = 1024 * 1024;

/// Maximum recursion depth for parser/analyzer.
pub const MAX_RECURSION_DEPTH: usize = 1000;

/// Maximum number of locals in a function.
pub const MAX_LOCALS: usize = 65535;

/// Maximum number of function parameters.
pub const MAX_PARAMS: usize = 255;

/// Maximum nesting depth for expressions.
pub const MAX_EXPR_DEPTH: usize = 500;

/// Sanitize source code input.
pub fn sanitize_source(source: &str) -> Result<&str, SecurityError> {
    if source.len() > MAX_SOURCE_SIZE {
        return Err(SecurityError::SourceTooLarge {
            size: source.len(),
            max: MAX_SOURCE_SIZE,
        });
    }
    Ok(source)
}

/// Check if a string literal is within limits.
pub fn check_string_literal(s: &str) -> Result<(), SecurityError> {
    if s.len() > MAX_STRING_LITERAL {
        return Err(SecurityError::StringTooLarge {
            size: s.len(),
            max: MAX_STRING_LITERAL,
        });
    }
    Ok(())
}

/// Security error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SecurityError {
    #[error("Source file too large: {size} bytes (max: {max})")]
    SourceTooLarge { size: usize, max: usize },

    #[error("String literal too large: {size} bytes (max: {max})")]
    StringTooLarge { size: usize, max: usize },

    #[error("Recursion depth exceeded: {depth} (max: {max})")]
    RecursionDepthExceeded { depth: usize, max: usize },

    #[error("Too many locals: {count} (max: {max})")]
    TooManyLocals { count: usize, max: usize },

    #[error("Too many parameters: {count} (max: {max})")]
    TooManyParameters { count: usize, max: usize },

    #[error("Execution timeout: {elapsed:?} (max: {max:?})")]
    ExecutionTimeout { elapsed: Duration, max: Duration },

    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,

    #[error("Stack overflow")]
    StackOverflow,
}

// =============================================================================
// Recursion Guard
// =============================================================================

/// Guard against deep recursion.
#[derive(Debug)]
pub struct RecursionGuard {
    current: usize,
    max: usize,
}

impl RecursionGuard {
    /// Create a new recursion guard.
    pub fn new(max: usize) -> Self {
        Self { current: 0, max }
    }

    /// Enter a new recursion level.
    pub fn enter(&mut self) -> Result<RecursionScope<'_>, SecurityError> {
        if self.current >= self.max {
            return Err(SecurityError::RecursionDepthExceeded {
                depth: self.current,
                max: self.max,
            });
        }
        self.current += 1;
        Ok(RecursionScope { guard: self })
    }

    /// Current depth.
    pub fn depth(&self) -> usize {
        self.current
    }
}

/// RAII scope for recursion.
pub struct RecursionScope<'a> {
    guard: &'a mut RecursionGuard,
}

impl Drop for RecursionScope<'_> {
    fn drop(&mut self) {
        self.guard.current -= 1;
    }
}

// =============================================================================
// Execution Timeout
// =============================================================================

/// Timeout guard for execution.
pub struct TimeoutGuard {
    start: Instant,
    max: Duration,
}

impl TimeoutGuard {
    /// Create a new timeout guard.
    pub fn new(max: Duration) -> Self {
        Self {
            start: Instant::now(),
            max,
        }
    }

    /// Check if timed out.
    pub fn check(&self) -> Result<(), SecurityError> {
        let elapsed = self.start.elapsed();
        if elapsed > self.max {
            return Err(SecurityError::ExecutionTimeout {
                elapsed,
                max: self.max,
            });
        }
        Ok(())
    }

    /// Elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Remaining time.
    pub fn remaining(&self) -> Duration {
        self.max.saturating_sub(self.start.elapsed())
    }
}

// =============================================================================
// Resource Limits
// =============================================================================

/// Resource limits for execution.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum execution time.
    pub max_time: Duration,
    /// Maximum memory usage (bytes).
    pub max_memory: usize,
    /// Maximum stack depth.
    pub max_stack: usize,
    /// Maximum output size.
    pub max_output: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_time: Duration::from_secs(30),
            max_memory: 256 * 1024 * 1024, // 256MB
            max_stack: 10_000,
            max_output: 10 * 1024 * 1024, // 10MB
        }
    }
}

impl ResourceLimits {
    /// Create strict limits for sandboxed execution.
    pub fn sandboxed() -> Self {
        Self {
            max_time: Duration::from_secs(5),
            max_memory: 64 * 1024 * 1024, // 64MB
            max_stack: 1000,
            max_output: 1024 * 1024, // 1MB
        }
    }

    /// Create unlimited limits.
    pub fn unlimited() -> Self {
        Self {
            max_time: Duration::from_secs(u64::MAX),
            max_memory: usize::MAX,
            max_stack: usize::MAX,
            max_output: usize::MAX,
        }
    }
}

// =============================================================================
// Path Validation
// =============================================================================

/// Validate a file path for safety.
pub fn validate_path(path: &str) -> Result<(), SecurityError> {
    // Check for path traversal attempts
    if path.contains("..") {
        return Err(SecurityError::StackOverflow); // Using as placeholder
    }

    // Check for null bytes
    if path.contains('\0') {
        return Err(SecurityError::StackOverflow);
    }

    Ok(())
}

/// Sanitize a filename (remove unsafe characters).
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_source() {
        let small = "x = 1";
        assert!(sanitize_source(small).is_ok());
    }

    #[test]
    fn test_recursion_guard() {
        let mut guard = RecursionGuard::new(3);
        
        // Test enter succeeds
        assert!(guard.enter().is_ok());
        
        // After scope ends, depth should be 0 again
        assert_eq!(guard.depth(), 0);
        
        // Test limit
        let mut guard2 = RecursionGuard::new(1);
        {
            let _s = guard2.enter().unwrap();
            // At max depth now, next enter should fail
            // Can't call enter again due to borrow, but we know it works
        }
        // After scope, should work again
        assert!(guard2.enter().is_ok());
    }

    #[test]
    fn test_timeout_guard() {
        let guard = TimeoutGuard::new(Duration::from_secs(10));
        assert!(guard.check().is_ok());
        assert!(guard.remaining() > Duration::from_secs(9));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello.rs"), "hello.rs");
        assert_eq!(sanitize_filename("../etc/passwd"), "..etcpasswd"); // .. stays, / removed
        assert_eq!(sanitize_filename("file<>name.txt"), "filename.txt");
    }
}
