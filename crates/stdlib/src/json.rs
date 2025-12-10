//! JSON parsing and serialization.
//!
//! Simple JSON support without external dependencies.

use std::collections::HashMap;
use std::fmt;

/// JSON value type.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    
    /// Check if boolean.
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }
    
    /// Check if number.
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if string.
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    
    /// Check if array.
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }
    
    /// Check if object.
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }
    
    /// Get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    
    /// Get as number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Get as i64.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) => Some(*n as i64),
            _ => None,
        }
    }
    
    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    
    /// Get as array.
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    
    /// Get as object.
    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
    
    /// Get value by key (for objects).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(o) => o.get(key),
            _ => None,
        }
    }
    
    /// Get value by index (for arrays).
    pub fn get_index(&self, index: usize) -> Option<&Value> {
        match self {
            Value::Array(a) => a.get(index),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", stringify(self))
    }
}

/// Parse JSON string.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    let mut parser = Parser::new(input);
    parser.parse_value()
}

/// Stringify a JSON value.
pub fn stringify(value: &Value) -> String {
    stringify_with_indent(value, 0, false)
}

/// Stringify with pretty printing.
pub fn stringify_pretty(value: &Value) -> String {
    stringify_with_indent(value, 0, true)
}

fn stringify_with_indent(value: &Value, indent: usize, pretty: bool) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => format!("\"{}\"", escape_string(s)),
        Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
            
            if pretty {
                let items: Vec<String> = arr.iter()
                    .map(|v| stringify_with_indent(v, indent + 2, true))
                    .collect();
                let spaces = " ".repeat(indent + 2);
                let end_spaces = " ".repeat(indent);
                format!("[\n{}{}\n{}]", spaces, items.join(&format!(",\n{}", spaces)), end_spaces)
            } else {
                let items: Vec<String> = arr.iter()
                    .map(|v| stringify_with_indent(v, 0, false))
                    .collect();
                format!("[{}]", items.join(","))
            }
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                return "{}".to_string();
            }
            
            if pretty {
                let items: Vec<String> = obj.iter()
                    .map(|(k, v)| {
                        format!("\"{}\": {}", escape_string(k), stringify_with_indent(v, indent + 2, true))
                    })
                    .collect();
                let spaces = " ".repeat(indent + 2);
                let end_spaces = " ".repeat(indent);
                format!("{{\n{}{}\n{}}}", spaces, items.join(&format!(",\n{}", spaces)), end_spaces)
            } else {
                let items: Vec<String> = obj.iter()
                    .map(|(k, v)| format!("\"{}\":{}", escape_string(k), stringify_with_indent(v, 0, false)))
                    .collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Parse error.
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON parse error at {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }
    
    fn parse_value(&mut self) -> Result<Value, ParseError> {
        self.skip_whitespace();
        
        match self.peek() {
            Some('n') => self.parse_null(),
            Some('t') | Some('f') => self.parse_bool(),
            Some('"') => self.parse_string(),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(self.error(&format!("Unexpected character: {}", c))),
            None => Err(self.error("Unexpected end of input")),
        }
    }
    
    fn parse_null(&mut self) -> Result<Value, ParseError> {
        self.expect("null")?;
        Ok(Value::Null)
    }
    
    fn parse_bool(&mut self) -> Result<Value, ParseError> {
        if self.try_consume("true") {
            Ok(Value::Bool(true))
        } else if self.try_consume("false") {
            Ok(Value::Bool(false))
        } else {
            Err(self.error("Expected 'true' or 'false'"))
        }
    }
    
    fn parse_string(&mut self) -> Result<Value, ParseError> {
        self.expect("\"")?;
        
        let mut s = String::new();
        
        loop {
            match self.next() {
                Some('"') => break,
                Some('\\') => {
                    match self.next() {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some('/') => s.push('/'),
                        Some('b') => s.push('\x08'),
                        Some('f') => s.push('\x0c'),
                        Some('n') => s.push('\n'),
                        Some('r') => s.push('\r'),
                        Some('t') => s.push('\t'),
                        Some('u') => {
                            let hex: String = (0..4)
                                .filter_map(|_| self.next())
                                .collect();
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(code) {
                                    s.push(c);
                                }
                            }
                        }
                        _ => return Err(self.error("Invalid escape sequence")),
                    }
                }
                Some(c) => s.push(c),
                None => return Err(self.error("Unterminated string")),
            }
        }
        
        Ok(Value::String(s))
    }
    
    fn parse_number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        
        // Optional minus
        if self.peek() == Some('-') {
            self.next();
        }
        
        // Integer part
        if self.peek() == Some('0') {
            self.next();
        } else {
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.next();
            }
        }
        
        // Fraction
        if self.peek() == Some('.') {
            self.next();
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.next();
            }
        }
        
        // Exponent
        if self.peek() == Some('e') || self.peek() == Some('E') {
            self.next();
            if self.peek() == Some('+') || self.peek() == Some('-') {
                self.next();
            }
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.next();
            }
        }
        
        let num_str = &self.input[start..self.pos];
        num_str.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| self.error("Invalid number"))
    }
    
    fn parse_array(&mut self) -> Result<Value, ParseError> {
        self.expect("[")?;
        self.skip_whitespace();
        
        let mut arr = Vec::new();
        
        if self.peek() == Some(']') {
            self.next();
            return Ok(Value::Array(arr));
        }
        
        loop {
            arr.push(self.parse_value()?);
            self.skip_whitespace();
            
            match self.peek() {
                Some(',') => {
                    self.next();
                    self.skip_whitespace();
                }
                Some(']') => {
                    self.next();
                    break;
                }
                _ => return Err(self.error("Expected ',' or ']'")),
            }
        }
        
        Ok(Value::Array(arr))
    }
    
    fn parse_object(&mut self) -> Result<Value, ParseError> {
        self.expect("{")?;
        self.skip_whitespace();
        
        let mut obj = HashMap::new();
        
        if self.peek() == Some('}') {
            self.next();
            return Ok(Value::Object(obj));
        }
        
        loop {
            self.skip_whitespace();
            
            let key = match self.parse_value()? {
                Value::String(s) => s,
                _ => return Err(self.error("Object key must be a string")),
            };
            
            self.skip_whitespace();
            self.expect(":")?;
            
            let value = self.parse_value()?;
            obj.insert(key, value);
            
            self.skip_whitespace();
            
            match self.peek() {
                Some(',') => {
                    self.next();
                }
                Some('}') => {
                    self.next();
                    break;
                }
                _ => return Err(self.error("Expected ',' or '}'")),
            }
        }
        
        Ok(Value::Object(obj))
    }
    
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    
    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    
    fn skip_whitespace(&mut self) {
        while self.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            self.next();
        }
    }
    
    fn expect(&mut self, s: &str) -> Result<(), ParseError> {
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            Ok(())
        } else {
            Err(self.error(&format!("Expected '{}'", s)))
        }
    }
    
    fn try_consume(&mut self, s: &str) -> bool {
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }
    
    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            position: self.pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_primitives() {
        assert_eq!(parse("null").unwrap(), Value::Null);
        assert_eq!(parse("true").unwrap(), Value::Bool(true));
        assert_eq!(parse("false").unwrap(), Value::Bool(false));
        assert_eq!(parse("42").unwrap(), Value::Number(42.0));
        assert_eq!(parse("\"hello\"").unwrap(), Value::String("hello".to_string()));
    }
    
    #[test]
    fn test_parse_array() {
        let value = parse("[1, 2, 3]").unwrap();
        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 3);
    }
    
    #[test]
    fn test_parse_object() {
        let value = parse(r#"{"name": "test", "value": 42}"#).unwrap();
        assert!(value.is_object());
        assert_eq!(value.get("name").unwrap().as_str(), Some("test"));
    }
    
    #[test]
    fn test_stringify() {
        let value = Value::Object(HashMap::from([
            ("name".to_string(), Value::String("test".to_string())),
            ("value".to_string(), Value::Number(42.0)),
        ]));
        
        let s = stringify(&value);
        let parsed = parse(&s).unwrap();
        assert_eq!(parsed.get("name").unwrap().as_str(), Some("test"));
    }
}

