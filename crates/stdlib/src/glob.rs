//! Glob pattern matching module for Roast.
//!
//! Provides Unix shell-style wildcards for path matching.

use std::path::{Path, PathBuf};
use std::fs;

// =============================================================================
// Pattern Matching
// =============================================================================

/// A compiled glob pattern.
#[derive(Clone, Debug)]
pub struct Pattern {
    tokens: Vec<Token>,
    original: String,
}

#[derive(Clone, Debug)]
enum Token {
    /// Literal text
    Literal(String),
    /// ? matches any single character
    Any,
    /// * matches any sequence (except /)
    Star,
    /// ** matches any sequence including /
    DoubleStar,
    /// [abc] character class
    CharClass(Vec<char>, bool), // chars, negated
    /// {a,b,c} alternation
    Alternation(Vec<String>),
}

impl Pattern {
    /// Compile a glob pattern.
    pub fn new(pattern: &str) -> Result<Self, GlobError> {
        let tokens = Self::parse(pattern)?;
        Ok(Self {
            tokens,
            original: pattern.to_string(),
        })
    }
    
    fn parse(pattern: &str) -> Result<Vec<Token>, GlobError> {
        let mut tokens = Vec::new();
        let mut chars = pattern.chars().peekable();
        let mut literal = String::new();
        
        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        tokens.push(Token::DoubleStar);
                    } else {
                        tokens.push(Token::Star);
                    }
                }
                '?' => {
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    tokens.push(Token::Any);
                }
                '[' => {
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    let (class, negated) = Self::parse_char_class(&mut chars)?;
                    tokens.push(Token::CharClass(class, negated));
                }
                '{' => {
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    let alternatives = Self::parse_alternation(&mut chars)?;
                    tokens.push(Token::Alternation(alternatives));
                }
                '\\' => {
                    // Escape next character
                    if let Some(next) = chars.next() {
                        literal.push(next);
                    }
                }
                _ => {
                    literal.push(c);
                }
            }
        }
        
        if !literal.is_empty() {
            tokens.push(Token::Literal(literal));
        }
        
        Ok(tokens)
    }
    
    fn parse_char_class(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<(Vec<char>, bool), GlobError> {
        let mut class = Vec::new();
        let negated = chars.peek() == Some(&'!') || chars.peek() == Some(&'^');
        if negated {
            chars.next();
        }
        
        while let Some(c) = chars.next() {
            match c {
                ']' => return Ok((class, negated)),
                '-' if !class.is_empty() && chars.peek() != Some(&']') => {
                    if let (Some(&start), Some(end)) = (class.last(), chars.next()) {
                        for ch in (start as u8 + 1)..=(end as u8) {
                            class.push(ch as char);
                        }
                    }
                }
                _ => class.push(c),
            }
        }
        
        Err(GlobError::InvalidPattern("Unclosed character class".to_string()))
    }
    
    fn parse_alternation(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<Vec<String>, GlobError> {
        let mut alternatives = Vec::new();
        let mut current = String::new();
        let mut depth = 1;
        
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    depth += 1;
                    current.push(c);
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        alternatives.push(current);
                        return Ok(alternatives);
                    }
                    current.push(c);
                }
                ',' if depth == 1 => {
                    alternatives.push(std::mem::take(&mut current));
                }
                _ => current.push(c),
            }
        }
        
        Err(GlobError::InvalidPattern("Unclosed alternation".to_string()))
    }
    
    /// Check if a path matches this pattern.
    pub fn matches(&self, path: &str) -> bool {
        self.matches_tokens(&self.tokens, path)
    }
    
    fn matches_tokens(&self, tokens: &[Token], text: &str) -> bool {
        if tokens.is_empty() {
            return text.is_empty();
        }
        
        match &tokens[0] {
            Token::Literal(lit) => {
                if text.starts_with(lit) {
                    self.matches_tokens(&tokens[1..], &text[lit.len()..])
                } else {
                    false
                }
            }
            Token::Any => {
                if text.is_empty() {
                    false
                } else {
                    let mut chars = text.chars();
                    let c = chars.next().unwrap();
                    if c != '/' && c != '\\' {
                        self.matches_tokens(&tokens[1..], chars.as_str())
                    } else {
                        false
                    }
                }
            }
            Token::Star => {
                // Try matching zero or more non-separator characters
                for (i, c) in text.char_indices() {
                    if c == '/' || c == '\\' {
                        return self.matches_tokens(&tokens[1..], &text[i..]);
                    }
                    if self.matches_tokens(&tokens[1..], &text[i..]) {
                        return true;
                    }
                }
                self.matches_tokens(&tokens[1..], "")
            }
            Token::DoubleStar => {
                // Try matching any number of path components
                for (i, _) in text.char_indices() {
                    if self.matches_tokens(&tokens[1..], &text[i..]) {
                        return true;
                    }
                }
                self.matches_tokens(&tokens[1..], "")
            }
            Token::CharClass(chars, negated) => {
                if text.is_empty() {
                    return false;
                }
                let c = text.chars().next().unwrap();
                let matches = chars.contains(&c);
                if matches != *negated {
                    self.matches_tokens(&tokens[1..], &text[c.len_utf8()..])
                } else {
                    false
                }
            }
            Token::Alternation(alts) => {
                for alt in alts {
                    if text.starts_with(alt) && self.matches_tokens(&tokens[1..], &text[alt.len()..]) {
                        return true;
                    }
                }
                false
            }
        }
    }
    
    /// Get the original pattern string.
    pub fn as_str(&self) -> &str {
        &self.original
    }
}

