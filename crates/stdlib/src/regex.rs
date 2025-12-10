//! Simple pattern matching (regex-like).
//!
//! A basic pattern matcher supporting common patterns.

/// Pattern matching result.
#[derive(Clone, Debug)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Match {
    /// Get the matched text.
    pub fn as_str(&self) -> &str {
        &self.text
    }
    
    /// Get the length of the match.
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    
    /// Check if match is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Simple pattern types.
#[derive(Clone, Debug)]
pub enum Pattern {
    /// Literal string.
    Literal(String),
    /// Match any single character.
    Any,
    /// Match one of the characters.
    CharClass(Vec<char>),
    /// Match anything except these characters.
    NegCharClass(Vec<char>),
    /// Match word character (alphanumeric + _).
    Word,
    /// Match digit.
    Digit,
    /// Match whitespace.
    Whitespace,
    /// Match start of string.
    Start,
    /// Match end of string.
    End,
    /// Sequence of patterns.
    Sequence(Vec<Pattern>),
    /// Either pattern.
    Alternate(Box<Pattern>, Box<Pattern>),
    /// Zero or more.
    Star(Box<Pattern>),
    /// One or more.
    Plus(Box<Pattern>),
    /// Zero or one.
    Optional(Box<Pattern>),
    /// Capture group.
    Group(Box<Pattern>),
}

/// Compile a simple pattern string.
/// Supported syntax:
/// - `.` - any character
/// - `\d` - digit
/// - `\w` - word character
/// - `\s` - whitespace
/// - `*` - zero or more (previous)
/// - `+` - one or more (previous)
/// - `?` - optional (previous)
/// - `^` - start of string
/// - `$` - end of string
/// - `[abc]` - character class
/// - `[^abc]` - negated character class
/// - `(...)` - group
/// - `|` - alternation
pub fn compile(pattern: &str) -> Result<Pattern, CompileError> {
    let mut parser = PatternParser::new(pattern);
    parser.parse()
}

/// Check if pattern matches entire string.
pub fn matches(pattern: &str, text: &str) -> bool {
    if let Ok(p) = compile(pattern) {
        match_at(&p, text, 0).map_or(false, |end| end == text.len())
    } else {
        false
    }
}

/// Find first match.
pub fn find(pattern: &str, text: &str) -> Option<Match> {
    let p = compile(pattern).ok()?;
    
    for start in 0..text.len() {
        if let Some(end) = match_at(&p, text, start) {
            return Some(Match {
                start,
                end,
                text: text[start..end].to_string(),
            });
        }
    }
    
    None
}

/// Find all matches.
pub fn find_all(pattern: &str, text: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    
    if let Ok(p) = compile(pattern) {
        let mut pos = 0;
        while pos < text.len() {
            if let Some(end) = match_at(&p, text, pos) {
                if end > pos {
                    matches.push(Match {
                        start: pos,
                        end,
                        text: text[pos..end].to_string(),
                    });
                    pos = end;
                } else {
                    pos += 1;
                }
            } else {
                pos += 1;
            }
        }
    }
    
    matches
}

/// Replace all matches.
pub fn replace_all(pattern: &str, text: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut last_end = 0;
    
    for m in find_all(pattern, text) {
        result.push_str(&text[last_end..m.start]);
        result.push_str(replacement);
        last_end = m.end;
    }
    
    result.push_str(&text[last_end..]);
    result
}

/// Split by pattern.
pub fn split(pattern: &str, text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut last_end = 0;
    
    for m in find_all(pattern, text) {
        result.push(text[last_end..m.start].to_string());
        last_end = m.end;
    }
    
    result.push(text[last_end..].to_string());
    result
}

fn match_at(pattern: &Pattern, text: &str, pos: usize) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    
    match pattern {
        Pattern::Literal(s) => {
            if text[pos..].starts_with(s) {
                Some(pos + s.len())
            } else {
                None
            }
        }
        Pattern::Any => {
            if pos < chars.len() {
                Some(pos + chars[pos].len_utf8())
            } else {
                None
            }
        }
        Pattern::CharClass(chars_set) => {
            if pos < chars.len() && chars_set.contains(&chars[pos]) {
                Some(pos + chars[pos].len_utf8())
            } else {
                None
            }
        }
        Pattern::NegCharClass(chars_set) => {
            if pos < chars.len() && !chars_set.contains(&chars[pos]) {
                Some(pos + chars[pos].len_utf8())
            } else {
                None
            }
        }
        Pattern::Word => {
            if pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                Some(pos + chars[pos].len_utf8())
            } else {
                None
            }
        }
        Pattern::Digit => {
            if pos < chars.len() && chars[pos].is_ascii_digit() {
                Some(pos + chars[pos].len_utf8())
            } else {
                None
            }
        }
        Pattern::Whitespace => {
            if pos < chars.len() && chars[pos].is_whitespace() {
                Some(pos + chars[pos].len_utf8())
            } else {
                None
            }
        }
        Pattern::Start => {
            if pos == 0 { Some(pos) } else { None }
        }
        Pattern::End => {
            if pos == text.len() { Some(pos) } else { None }
        }
        Pattern::Sequence(patterns) => {
            let mut current = pos;
            for p in patterns {
                current = match_at(p, text, current)?;
            }
            Some(current)
        }
        Pattern::Alternate(a, b) => {
            match_at(a, text, pos).or_else(|| match_at(b, text, pos))
        }
        Pattern::Star(p) => {
            let mut current = pos;
            while let Some(next) = match_at(p, text, current) {
                if next == current {
                    break; // Prevent infinite loop
                }
                current = next;
            }
            Some(current)
        }
        Pattern::Plus(p) => {
            let first = match_at(p, text, pos)?;
            let mut current = first;
            while let Some(next) = match_at(p, text, current) {
                if next == current {
                    break;
                }
                current = next;
            }
            Some(current)
        }
        Pattern::Optional(p) => {
            match_at(p, text, pos).or(Some(pos))
        }
        Pattern::Group(p) => {
            match_at(p, text, pos)
        }
    }
}

