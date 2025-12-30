//! Error hints and suggestions for common mistakes.
//!
//! This module provides helper functions that generate helpful hints
//! and suggestions for common error patterns in Roast code.

use crate::Diagnostic;
use crate::span::Span;

/// Generates hints for undefined name errors.
/// 
/// Checks for common typos and suggests similar names.
pub fn undefined_name_hints(name: &str, available_names: &[&str]) -> Vec<String> {
    let mut hints = Vec::new();
    
    // Check for common typos (edit distance <= 2)
    if let Some(suggestion) = find_similar_name(name, available_names) {
        hints.push(format!("did you mean `{}`?", suggestion));
    }
    
    // Check for common mistakes
    match name {
        "true" => hints.push("hint: in Roast, use `True` (capitalized)".to_string()),
        "false" => hints.push("hint: in Roast, use `False` (capitalized)".to_string()),
        "none" => hints.push("hint: in Roast, use `None` (capitalized)".to_string()),
        "NULL" | "null" | "nil" => hints.push("hint: in Roast, use `None` for null values".to_string()),
        "string" => hints.push("hint: in Roast, use `str` for string type".to_string()),
        "integer" => hints.push("hint: in Roast, use `int` for integer type".to_string()),
        "boolean" => hints.push("hint: in Roast, use `bool` for boolean type".to_string()),
        "array" | "Array" => hints.push("hint: in Roast, use `list` for arrays".to_string()),
        "object" | "Object" => hints.push("hint: in Roast, use `dict` for objects/maps".to_string()),
        "lambda" => hints.push("hint: lambdas are written as `lambda x: x + 1`".to_string()),
        "function" | "func" | "fn" => hints.push("hint: define functions with `def`".to_string()),
        _ => {}
    }
    
    hints
}

/// Generates hints for type mismatch errors.
pub fn type_mismatch_hints(expected: &str, found: &str) -> Vec<String> {
    let mut hints = Vec::new();
    
    // Suggest conversions
    match (expected, found) {
        ("int", "float") => hints.push("hint: use `int()` to convert float to int".to_string()),
        ("float", "int") => hints.push("hint: use `float()` to convert int to float".to_string()),
        ("str", _) => hints.push(format!("hint: use `str()` to convert {} to str", found)),
        ("int", "str") => hints.push("hint: use `int()` to parse string as integer".to_string()),
        ("float", "str") => hints.push("hint: use `float()` to parse string as float".to_string()),
        ("list", "tuple") => hints.push("hint: use `list()` to convert tuple to list".to_string()),
        ("tuple", "list") => hints.push("hint: use `tuple()` to convert list to tuple".to_string()),
        ("bool", _) => hints.push(format!("hint: use `bool()` to convert {} to bool", found)),
        _ => {}
    }
    
    hints
}

/// Generates hints for attribute errors.
pub fn attribute_error_hints(ty: &str, attr: &str) -> Vec<String> {
    let mut hints = Vec::new();
    
    // Common string methods
    if ty == "str" {
        let string_methods = ["upper", "lower", "strip", "split", "join", "replace", 
                              "startswith", "endswith", "find", "format", "count"];
        if let Some(suggestion) = find_similar_name(attr, &string_methods) {
            hints.push(format!("did you mean `{}`?", suggestion));
        }
    }
    
    // Common list methods
    if ty == "list" {
        let list_methods = ["append", "extend", "pop", "remove", "insert", "index",
                           "count", "sort", "reverse", "clear", "copy"];
        if let Some(suggestion) = find_similar_name(attr, &list_methods) {
            hints.push(format!("did you mean `{}`?", suggestion));
        }
    }
    
    // Common dict methods
    if ty == "dict" {
        let dict_methods = ["get", "set", "keys", "values", "items", "pop", 
                           "update", "clear", "copy", "setdefault"];
        if let Some(suggestion) = find_similar_name(attr, &dict_methods) {
            hints.push(format!("did you mean `{}`?", suggestion));
        }
    }
    
    // Private attribute hint
    if attr.starts_with("_") && !attr.starts_with("__") {
        hints.push("hint: attributes starting with `_` are private".to_string());
    }
    
    hints
}

/// Generates hints for borrow checker errors.
pub fn borrow_error_hints(error_kind: &str) -> Vec<String> {
    let mut hints = Vec::new();
    
    match error_kind {
        "use_after_move" => {
            hints.push("help: once a value is moved, it can no longer be used".to_string());
            hints.push("hint: consider cloning the value before passing it: `.clone()`".to_string());
            hints.push("hint: or pass a reference instead: `&value` or `&mut value`".to_string());
        }
        "double_move" => {
            hints.push("help: a value can only be moved once".to_string());
            hints.push("hint: clone the value if you need to pass it multiple times".to_string());
        }
        "borrow_while_borrowed" => {
            hints.push("help: cannot borrow mutably while already borrowed".to_string());
            hints.push("hint: ensure the previous borrow ends before this one begins".to_string());
        }
        _ => {}
    }
    
    hints
}

