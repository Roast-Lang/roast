//! Additional built-in functions and types.

use roast_runtime::Value;
use std::sync::Arc;

/// Built-in exception types.
#[derive(Clone, Debug)]
pub enum Exception {
    BaseException(String),
    Exception(String),
    TypeError(String),
    ValueError(String),
    KeyError(String),
    IndexError(String),
    AttributeError(String),
    NameError(String),
    RuntimeError(String),
    StopIteration,
    GeneratorExit,
    AssertionError(String),
    ImportError(String),
    OSError(String),
    FileNotFoundError(String),
    PermissionError(String),
    ZeroDivisionError,
    OverflowError,
    MemoryError,
    RecursionError,
    NotImplementedError(String),
    SyntaxError(String),
    IndentationError(String),
    TabError(String),
}

impl Exception {
    pub fn message(&self) -> &str {
        match self {
            Exception::BaseException(m) => m,
            Exception::Exception(m) => m,
            Exception::TypeError(m) => m,
            Exception::ValueError(m) => m,
            Exception::KeyError(m) => m,
            Exception::IndexError(m) => m,
            Exception::AttributeError(m) => m,
            Exception::NameError(m) => m,
            Exception::RuntimeError(m) => m,
            Exception::StopIteration => "",
            Exception::GeneratorExit => "",
            Exception::AssertionError(m) => m,
            Exception::ImportError(m) => m,
            Exception::OSError(m) => m,
            Exception::FileNotFoundError(m) => m,
            Exception::PermissionError(m) => m,
            Exception::ZeroDivisionError => "division by zero",
            Exception::OverflowError => "integer overflow",
            Exception::MemoryError => "out of memory",
            Exception::RecursionError => "maximum recursion depth exceeded",
            Exception::NotImplementedError(m) => m,
            Exception::SyntaxError(m) => m,
            Exception::IndentationError(m) => m,
            Exception::TabError(m) => m,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Exception::BaseException(_) => "BaseException",
            Exception::Exception(_) => "Exception",
            Exception::TypeError(_) => "TypeError",
            Exception::ValueError(_) => "ValueError",
            Exception::KeyError(_) => "KeyError",
            Exception::IndexError(_) => "IndexError",
            Exception::AttributeError(_) => "AttributeError",
            Exception::NameError(_) => "NameError",
            Exception::RuntimeError(_) => "RuntimeError",
            Exception::StopIteration => "StopIteration",
            Exception::GeneratorExit => "GeneratorExit",
            Exception::AssertionError(_) => "AssertionError",
            Exception::ImportError(_) => "ImportError",
            Exception::OSError(_) => "OSError",
            Exception::FileNotFoundError(_) => "FileNotFoundError",
            Exception::PermissionError(_) => "PermissionError",
            Exception::ZeroDivisionError => "ZeroDivisionError",
            Exception::OverflowError => "OverflowError",
            Exception::MemoryError => "MemoryError",
            Exception::RecursionError => "RecursionError",
            Exception::NotImplementedError(_) => "NotImplementedError",
            Exception::SyntaxError(_) => "SyntaxError",
            Exception::IndentationError(_) => "IndentationError",
            Exception::TabError(_) => "TabError",
        }
    }
}

impl std::fmt::Display for Exception {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = self.message();
        if msg.is_empty() {
            write!(f, "{}", self.name())
        } else {
            write!(f, "{}: {}", self.name(), msg)
        }
    }
}

/// Slice object for sequence slicing.
#[derive(Clone, Debug)]
pub struct Slice {
    pub start: Option<i64>,
    pub stop: Option<i64>,
    pub step: Option<i64>,
}

impl Slice {
    pub fn new(start: Option<i64>, stop: Option<i64>, step: Option<i64>) -> Self {
        Self { start, stop, step }
    }

    /// Computes the actual indices for a sequence of the given length.
    pub fn indices(&self, length: usize) -> (usize, usize, i64) {
        let len = length as i64;
        let step = self.step.unwrap_or(1);
        
        let (start, stop) = if step > 0 {
            let start = self.start.map(|s| {
                if s < 0 { (len + s).max(0) } else { s.min(len) }
            }).unwrap_or(0);
            let stop = self.stop.map(|s| {
                if s < 0 { (len + s).max(0) } else { s.min(len) }
            }).unwrap_or(len);
            (start, stop)
        } else {
            let start = self.start.map(|s| {
                if s < 0 { (len + s).max(-1) } else { s.min(len - 1) }
            }).unwrap_or(len - 1);
            let stop = self.stop.map(|s| {
                if s < 0 { (len + s).max(-1) } else { s.min(len) }
            }).unwrap_or(-1);
            (start, stop)
        };

        (start.max(0) as usize, stop.max(0) as usize, step)
    }

