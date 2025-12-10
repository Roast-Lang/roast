//! String operations.

/// Check if string starts with prefix.
pub fn startswith(s: &str, prefix: &str) -> bool {
    s.starts_with(prefix)
}

/// Check if string ends with suffix.
pub fn endswith(s: &str, suffix: &str) -> bool {
    s.ends_with(suffix)
}

/// Convert to uppercase.
pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// Convert to lowercase.
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// Capitalize first letter.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

/// Title case.
pub fn title(s: &str) -> String {
    s.split_whitespace()
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Swap case.
pub fn swapcase(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_uppercase() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                c.to_uppercase().next().unwrap_or(c)
            }
        })
        .collect()
}

/// Strip whitespace from both ends.
pub fn strip(s: &str) -> &str {
    s.trim()
}

/// Strip whitespace from left.
pub fn lstrip(s: &str) -> &str {
    s.trim_start()
}

/// Strip whitespace from right.
pub fn rstrip(s: &str) -> &str {
    s.trim_end()
}

/// Split string.
pub fn split<'a>(s: &'a str, sep: Option<&str>) -> Vec<&'a str> {
    match sep {
        Some(sep) => s.split(sep).collect(),
        None => s.split_whitespace().collect(),
    }
}

/// Split into lines.
pub fn splitlines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

/// Join strings.
pub fn join(sep: &str, items: &[&str]) -> String {
    items.join(sep)
}

/// Replace substring.
pub fn replace(s: &str, old: &str, new: &str) -> String {
    s.replace(old, new)
}

/// Find substring.
pub fn find(s: &str, sub: &str) -> Option<usize> {
    s.find(sub)
}

/// Reverse find substring.
pub fn rfind(s: &str, sub: &str) -> Option<usize> {
    s.rfind(sub)
}

/// Count occurrences.
pub fn count(s: &str, sub: &str) -> usize {
    s.matches(sub).count()
}

/// Check if alphabetic.
pub fn isalpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphabetic())
}

/// Check if alphanumeric.
pub fn isalnum(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric())
}

/// Check if digits.
pub fn isdigit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Check if whitespace.
pub fn isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_whitespace())
}

/// Check if uppercase.
pub fn isupper(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic()) && s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

/// Check if lowercase.
pub fn islower(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic()) && s.chars().all(|c| !c.is_alphabetic() || c.is_lowercase())
}

/// Center string.
pub fn center(s: &str, width: usize, fillchar: char) -> String {
    if s.len() >= width {
        return s.to_string();
    }
    let padding = width - s.len();
    let left = padding / 2;
    let right = padding - left;
    format!(
        "{}{}{}",
        fillchar.to_string().repeat(left),
        s,
        fillchar.to_string().repeat(right)
    )
}

/// Left-justify string.
pub fn ljust(s: &str, width: usize, fillchar: char) -> String {
    if s.len() >= width {
        return s.to_string();
    }
    format!("{}{}", s, fillchar.to_string().repeat(width - s.len()))
}

/// Right-justify string.
pub fn rjust(s: &str, width: usize, fillchar: char) -> String {
    if s.len() >= width {
        return s.to_string();
    }
    format!("{}{}", fillchar.to_string().repeat(width - s.len()), s)
}

/// Zero-fill string.
pub fn zfill(s: &str, width: usize) -> String {
    if s.len() >= width {
        return s.to_string();
    }
    if s.starts_with('-') || s.starts_with('+') {
        format!(
            "{}{}{}",
            &s[..1],
            "0".repeat(width - s.len()),
            &s[1..]
        )
    } else {
        format!("{}{}", "0".repeat(width - s.len()), s)
    }
}