/// Generates hints for import errors.
pub fn import_error_hints(module: &str) -> Vec<String> {
    let mut hints = Vec::new();
    
    // Common stdlib modules
    let stdlib_modules = ["os", "sys", "io", "json", "http", "time", "datetime", 
                         "math", "random", "re", "collections", "itertools",
                         "concurrent", "async_io", "subprocess", "pathlib"];
    
    if let Some(suggestion) = find_similar_name(module, &stdlib_modules) {
        hints.push(format!("did you mean `{}`?", suggestion));
    }
    
    // Check for common module naming issues
    if module.contains("-") {
        hints.push("hint: module names use underscores, not hyphens".to_string());
        hints.push(format!("suggestion: try `{}`", module.replace("-", "_")));
    }
    
    if module.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        hints.push("hint: module names are typically lowercase".to_string());
        hints.push(format!("suggestion: try `{}`", module.to_lowercase()));
    }
    
    hints
}

/// Find a similar name using Levenshtein distance.
fn find_similar_name<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let name_lower = name.to_lowercase();
    let mut best_match = None;
    let mut best_distance = 3; // Max edit distance of 2
    
    for &candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        let distance = levenshtein_distance(&name_lower, &candidate_lower);
        
        if distance < best_distance {
            best_distance = distance;
            best_match = Some(candidate);
        }
    }
    
    best_match
}

/// Compute Levenshtein edit distance between two strings.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();
    
    if m == 0 { return n; }
    if n == 0 { return m; }
    
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i-1] == b_chars[j-1] { 0 } else { 1 };
            dp[i][j] = (dp[i-1][j] + 1)
                .min(dp[i][j-1] + 1)
                .min(dp[i-1][j-1] + cost);
        }
    }
    
    dp[m][n]
}

/// Create a rich undefined name error with suggestions.
pub fn undefined_name_error(name: &str, span: Span, available_names: &[&str]) -> Diagnostic {
    let mut diag = Diagnostic::error(format!("undefined name `{}`", name))
        .with_span(span)
        .with_code("E0001");
    
    for hint in undefined_name_hints(name, available_names) {
        diag = diag.with_note(hint);
    }
    
    diag
}

/// Create a rich type mismatch error with suggestions.
pub fn type_mismatch_error(expected: &str, found: &str, span: Span) -> Diagnostic {
    let mut diag = Diagnostic::error(format!("type mismatch: expected `{}`, found `{}`", expected, found))
        .with_span(span)
        .with_code("E0002");
    
    for hint in type_mismatch_hints(expected, found) {
        diag = diag.with_note(hint);
    }
    
    diag
}

/// Create a rich attribute error with suggestions.
pub fn attribute_error(ty: &str, attr: &str, span: Span) -> Diagnostic {
    let mut diag = Diagnostic::error(format!("type `{}` has no attribute `{}`", ty, attr))
        .with_span(span)
        .with_code("E0003");
    
    for hint in attribute_error_hints(ty, attr) {
        diag = diag.with_note(hint);
    }
    
    diag
}

/// Create a rich borrow error with suggestions.
pub fn borrow_error(kind: &str, message: &str, span: Span) -> Diagnostic {
    let mut diag = Diagnostic::error(message)
        .with_span(span)
        .with_code("E0004");
    
    for hint in borrow_error_hints(kind) {
        diag = diag.with_note(hint);
    }
    
    diag
}

/// Create a rich import error with suggestions.
pub fn import_error(module: &str, span: Span) -> Diagnostic {
    let mut diag = Diagnostic::error(format!("cannot find module `{}`", module))
        .with_span(span)
        .with_code("E0005");
    
    for hint in import_error_hints(module) {
        diag = diag.with_note(hint);
    }
    
    diag
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("cat", "cat"), 0);
        assert_eq!(levenshtein_distance("cat", "bat"), 1);
        assert_eq!(levenshtein_distance("cat", "car"), 1);
        assert_eq!(levenshtein_distance("cat", "cars"), 2);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }
    
    #[test]
    fn test_find_similar_name() {
        let candidates = ["print", "range", "len", "str", "int"];
        
        assert_eq!(find_similar_name("prnt", &candidates), Some("print"));
        assert_eq!(find_similar_name("ragne", &candidates), Some("range"));
        assert_eq!(find_similar_name("length", &candidates), Some("len"));
        assert_eq!(find_similar_name("xyz", &candidates), None);
    }
    
    #[test]
    fn test_undefined_name_hints() {
        let hints = undefined_name_hints("true", &[]);
        assert!(hints.iter().any(|h| h.contains("True")));
        
        let hints = undefined_name_hints("null", &[]);
        assert!(hints.iter().any(|h| h.contains("None")));
    }
    
    #[test]
    fn test_type_mismatch_hints() {
        let hints = type_mismatch_hints("int", "float");
        assert!(hints.iter().any(|h| h.contains("int()")));
        
        let hints = type_mismatch_hints("str", "int");
        assert!(hints.iter().any(|h| h.contains("str()")));
    }
}
