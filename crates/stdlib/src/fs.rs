//! File system operations.
//!
//! Provides functions for working with files and directories.

use std::fs as std_fs;
use std::io::{self, Read, Write, BufReader, BufWriter};
use std::path::Path;

/// Read entire file contents as a string.
pub fn read_text<P: AsRef<Path>>(path: P) -> io::Result<String> {
    std_fs::read_to_string(path)
}

/// Read entire file contents as bytes.
pub fn read_bytes<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    std_fs::read(path)
}

/// Write string to file (creates or overwrites).
pub fn write_text<P: AsRef<Path>>(path: P, contents: &str) -> io::Result<()> {
    std_fs::write(path, contents)
}

/// Write bytes to file (creates or overwrites).
pub fn write_bytes<P: AsRef<Path>>(path: P, contents: &[u8]) -> io::Result<()> {
    std_fs::write(path, contents)
}

/// Append string to file.
pub fn append_text<P: AsRef<Path>>(path: P, contents: &str) -> io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

/// Check if path exists.
pub fn exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Check if path is a file.
pub fn is_file<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().is_file()
}

/// Check if path is a directory.
pub fn is_dir<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().is_dir()
}

/// Create a directory.
pub fn mkdir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std_fs::create_dir(path)
}

/// Create a directory and all parent directories.
pub fn mkdir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std_fs::create_dir_all(path)
}

/// Remove a file.
pub fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std_fs::remove_file(path)
}

/// Remove an empty directory.
pub fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std_fs::remove_dir(path)
}

/// Remove a directory and all its contents.
pub fn remove_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std_fs::remove_dir_all(path)
}

/// Copy a file.
pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
    std_fs::copy(from, to)
}

/// Rename/move a file or directory.
pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    std_fs::rename(from, to)
}

/// Get file metadata.
pub fn metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata> {
    let meta = std_fs::metadata(path)?;
    Ok(Metadata {
        size: meta.len(),
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        is_symlink: meta.is_symlink(),
        readonly: meta.permissions().readonly(),
        modified: meta.modified().ok().map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }),
        created: meta.created().ok().map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }),
    })
}

/// File metadata.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub size: u64,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub readonly: bool,
    pub modified: Option<u64>,
    pub created: Option<u64>,
}

/// List directory contents.
pub fn list_dir<P: AsRef<Path>>(path: P) -> io::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in std_fs::read_dir(path)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            entries.push(name.to_string());
        }
    }
    Ok(entries)
}

/// Walk directory tree.
pub fn walk_dir<P: AsRef<Path>>(path: P) -> io::Result<Vec<WalkEntry>> {
    fn walk_recursive(path: &Path, entries: &mut Vec<WalkEntry>) -> io::Result<()> {
        if path.is_dir() {
            for entry in std_fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                let meta = entry.metadata()?;
                
                entries.push(WalkEntry {
                    path: path.to_string_lossy().to_string(),
                    is_file: meta.is_file(),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                });
                
                if path.is_dir() {
                    walk_recursive(&path, entries)?;
                }
            }
        }
        Ok(())
    }
    
    let mut entries = Vec::new();
    walk_recursive(path.as_ref(), &mut entries)?;
    Ok(entries)
}

/// Directory walk entry.
#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

/// Read file line by line.
pub fn read_lines<P: AsRef<Path>>(path: P) -> io::Result<Vec<String>> {
    let contents = std_fs::read_to_string(path)?;
    Ok(contents.lines().map(String::from).collect())
}

/// Temporary file operations.
pub mod temp {
    use super::*;
    use std::env;
    
    /// Get the temporary directory path.
    pub fn temp_dir() -> String {
        env::temp_dir().to_string_lossy().to_string()
    }
    
    /// Create a temporary file and return its path.
    pub fn temp_file(prefix: &str) -> io::Result<String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        
        let path = format!("{}/{}_{}", temp_dir(), prefix, timestamp);
        std_fs::write(&path, "")?;
        Ok(path)
    }
    
    /// Create a temporary directory and return its path.
    pub fn temp_dir_create(prefix: &str) -> io::Result<String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        
        let path = format!("{}/{}_{}", temp_dir(), prefix, timestamp);
        std_fs::create_dir(&path)?;
        Ok(path)
    }
}

/// File handle for streaming operations.
pub struct File {
    inner: std_fs::File,
    path: String,
}

impl File {
    /// Open file for reading.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let inner = std_fs::File::open(path)?;
        Ok(Self { inner, path: path_str })
    }
    
    /// Create file for writing.
    pub fn create<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let inner = std_fs::File::create(path)?;
        Ok(Self { inner, path: path_str })
    }
    
    /// Read all contents.
    pub fn read_all(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.inner.read_to_end(&mut buf)?;
        Ok(buf)
    }
    
    /// Read as string.
    pub fn read_string(&mut self) -> io::Result<String> {
        let mut buf = String::new();
        self.inner.read_to_string(&mut buf)?;
        Ok(buf)
    }
    
    /// Write bytes.
    pub fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.inner.write(data)
    }
    
    /// Write string.
    pub fn write_str(&mut self, data: &str) -> io::Result<usize> {
        self.inner.write(data.as_bytes())
    }
    
    /// Flush writes.
    pub fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
    
    /// Get file path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

