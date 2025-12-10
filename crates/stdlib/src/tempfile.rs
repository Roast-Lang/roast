//! Temporary file and directory utilities for Roast.
//!
//! Generate temporary files and directories that can be automatically cleaned up.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for unique names
static COUNTER: AtomicU64 = AtomicU64::new(0);

// =============================================================================
// Temporary Directory
// =============================================================================

/// A temporary directory that is deleted when dropped.
pub struct TempDir {
    path: PathBuf,
    cleanup: bool,
}

impl TempDir {
    /// Create a new temporary directory.
    pub fn new() -> io::Result<Self> {
        Self::new_in(env::temp_dir())
    }
    
    /// Create a temporary directory with a prefix.
    pub fn with_prefix(prefix: &str) -> io::Result<Self> {
        Self::with_prefix_in(prefix, env::temp_dir())
    }
    
    /// Create a temporary directory in a specific location.
    pub fn new_in<P: AsRef<Path>>(dir: P) -> io::Result<Self> {
        Self::with_prefix_in("roast_tmp", dir)
    }
    
    /// Create a temporary directory with prefix in a specific location.
    pub fn with_prefix_in<P: AsRef<Path>>(prefix: &str, dir: P) -> io::Result<Self> {
        let unique = generate_unique_name(prefix);
        let path = dir.as_ref().join(&unique);
        
        fs::create_dir_all(&path)?;
        
        Ok(Self { path, cleanup: true })
    }
    
    /// Get the path to this temporary directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// Create a file in this temporary directory.
    pub fn create_file(&self, name: &str) -> io::Result<File> {
        File::create(self.path.join(name))
    }
    
    /// Create a subdirectory in this temporary directory.
    pub fn create_dir(&self, name: &str) -> io::Result<PathBuf> {
        let path = self.path.join(name);
        fs::create_dir_all(&path)?;
        Ok(path)
    }
    
    /// Keep the directory instead of cleaning up.
    pub fn into_path(mut self) -> PathBuf {
        self.cleanup = false;
        self.path.clone()
    }
    
    /// Close and remove the temporary directory.
    pub fn close(self) -> io::Result<()> {
        // Drop will handle cleanup
        Ok(())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

// =============================================================================
// Temporary File
// =============================================================================

/// A temporary file that is deleted when dropped.
pub struct NamedTempFile {
    file: File,
    path: PathBuf,
    cleanup: bool,
}

impl NamedTempFile {
    /// Create a new temporary file.
    pub fn new() -> io::Result<Self> {
        Self::new_in(env::temp_dir())
    }
    
    /// Create a temporary file with a suffix.
    pub fn with_suffix(suffix: &str) -> io::Result<Self> {
        Self::with_suffix_in(suffix, env::temp_dir())
    }
    
    /// Create a temporary file in a specific directory.
    pub fn new_in<P: AsRef<Path>>(dir: P) -> io::Result<Self> {
        let unique = generate_unique_name("roast_tmp");
        let path = dir.as_ref().join(&unique);
        
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        
        Ok(Self { file, path, cleanup: true })
    }
    
    /// Create a temporary file with suffix in a specific directory.
    pub fn with_suffix_in<P: AsRef<Path>>(suffix: &str, dir: P) -> io::Result<Self> {
        let unique = format!("{}{}", generate_unique_name("roast_tmp"), suffix);
        let path = dir.as_ref().join(&unique);
        
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        
        Ok(Self { file, path, cleanup: true })
    }
    
    /// Get the path to this temporary file.
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// Get a reference to the underlying file.
    pub fn as_file(&self) -> &File {
        &self.file
    }
    
    /// Get a mutable reference to the underlying file.
    pub fn as_file_mut(&mut self) -> &mut File {
        &mut self.file
    }
    
    /// Reopen the file for reading.
    pub fn reopen(&self) -> io::Result<File> {
        File::open(&self.path)
    }
    
    /// Keep the file instead of cleaning up, return the path.
    pub fn into_path(mut self) -> PathBuf {
        self.cleanup = false;
        self.path.clone()
    }
    
    /// Keep the file, return path and reopen the file.
    pub fn keep(mut self) -> io::Result<(File, PathBuf)> {
        self.cleanup = false;
        let path = self.path.clone();
        let file = File::open(&path)?;
        Ok((file, path))
    }
    
    /// Close and remove the temporary file.
    pub fn close(self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for NamedTempFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for NamedTempFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }
    
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for NamedTempFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

impl Drop for NamedTempFile {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl AsRef<Path> for NamedTempFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

// =============================================================================
// Spooled Temporary File
// =============================================================================

/// A file that starts in memory and spills to disk when it exceeds a threshold.
pub struct SpooledTempFile {
    inner: SpoolState,
    max_size: usize,
}

enum SpoolState {
    InMemory(io::Cursor<Vec<u8>>),
    OnDisk(NamedTempFile),
}

impl SpooledTempFile {
    /// Create a spooled temp file with given max in-memory size.
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: SpoolState::InMemory(io::Cursor::new(Vec::new())),
            max_size,
        }
    }
    
    /// Check if the file has been spooled to disk.
    pub fn is_rolled(&self) -> bool {
        matches!(self.inner, SpoolState::OnDisk(_))
    }
    
    /// Force the file to roll over to disk.
    pub fn rollover(&mut self) -> io::Result<()> {
        if let SpoolState::InMemory(cursor) = &self.inner {
            let mut file = NamedTempFile::new()?;
            file.write_all(cursor.get_ref())?;
            file.seek(SeekFrom::Start(cursor.position()))?;
            self.inner = SpoolState::OnDisk(file);
        }
        Ok(())
    }
    
    fn maybe_rollover(&mut self) -> io::Result<()> {
        if let SpoolState::InMemory(cursor) = &self.inner {
            if cursor.get_ref().len() > self.max_size {
                self.rollover()?;
            }
        }
        Ok(())
    }
}

impl Read for SpooledTempFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            SpoolState::InMemory(cursor) => cursor.read(buf),
            SpoolState::OnDisk(file) => file.read(buf),
        }
    }
}

