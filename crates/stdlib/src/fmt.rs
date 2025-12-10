//! String formatting utilities.

use std::collections::HashMap;

/// Format a string with positional arguments.
/// Uses {} for placeholders.
pub fn format(template: &str, args: &[&str]) -> String {
    let mut result = String::new();
    let mut arg_index = 0;
    let mut chars = template.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                // Escaped {{
                chars.next();
                result.push('{');
            } else if chars.peek() == Some(&'}') {
                // {}
                chars.next();
                if arg_index < args.len() {
                    result.push_str(args[arg_index]);
                    arg_index += 1;
                } else {
                    result.push_str("{}");
                }
            } else {
                // {n} - indexed
                let mut num = String::new();
                while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
                    num.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&'}') {
                    chars.next();
                    if let Ok(idx) = num.parse::<usize>() {
                        if idx < args.len() {
                            result.push_str(args[idx]);
                        }
                    }
                } else {
                    result.push('{');
                    result.push_str(&num);
                }
            }
        } else if c == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
                result.push('}');
            } else {
                result.push('}');
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Format with named arguments.
pub fn format_map(template: &str, args: &HashMap<&str, &str>) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                result.push('{');
            } else {
                let mut name = String::new();
                while chars.peek().map_or(false, |&c| c != '}') {
                    name.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&'}') {
                    chars.next();
                    if let Some(value) = args.get(name.as_str()) {
                        result.push_str(value);
                    } else {
                        result.push('{');
                        result.push_str(&name);
                        result.push('}');
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Pad a string on the left.
pub fn pad_left(s: &str, width: usize, fill: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let padding: String = std::iter::repeat(fill).take(width - s.len()).collect();
        format!("{}{}", padding, s)
    }
}

/// Pad a string on the right.
pub fn pad_right(s: &str, width: usize, fill: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let padding: String = std::iter::repeat(fill).take(width - s.len()).collect();
        format!("{}{}", s, padding)
    }
}

/// Center a string.
pub fn center(s: &str, width: usize, fill: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let total_padding = width - s.len();
        let left_padding = total_padding / 2;
        let right_padding = total_padding - left_padding;
        let left: String = std::iter::repeat(fill).take(left_padding).collect();
        let right: String = std::iter::repeat(fill).take(right_padding).collect();
        format!("{}{}{}", left, s, right)
    }
}

/// Truncate a string.
pub fn truncate(s: &str, max_len: usize, suffix: &str) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncate_at = max_len.saturating_sub(suffix.len());
        format!("{}{}", &s[..truncate_at], suffix)
    }
}

/// Word wrap text.
pub fn wrap(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut line_len = 0;
    
    for word in text.split_whitespace() {
        if line_len > 0 && line_len + 1 + word.len() > width {
            result.push('\n');
            line_len = 0;
        } else if line_len > 0 {
            result.push(' ');
            line_len += 1;
        }
        result.push_str(word);
        line_len += word.len();
    }
    
    result
}

/// Indent text.
pub fn indent(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{}{}", prefix, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Dedent text (remove common leading whitespace).
pub fn dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    
    let min_indent = lines.iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    
    lines.iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format number with thousands separator.
pub fn format_number(n: i64, sep: char) -> String {
    let negative = n < 0;
    let mut s = n.abs().to_string();
    let mut result = String::new();
    
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(sep);
        }
        result.push(c);
    }
    
    if negative {
        result.push('-');
    }
    
    result.chars().rev().collect()
}

/// Format bytes as human-readable size.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    
    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Format duration as human-readable.
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format() {
        assert_eq!(format("Hello, {}!", &["World"]), "Hello, World!");
        assert_eq!(format("{} + {} = {}", &["1", "2", "3"]), "1 + 2 = 3");
        assert_eq!(format("{1} before {0}", &["B", "A"]), "A before B");
    }
    
    #[test]
    fn test_format_map() {
        let mut args = HashMap::new();
        args.insert("name", "World");
        assert_eq!(format_map("Hello, {name}!", &args), "Hello, World!");
    }
    
    #[test]
    fn test_pad() {
        assert_eq!(pad_left("42", 5, '0'), "00042");
        assert_eq!(pad_right("hi", 5, '-'), "hi---");
        assert_eq!(center("hi", 6, '*'), "**hi**");
    }
    
    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1000, ','), "1,000");
        assert_eq!(format_number(1000000, ','), "1,000,000");
        assert_eq!(format_number(-1234, ','), "-1,234");
    }
    
    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(100), "100 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
    }
}