    /// Applies the slice to a vector.
    pub fn apply<T: Clone>(&self, items: &[T]) -> Vec<T> {
        let (start, stop, step) = self.indices(items.len());
        
        if step > 0 {
            let mut result = Vec::new();
            let mut i = start;
            while i < stop {
                if let Some(item) = items.get(i) {
                    result.push(item.clone());
                }
                i = (i as i64 + step) as usize;
            }
            result
        } else {
            let mut result = Vec::new();
            let mut i = start as i64;
            while i > stop as i64 {
                if let Some(item) = items.get(i as usize) {
                    result.push(item.clone());
                }
                i += step;
            }
            result
        }
    }
}

/// Range iterator.
#[derive(Clone, Debug)]
pub struct RangeIter {
    current: i64,
    stop: i64,
    step: i64,
}

impl RangeIter {
    pub fn new(start: i64, stop: i64, step: i64) -> Self {
        Self {
            current: start,
            stop,
            step,
        }
    }
}

impl Iterator for RangeIter {
    type Item = i64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.step > 0 {
            if self.current < self.stop {
                let value = self.current;
                self.current += self.step;
                Some(value)
            } else {
                None
            }
        } else {
            if self.current > self.stop {
                let value = self.current;
                self.current += self.step;
                Some(value)
            } else {
                None
            }
        }
    }
}

/// Enumerate iterator.
pub struct EnumerateIter<I> {
    iter: I,
    count: i64,
}

impl<I: Iterator> EnumerateIter<I> {
    pub fn new(iter: I, start: i64) -> Self {
        Self { iter, count: start }
    }
}

impl<I: Iterator> Iterator for EnumerateIter<I> {
    type Item = (i64, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item| {
            let idx = self.count;
            self.count += 1;
            (idx, item)
        })
    }
}

/// Zip iterator.
pub struct ZipIter<A, B> {
    a: A,
    b: B,
}

impl<A: Iterator, B: Iterator> ZipIter<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: Iterator, B: Iterator> Iterator for ZipIter<A, B> {
    type Item = (A::Item, B::Item);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.a.next(), self.b.next()) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    }
}

/// Map iterator.
pub struct MapIter<I, F> {
    iter: I,
    func: F,
}

impl<I: Iterator, F: FnMut(I::Item) -> Value> MapIter<I, F> {
    pub fn new(iter: I, func: F) -> Self {
        Self { iter, func }
    }
}

impl<I: Iterator, F: FnMut(I::Item) -> Value> Iterator for MapIter<I, F> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item| (self.func)(item))
    }
}

/// Filter iterator.
pub struct FilterIter<I, F> {
    iter: I,
    pred: F,
}

impl<I: Iterator, F: FnMut(&I::Item) -> bool> FilterIter<I, F> {
    pub fn new(iter: I, pred: F) -> Self {
        Self { iter, pred }
    }
}

impl<I: Iterator, F: FnMut(&I::Item) -> bool> Iterator for FilterIter<I, F> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(item) if (self.pred)(&item) => return Some(item),
                Some(_) => continue,
                None => return None,
            }
        }
    }
}

/// String formatting utilities.
pub mod formatting {
    use super::*;