/// Compile error.
#[derive(Debug)]
pub struct CompileError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Regex compile error at {}: {}", self.position, self.message)
    }
}

impl std::error::Error for CompileError {}

struct PatternParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> PatternParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }
    
    fn parse(&mut self) -> Result<Pattern, CompileError> {
        self.parse_alternate()
    }
    
    fn parse_alternate(&mut self) -> Result<Pattern, CompileError> {
        let left = self.parse_sequence()?;
        
        if self.peek() == Some('|') {
            self.next();
            let right = self.parse_alternate()?;
            Ok(Pattern::Alternate(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }
    
    fn parse_sequence(&mut self) -> Result<Pattern, CompileError> {
        let mut patterns = Vec::new();
        
        while let Some(p) = self.parse_quantified()? {
            patterns.push(p);
        }
        
        if patterns.is_empty() {
            Ok(Pattern::Literal(String::new()))
        } else if patterns.len() == 1 {
            Ok(patterns.pop().unwrap())
        } else {
            Ok(Pattern::Sequence(patterns))
        }
    }
    
    fn parse_quantified(&mut self) -> Result<Option<Pattern>, CompileError> {
        let atom = match self.parse_atom()? {
            Some(a) => a,
            None => return Ok(None),
        };
        
        Ok(Some(match self.peek() {
            Some('*') => {
                self.next();
                Pattern::Star(Box::new(atom))
            }
            Some('+') => {
                self.next();
                Pattern::Plus(Box::new(atom))
            }
            Some('?') => {
                self.next();
                Pattern::Optional(Box::new(atom))
            }
            _ => atom,
        }))
    }
    
    fn parse_atom(&mut self) -> Result<Option<Pattern>, CompileError> {
        match self.peek() {
            None | Some('|') | Some(')') => Ok(None),
            Some('^') => {
                self.next();
                Ok(Some(Pattern::Start))
            }
            Some('$') => {
                self.next();
                Ok(Some(Pattern::End))
            }
            Some('.') => {
                self.next();
                Ok(Some(Pattern::Any))
            }
            Some('\\') => {
                self.next();
                match self.next() {
                    Some('d') => Ok(Some(Pattern::Digit)),
                    Some('w') => Ok(Some(Pattern::Word)),
                    Some('s') => Ok(Some(Pattern::Whitespace)),
                    Some(c) => Ok(Some(Pattern::Literal(c.to_string()))),
                    None => Err(self.error("Incomplete escape")),
                }
            }
            Some('[') => self.parse_char_class(),
            Some('(') => {
                self.next();
                let inner = self.parse_alternate()?;
                if self.next() != Some(')') {
                    return Err(self.error("Unclosed group"));
                }
                Ok(Some(Pattern::Group(Box::new(inner))))
            }
            Some('*') | Some('+') | Some('?') => {
                Err(self.error("Quantifier without target"))
            }
            Some(c) => {
                self.next();
                Ok(Some(Pattern::Literal(c.to_string())))
            }
        }
    }
    
    fn parse_char_class(&mut self) -> Result<Option<Pattern>, CompileError> {
        self.next(); // [
        
        let negated = self.peek() == Some('^');
        if negated {
            self.next();
        }
        
        let mut chars = Vec::new();
        
        while self.peek() != Some(']') && self.peek().is_some() {
            if let Some(c) = self.next() {
                chars.push(c);
            }
        }
        
        if self.next() != Some(']') {
            return Err(self.error("Unclosed character class"));
        }
        
        if negated {
            Ok(Some(Pattern::NegCharClass(chars)))
        } else {
            Ok(Some(Pattern::CharClass(chars)))
        }
    }
    
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    
    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    
    fn error(&self, message: &str) -> CompileError {
        CompileError {
            message: message.to_string(),
            position: self.pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_literal() {
        assert!(matches("hello", "hello"));
        assert!(!matches("hello", "world"));
    }
    
    #[test]
    fn test_any() {
        assert!(matches("h.llo", "hello"));
        assert!(matches("h.llo", "hallo"));
    }
    
    #[test]
    fn test_digit() {
        assert!(matches(r"\d+", "123"));
        assert!(!matches(r"\d+", "abc"));
    }
    
    #[test]
    fn test_find_all() {
        let matches = find_all(r"\d+", "foo123bar456baz");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].text, "123");
        assert_eq!(matches[1].text, "456");
    }
    
    #[test]
    fn test_replace() {
        let result = replace_all(r"\d+", "foo123bar456", "X");
        assert_eq!(result, "fooXbarX");
    }
}