impl Write for SpooledTempFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = match &mut self.inner {
            SpoolState::InMemory(cursor) => cursor.write(buf)?,
            SpoolState::OnDisk(file) => file.write(buf)?,
        };
        self.maybe_rollover()?;
        Ok(n)
    }
    
    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            SpoolState::InMemory(cursor) => cursor.flush(),
            SpoolState::OnDisk(file) => file.flush(),
        }
    }
}

impl Seek for SpooledTempFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match &mut self.inner {
            SpoolState::InMemory(cursor) => cursor.seek(pos),
            SpoolState::OnDisk(file) => file.seek(pos),
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn generate_unique_name(prefix: &str) -> String {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    
    format!("{}_{}_{}_{}", prefix, pid, ts, id)
}

/// Get the system's temporary directory.
pub fn gettempdir() -> PathBuf {
    env::temp_dir()
}

/// Get the prefix used for temporary files.
pub fn gettempprefix() -> &'static str {
    "roast_tmp"
}

/// Create a temporary file and return its name.
pub fn mktemp(suffix: &str, prefix: &str, dir: Option<&Path>) -> io::Result<PathBuf> {
    let base_dir = dir.map(PathBuf::from).unwrap_or_else(env::temp_dir);
    let name = format!("{}{}{}", prefix, generate_unique_name(""), suffix);
    let path = base_dir.join(&name);
    
    // Just return the path, don't create the file
    Ok(path)
}

/// Create a temporary directory and return its path.
pub fn mkdtemp(suffix: &str, prefix: &str, dir: Option<&Path>) -> io::Result<PathBuf> {
    let base_dir = dir.map(PathBuf::from).unwrap_or_else(env::temp_dir);
    let name = format!("{}{}{}", prefix, generate_unique_name(""), suffix);
    let path = base_dir.join(&name);
    
    fs::create_dir_all(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tempdir() {
        let dir = TempDir::new().unwrap();
        assert!(dir.path().exists());
        
        let path = dir.path().to_path_buf();
        drop(dir);
        assert!(!path.exists());
    }
    
    #[test]
    fn test_tempfile() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "hello").unwrap();
        file.flush().unwrap();
        
        assert!(file.path().exists());
        
        let path = file.path().to_path_buf();
        drop(file);
        assert!(!path.exists());
    }
    
    #[test]
    fn test_tempfile_persist() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "hello").unwrap();
        
        let path = file.into_path();
        assert!(path.exists());
        
        // Clean up manually
        fs::remove_file(&path).unwrap();
    }
    
    #[test]
    fn test_spooled() {
        let mut file = SpooledTempFile::new(10);
        
        // Should stay in memory
        write!(file, "small").unwrap();
        assert!(!file.is_rolled());
        
        // Should roll over
        write!(file, "this is a much longer string").unwrap();
        assert!(file.is_rolled());
    }
    
    #[test]
    fn test_gettempdir() {
        let dir = gettempdir();
        assert!(dir.exists());
    }
}

