//! Structured error codes for the Roast compiler.
//!
//! Error codes follow Rust's convention: E0xxx where xxx is a 4-digit number.
//! Each error has:
//! - A unique code (e.g., E0001)
//! - A short description
//! - A detailed explanation
//! - Suggestions for fixing
//!
//! Users can run `roastc --explain E0001` to see detailed help.

use std::collections::HashMap;

// =============================================================================
// Error Code Registry
// =============================================================================

/// An error code with its metadata.
#[derive(Debug, Clone)]
pub struct ErrorInfo {
    /// The error code (e.g., "E0001").
    pub code: &'static str,
    /// Short description for error message.
    pub summary: &'static str,
    /// Detailed explanation.
    pub explanation: &'static str,
    /// Suggestions for fixing.
    pub suggestions: &'static [&'static str],
}

impl ErrorInfo {
    /// Create a new error info.
    pub const fn new(
        code: &'static str,
        summary: &'static str,
        explanation: &'static str,
        suggestions: &'static [&'static str],
    ) -> Self {
        Self {
            code,
            summary,
            explanation,
            suggestions,
        }
    }

    /// Format detailed help text.
    pub fn help_text(&self) -> String {
        let mut text = format!("Error {}: {}\n\n", self.code, self.summary);
        text.push_str("Explanation:\n");
        text.push_str(self.explanation);
        text.push_str("\n\n");

        if !self.suggestions.is_empty() {
            text.push_str("Suggestions:\n");
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                text.push_str(&format!("  {}. {}\n", i + 1, suggestion));
            }
        }

        text
    }
}

// =============================================================================
// Error Code Definitions
// =============================================================================

// Parser Errors: E0001-E0099
pub const E0001: ErrorInfo = ErrorInfo::new(
    "E0001",
    "unexpected token",
    "The parser encountered a token it didn't expect at this position.
This usually happens when there's a syntax error like a missing comma,
bracket, or keyword.",
    &[
        "Check for missing commas between list elements",
        "Ensure all brackets and parentheses are properly matched",
        "Verify that keywords like 'def', 'class', 'if' are spelled correctly",
    ],
);

pub const E0002: ErrorInfo = ErrorInfo::new(
    "E0002",
    "unexpected end of file",
    "The file ended unexpectedly, usually because a block or expression
was not completed.",
    &[
        "Check for unclosed brackets, parentheses, or braces",
        "Ensure all strings are properly closed with matching quotes",
        "Verify that all function and class definitions have bodies",
    ],
);

pub const E0003: ErrorInfo = ErrorInfo::new(
    "E0003",
    "invalid indentation",
    "Roast uses indentation to define blocks, similar to Python.
The indentation level doesn't match what was expected.",
    &[
        "Use consistent indentation (4 spaces recommended)",
        "Don't mix tabs and spaces",
        "Ensure the block is properly indented after a colon",
    ],
);

pub const E0004: ErrorInfo = ErrorInfo::new(
    "E0004",
    "invalid escape sequence",
    "The string contains an escape sequence that is not recognized.",
    &[
        "Valid escape sequences: \\n, \\t, \\r, \\\\, \\', \\\", \\0",
        "For raw strings, use r\"...\" prefix",
        "To include a backslash, use \\\\",
    ],
);

// Type Errors: E0100-E0199
pub const E0100: ErrorInfo = ErrorInfo::new(
    "E0100",
    "type mismatch",
    "The type of an expression doesn't match what was expected.
This is a fundamental type error that prevents the program from compiling.",
    &[
        "Check that function arguments match the expected types",
        "Verify that return values match the declared return type",
        "Use explicit type conversions if needed (e.g., int(x), str(x))",
    ],
);

pub const E0101: ErrorInfo = ErrorInfo::new(
    "E0101",
    "undefined name",
    "A variable, function, or type was used but not defined in scope.",
    &[
        "Check for typos in the name",
        "Ensure the variable is defined before use",
        "Import the module if the name is from another file",
    ],
);

pub const E0102: ErrorInfo = ErrorInfo::new(
    "E0102",
    "cannot infer type",
    "The type checker cannot determine the type of this expression.
TypeAnnotation is needed.",
    &[
        "Add an explicit type annotation: x: int = ...",
        "Provide more context that helps type inference",
        "Ensure generic functions have enough type information",
    ],
);

pub const E0103: ErrorInfo = ErrorInfo::new(
    "E0103",
    "missing type annotation",
    "A type annotation is required but was not provided.",
    &[
        "Add type annotation to function parameters: def foo(x: int)",
        "Add return type annotation: def foo() -> int:",
        "Annotate class fields: name: str",
    ],
);