    /// Formats a value using Python-style format specification.
    pub fn format_value(value: &Value, spec: &str) -> String {
        if spec.is_empty() {
            return format!("{:?}", value);
        }

        // Parse format spec: [[fill]align][sign][#][0][width][,][.precision][type]
        let mut chars = spec.chars().peekable();
        
        let mut fill = ' ';
        let mut align = None;
        let mut width = 0usize;
        let mut precision = None;
        let mut type_char = None;

        // Check for fill and align
        if chars.clone().count() >= 2 {
            let first = chars.clone().next();
            let second = chars.clone().nth(1);
            if matches!(second, Some('<') | Some('>') | Some('^') | Some('=')) {
                fill = first.unwrap();
                chars.next();
                align = chars.next();
            } else if matches!(first, Some('<') | Some('>') | Some('^') | Some('=')) {
                align = chars.next();
            }
        }

        // Parse width
        let mut width_str = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                width_str.push(chars.next().unwrap());
            } else {
                break;
            }
        }
        if !width_str.is_empty() {
            width = width_str.parse().unwrap_or(0);
        }

        // Parse precision
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut prec_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    prec_str.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            precision = Some(prec_str.parse().unwrap_or(6));
        }

        // Parse type
        type_char = chars.next();

        // Format based on type
        let formatted = match (value, type_char) {
            (Value::Int(n), Some('d') | None) => format!("{}", n),
            (Value::Int(n), Some('x')) => format!("{:x}", n),
            (Value::Int(n), Some('X')) => format!("{:X}", n),
            (Value::Int(n), Some('o')) => format!("{:o}", n),
            (Value::Int(n), Some('b')) => format!("{:b}", n),
            (Value::Float(f), Some('f') | Some('F') | None) => {
                let prec = precision.unwrap_or(6);
                format!("{:.prec$}", f, prec = prec)
            }
            (Value::Float(f), Some('e')) => {
                let prec = precision.unwrap_or(6);
                format!("{:.prec$e}", f, prec = prec)
            }
            (Value::Float(f), Some('E')) => {
                let prec = precision.unwrap_or(6);
                format!("{:.prec$E}", f, prec = prec)
            }
            (Value::Float(f), Some('%')) => {
                let prec = precision.unwrap_or(6);
                format!("{:.prec$}%", f * 100.0, prec = prec)
            }
            (Value::Str(s), _) => {
                if let Some(prec) = precision {
                    s.chars().take(prec).collect()
                } else {
                    s.to_string()
                }
            }
            _ => format!("{:?}", value),
        };

        // Apply width and alignment
        if width > formatted.len() {
            let padding = width - formatted.len();
            match align {
                Some('<') => format!("{}{}", formatted, fill.to_string().repeat(padding)),
                Some('>') | None => format!("{}{}", fill.to_string().repeat(padding), formatted),
                Some('^') => {
                    let left = padding / 2;
                    let right = padding - left;
                    format!(
                        "{}{}{}",
                        fill.to_string().repeat(left),
                        formatted,
                        fill.to_string().repeat(right)
                    )
                }
                _ => formatted,
            }
        } else {
            formatted
        }
    }

    /// Parses an f-string and evaluates it.
    pub fn format_string(template: &str, values: &[(&str, Value)]) -> String {
        let mut result = String::new();
        let mut chars = template.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '{' {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    result.push('{');
                    continue;
                }
                
                // Parse field name
                let mut field = String::new();
                let mut spec = String::new();
                let mut in_spec = false;
                
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next();
                        break;
                    }
                    if c == ':' && !in_spec {
                        in_spec = true;
                        chars.next();
                        continue;
                    }
                    if in_spec {
                        spec.push(chars.next().unwrap());
                    } else {
                        field.push(chars.next().unwrap());
                    }
                }
                
                // Look up value
                if let Some((_, value)) = values.iter().find(|(name, _)| *name == field) {
                    result.push_str(&format_value(value, &spec));
                } else if let Ok(idx) = field.parse::<usize>() {
                    if let Some((_, value)) = values.get(idx) {
                        result.push_str(&format_value(value, &spec));
                    }
                }
            } else if c == '}' {
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                result.push('}');
            } else {
                result.push(c);
            }
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_indices() {
        let slice = Slice::new(Some(1), Some(4), None);
        assert_eq!(slice.indices(10), (1, 4, 1));
        
        let slice = Slice::new(Some(-3), None, None);
        assert_eq!(slice.indices(10), (7, 10, 1));
    }

    #[test]
    fn test_slice_apply() {
        let items = vec![0, 1, 2, 3, 4, 5];
        
        let slice = Slice::new(Some(1), Some(4), None);
        assert_eq!(slice.apply(&items), vec![1, 2, 3]);
        
        let slice = Slice::new(None, None, Some(2));
        assert_eq!(slice.apply(&items), vec![0, 2, 4]);
    }

    #[test]
    fn test_range_iter() {
        let mut iter = RangeIter::new(0, 5, 1);
        assert_eq!(iter.collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
        
        let mut iter = RangeIter::new(0, 10, 2);
        assert_eq!(iter.collect::<Vec<_>>(), vec![0, 2, 4, 6, 8]);
    }

    #[test]
    fn test_format_value() {
        use formatting::format_value;
        
        assert_eq!(format_value(&Value::Int(42), "d"), "42");
        assert_eq!(format_value(&Value::Int(255), "x"), "ff");
        assert_eq!(format_value(&Value::Float(3.14159), ".2f"), "3.14");
    }
}

