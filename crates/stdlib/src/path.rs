//! Path manipulation utilities.
//!
//! Platform-independent path operations.

use std::path::{Path as StdPath, PathBuf};

/// Join path components.
pub fn join(parts: &[&str]) -> String {
    let path: PathBuf = parts.iter().collect();
    path.to_string_lossy().to_string()
}

/// Get file name from path.
pub fn basename(path: &str) -> String {
    StdPath::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Get directory name from path.
pub fn dirname(path: &str) -> String {
    StdPath::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Get file extension (without dot).
pub fn extension(path: &str) -> String {
    StdPath::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Get file stem (name without extension).
pub fn stem(path: &str) -> String {
    StdPath::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Split path into (dirname, basename).
pub fn split(path: &str) -> (String, String) {
    (dirname(path), basename(path))
}

/// Split extension: ("path/file", ".ext").
pub fn splitext(path: &str) -> (String, String) {
    let p = StdPath::new(path);
    let ext = p.extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let without_ext = if ext.is_empty() {
        path.to_string()
    } else {
        path[..path.len() - ext.len()].to_string()
    };
    (without_ext, ext)
}

/// Normalize path (remove . and ..).
pub fn normalize(path: &str) -> String {
    let mut components = Vec::new();
    
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => { components.pop(); }
            _ => components.push(part),
        }
    }
    
    let result = components.join("/");
    if path.starts_with('/') {
        format!("/{}", result)
    } else {
        result
    }
}

/// Check if path is absolute.
pub fn is_absolute(path: &str) -> bool {
    StdPath::new(path).is_absolute()
}

/// Check if path is relative.
pub fn is_relative(path: &str) -> bool {
    !is_absolute(path)
}

/// Convert to absolute path.
pub fn abspath(path: &str) -> std::io::Result<String> {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
}

/// Get current working directory.
pub fn cwd() -> std::io::Result<String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
}

/// Change current working directory.
pub fn chdir(path: &str) -> std::io::Result<()> {
    std::env::set_current_dir(path)
}

/// Expand ~ to home directory.
pub fn expanduser(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.to_string_lossy();
            if path == "~" {
                return home.to_string();
            }
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}

/// Check if two paths are the same file.
pub fn same_file(path1: &str, path2: &str) -> std::io::Result<bool> {
    let p1 = std::fs::canonicalize(path1)?;
    let p2 = std::fs::canonicalize(path2)?;
    Ok(p1 == p2)
}

/// Get relative path from one path to another.
pub fn relative(path: &str, base: &str) -> String {
    let path = StdPath::new(path);
    let base = StdPath::new(base);
    
    if let Ok(rel) = path.strip_prefix(base) {
        rel.to_string_lossy().to_string()
    } else {
        path.to_string_lossy().to_string()
    }
}

/// Path builder for constructing paths fluently.
pub struct PathBuilder {
    parts: Vec<String>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }
    
    pub fn from(path: &str) -> Self {
        Self { parts: vec![path.to_string()] }
    }
    
    pub fn push(&mut self, part: &str) -> &mut Self {
        self.parts.push(part.to_string());
        self
    }
    
    pub fn join(&mut self, part: &str) -> &mut Self {
        self.push(part)
    }
    
    pub fn build(&self) -> String {
        let refs: Vec<&str> = self.parts.iter().map(|s| s.as_str()).collect();
        super::path::join(&refs)
    }
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basename() {
        assert_eq!(basename("/path/to/file.txt"), "file.txt");
        assert_eq!(basename("file.txt"), "file.txt");
    }
    
    #[test]
    fn test_dirname() {
        assert_eq!(dirname("/path/to/file.txt"), "/path/to");
    }
    
    #[test]
    fn test_extension() {
        assert_eq!(extension("file.txt"), "txt");
        assert_eq!(extension("file"), "");
    }
    
    #[test]
    fn test_normalize() {
        assert_eq!(normalize("/a/b/../c"), "/a/c");
        assert_eq!(normalize("a/./b/c"), "a/b/c");
    }
}