pub const E0104: ErrorInfo = ErrorInfo::new(
    "E0104",
    "incompatible types in operation",
    "The operator cannot be applied to these types.",
    &[
        "Check that both operands have compatible types",
        "Implement the required trait/protocol for custom types",
        "Use explicit type conversion",
    ],
);

// Borrow Checker Errors: E0200-E0299
pub const E0200: ErrorInfo = ErrorInfo::new(
    "E0200",
    "use of moved value",
    "A value was used after it was moved to another location.
In Roast, owned values can only be used once unless they implement Copy.",
    &[
        "Clone the value before moving: x.clone()",
        "Use a reference instead of moving: &x",
        "Restructure code to avoid using after move",
    ],
);

pub const E0201: ErrorInfo = ErrorInfo::new(
    "E0201",
    "cannot borrow as mutable",
    "A value cannot be borrowed mutably because it's already borrowed,
or because it wasn't declared as mutable.",
    &[
        "Declare the variable as mutable: mut x = ...",
        "Ensure no immutable borrows are active",
        "Consider using interior mutability (Cell, RefCell)",
    ],
);

pub const E0202: ErrorInfo = ErrorInfo::new(
    "E0202",
    "borrow conflicts with existing borrow",
    "Multiple borrows are conflicting. Either multiple mutable borrows,
or a mutable borrow while an immutable borrow exists.",
    &[
        "End the first borrow before starting the second",
        "Clone the data to avoid shared borrowing",
        "Restructure code to separate the borrowing phases",
    ],
);

pub const E0203: ErrorInfo = ErrorInfo::new(
    "E0203",
    "cannot move out of borrowed content",
    "An attempt was made to move a value out of a borrowed reference.",
    &[
        "Clone the value instead of moving",
        "Use take() or replace() if available",
        "Restructure to own the value directly",
    ],
);

// Import Errors: E0300-E0399
pub const E0300: ErrorInfo = ErrorInfo::new(
    "E0300",
    "module not found",
    "The specified module could not be located.",
    &[
        "Check the module path is correct",
        "Ensure the file exists and has a .roast or .ro extension",
        "Verify the module is in the search path",
    ],
);

pub const E0301: ErrorInfo = ErrorInfo::new(
    "E0301",
    "circular import detected",
    "Module A imports B, which imports A (directly or transitively).",
    &[
        "Restructure code to remove the circular dependency",
        "Move shared code to a third module",
        "Use lazy imports where possible",
    ],
);

// Class/Function Errors: E0400-E0499
pub const E0400: ErrorInfo = ErrorInfo::new(
    "E0400",
    "method not found",
    "The method does not exist on this type.",
    &[
        "Check the method name for typos",
        "Verify the type has this method defined",
        "Import the trait that provides this method",
    ],
);

pub const E0401: ErrorInfo = ErrorInfo::new(
    "E0401",
    "wrong number of arguments",
    "A function was called with the wrong number of arguments.",
    &[
        "Check the function signature for required parameters",
        "Provide default values for optional parameters",
        "Use *args or **kwargs for variable arguments",
    ],
);

pub const E0402: ErrorInfo = ErrorInfo::new(
    "E0402",
    "missing required argument",
    "A required argument was not provided in the function call.",
    &[
        "Check which arguments are required vs optional",
        "Provide the missing argument",
        "Use named arguments for clarity",
    ],
);

// =============================================================================
// Error Code Registry
// =============================================================================

/// Get all registered error codes.
pub fn all_error_codes() -> Vec<&'static ErrorInfo> {
    vec![
        // Parser
        &E0001, &E0002, &E0003, &E0004,
        // Type
        &E0100, &E0101, &E0102, &E0103, &E0104,
        // Borrow
        &E0200, &E0201, &E0202, &E0203,
        // Import
        &E0300, &E0301,
        // Class/Function
        &E0400, &E0401, &E0402,
    ]
}

/// Look up an error code by its string (e.g., "E0001").
pub fn lookup_error(code: &str) -> Option<&'static ErrorInfo> {
    let code = code.to_uppercase();
    all_error_codes()
        .into_iter()
        .find(|e| e.code == code)
}

/// Print explanation for an error code.
pub fn explain_error(code: &str) {
    match lookup_error(code) {
        Some(info) => {
            println!("{}", info.help_text());
        }
        None => {
            println!("Unknown error code: {}", code);
            println!("Use 'roastc --explain E0001' format.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_error() {
        let info = lookup_error("E0001").expect("E0001 should exist");
        assert_eq!(info.code, "E0001");
        assert!(info.summary.contains("token"));
    }

    #[test]
    fn test_lookup_case_insensitive() {
        let info = lookup_error("e0100").expect("e0100 should exist");
        assert_eq!(info.code, "E0100");
    }

    #[test]
    fn test_all_error_codes() {
        let codes = all_error_codes();
        assert!(codes.len() >= 10);
    }
}