// =============================================================================
// Glob Functions
// =============================================================================

/// Error type for glob operations.
#[derive(Debug, Clone)]
pub enum GlobError {
    InvalidPattern(String),
    IoError(String),
}

impl std::fmt::Display for GlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlobError::InvalidPattern(msg) => write!(f, "Invalid pattern: {}", msg),
            GlobError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for GlobError {}

/// Find all paths matching a glob pattern.
pub fn glob(pattern: &str) -> Result<Vec<PathBuf>, GlobError> {
    let compiled = Pattern::new(pattern)?;
    glob_with_pattern(&compiled, ".")
}

/// Find paths matching a pattern in a specific directory.
pub fn glob_in(pattern: &str, base: &Path) -> Result<Vec<PathBuf>, GlobError> {
    let compiled = Pattern::new(pattern)?;
    glob_with_pattern(&compiled, base)
}

fn glob_with_pattern<P: AsRef<Path>>(pattern: &Pattern, base: P) -> Result<Vec<PathBuf>, GlobError> {
    let mut results = Vec::new();
    let base = base.as_ref();
    
    glob_recursive(pattern, base, &PathBuf::new(), &mut results)?;
    
    Ok(results)
}

fn glob_recursive(
    pattern: &Pattern,
    base: &Path,
    current: &Path,
    results: &mut Vec<PathBuf>,
) -> Result<(), GlobError> {
    let full_path = base.join(current);
    
    let entries = fs::read_dir(&full_path)
        .map_err(|e| GlobError::IoError(e.to_string()))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| GlobError::IoError(e.to_string()))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        // Skip hidden files unless pattern explicitly includes them
        if name_str.starts_with('.') && !pattern.as_str().starts_with('.') {
            continue;
        }
        
        let relative = current.join(&name);
        let relative_str = relative.to_string_lossy();
        
        if pattern.matches(&relative_str) {
            results.push(entry.path());
        }
        
        // Recurse into directories
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            glob_recursive(pattern, base, &relative, results)?;
        }
    }
    
    Ok(())
}

/// Iterate over paths matching a glob pattern.
pub fn iglob(pattern: &str) -> Result<GlobIterator, GlobError> {
    let paths = glob(pattern)?;
    Ok(GlobIterator {
        paths: paths.into_iter(),
    })
}

/// Iterator over glob matches.
pub struct GlobIterator {
    paths: std::vec::IntoIter<PathBuf>,
}

impl Iterator for GlobIterator {
    type Item = PathBuf;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.paths.next()
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Check if a string matches a glob pattern.
pub fn fnmatch(pattern: &str, string: &str) -> bool {
    Pattern::new(pattern)
        .map(|p| p.matches(string))
        .unwrap_or(false)
}

/// Filter a list of strings with a pattern.
pub fn filter<'a, I>(pattern: &str, strings: I) -> Vec<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    if let Ok(p) = Pattern::new(pattern) {
        strings.into_iter()
            .filter(|s| p.matches(s))
            .collect()
    } else {
        Vec::new()
    }
}

/// Escape special characters in a string.
pub fn escape(string: &str) -> String {
    let mut result = String::with_capacity(string.len() * 2);
    for c in string.chars() {
        match c {
            '*' | '?' | '[' | ']' | '{' | '}' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Check if a pattern has any special characters.
pub fn has_magic(pattern: &str) -> bool {
    pattern.chars().any(|c| matches!(c, '*' | '?' | '[' | '{'))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_literal() {
        let p = Pattern::new("hello").unwrap();
        assert!(p.matches("hello"));
        assert!(!p.matches("world"));
    }
    
    #[test]
    fn test_star() {
        let p = Pattern::new("*.txt").unwrap();
        assert!(p.matches("file.txt"));
        assert!(p.matches("hello.txt"));
        assert!(!p.matches("file.rs"));
    }
    
    #[test]
    fn test_question() {
        let p = Pattern::new("file?.txt").unwrap();
        assert!(p.matches("file1.txt"));
        assert!(p.matches("fileA.txt"));
        assert!(!p.matches("file10.txt"));
    }
    
    #[test]
    fn test_char_class() {
        let p = Pattern::new("[abc].txt").unwrap();
        assert!(p.matches("a.txt"));
        assert!(p.matches("b.txt"));
        assert!(!p.matches("d.txt"));
    }
    
    #[test]
    fn test_negated_class() {
        let p = Pattern::new("[!abc].txt").unwrap();
        assert!(!p.matches("a.txt"));
        assert!(p.matches("d.txt"));
    }
    
    #[test]
    fn test_alternation() {
        let p = Pattern::new("file.{txt,rs,py}").unwrap();
        assert!(p.matches("file.txt"));
        assert!(p.matches("file.rs"));
        assert!(p.matches("file.py"));
        assert!(!p.matches("file.js"));
    }
    
    #[test]
    fn test_fnmatch() {
        assert!(fnmatch("*.py", "test.py"));
        assert!(!fnmatch("*.py", "test.txt"));
    }
    
    #[test]
    fn test_escape() {
        assert_eq!(escape("file*.txt"), "file\\*.txt");
        assert_eq!(escape("normal"), "normal");
    }
    
    #[test]
    fn test_has_magic() {
        assert!(has_magic("*.txt"));
        assert!(has_magic("file?.txt"));
        assert!(!has_magic("normal.txt"));
    }
}

