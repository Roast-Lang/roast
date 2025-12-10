//! Memory-mapped file support for Roast.
//!
//! Provides memory-mapped file access similar to Python's mmap module.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

// =============================================================================
// Mmap Constants
// =============================================================================

/// Memory map access mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Read-only access
    Read,
    /// Read-write access
    Write,
    /// Copy-on-write (changes are private)
    Copy,
}

/// Memory map flags.
#[derive(Clone, Copy, Debug, Default)]
pub struct MmapFlags {
    /// Lock pages in memory
    pub locked: bool,
    /// Populate page tables (prefault)
    pub populate: bool,
    /// Use huge pages if available
    pub huge_pages: bool,
}

// =============================================================================
// Mmap Implementation
// =============================================================================

/// A memory-mapped file.
/// 
/// Note: This is a simplified implementation using regular file I/O.
/// A full implementation would use OS-specific mmap syscalls.
pub struct Mmap {
    file: File,
    length: usize,
    offset: usize,
    access: Access,
    position: usize,
    // In a real implementation, this would be a raw pointer to mapped memory
    buffer: Vec<u8>,
}

impl Mmap {
    /// Create a memory map from a file path.
    pub fn open(path: impl AsRef<Path>, access: Access) -> io::Result<Self> {
        Self::open_with_options(path, access, 0, 0, MmapFlags::default())
    }
    
    /// Create a memory map with full options.
    pub fn open_with_options(
        path: impl AsRef<Path>,
        access: Access,
        offset: usize,
        length: usize,
        _flags: MmapFlags,
    ) -> io::Result<Self> {
        let file = match access {
            Access::Read => OpenOptions::new().read(true).open(path)?,
            Access::Write | Access::Copy => OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)?,
        };
        
        let metadata = file.metadata()?;
        let file_len = metadata.len() as usize;
        
        let length = if length == 0 {
            file_len.saturating_sub(offset)
        } else {
            length.min(file_len.saturating_sub(offset))
        };
        
        // Read the file into buffer (simplified implementation)
        let mut mmap = Self {
            file,
            length,
            offset,
            access,
            position: 0,
            buffer: vec![0u8; length],
        };
        
        mmap.reload()?;
        
        Ok(mmap)
    }
    
    /// Create an anonymous memory map (not backed by a file).
    pub fn anonymous(length: usize) -> Self {
        Self {
            file: File::open("/dev/null").unwrap_or_else(|_| {
                // Fallback for non-Unix systems
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(std::env::temp_dir().join(".roast_mmap_anon"))
                    .unwrap()
            }),
            length,
            offset: 0,
            access: Access::Write,
            position: 0,
            buffer: vec![0u8; length],
        }
    }
    
    /// Get the length of the mapped region.
    pub fn len(&self) -> usize {
        self.length
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    
    /// Read bytes from current position.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let available = self.length.saturating_sub(self.position);
        let to_read = buf.len().min(available);
        
        buf[..to_read].copy_from_slice(&self.buffer[self.position..self.position + to_read]);
        self.position += to_read;
        
        to_read
    }
    
    /// Read exact number of bytes.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if self.position + buf.len() > self.length {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "not enough data"));
        }
        
        buf.copy_from_slice(&self.buffer[self.position..self.position + buf.len()]);
        self.position += buf.len();
        
        Ok(())
    }
    
    /// Write bytes at current position.
    pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.access == Access::Read {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "read-only mmap"));
        }
        
        let available = self.length.saturating_sub(self.position);
        let to_write = buf.len().min(available);
        
        self.buffer[self.position..self.position + to_write].copy_from_slice(&buf[..to_write]);
        self.position += to_write;
        
        Ok(to_write)
    }
    
    /// Seek to position.
    pub fn seek(&mut self, pos: usize) -> io::Result<usize> {
        if pos > self.length {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek beyond end"));
        }
        self.position = pos;
        Ok(self.position)
    }
    
    /// Get current position.
    pub fn tell(&self) -> usize {
        self.position
    }
    
    /// Get byte at position.
    pub fn get(&self, index: usize) -> Option<u8> {
        self.buffer.get(index).copied()
    }
    
    /// Set byte at position.
    pub fn set(&mut self, index: usize, value: u8) -> io::Result<()> {
        if self.access == Access::Read {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "read-only mmap"));
        }
        
        if index >= self.length {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "index out of bounds"));
        }
        
        self.buffer[index] = value;
        Ok(())
    }
    
    /// Get a slice of the mapped memory.
    pub fn slice(&self, start: usize, end: usize) -> Option<&[u8]> {
        if end <= self.length && start <= end {
            Some(&self.buffer[start..end])
        } else {
            None
        }
    }
    
    /// Find bytes in the mapped memory.
    pub fn find(&self, needle: &[u8]) -> Option<usize> {
        self.find_from(needle, 0)
    }
    
    /// Find bytes starting from position.
    pub fn find_from(&self, needle: &[u8], start: usize) -> Option<usize> {
        if needle.is_empty() || start >= self.length {
            return None;
        }
        
        self.buffer[start..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + start)
    }
    
    /// Find bytes in reverse.
    pub fn rfind(&self, needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        
        self.buffer
            .windows(needle.len())
            .rposition(|w| w == needle)
    }
    
    /// Flush changes to disk.
    pub fn flush(&mut self) -> io::Result<()> {
        if self.access == Access::Read {
            return Ok(());
        }
        
        self.file.seek(SeekFrom::Start(self.offset as u64))?;
        self.file.write_all(&self.buffer)?;
        self.file.sync_all()?;
        
        Ok(())
    }
    
    /// Reload data from file.
    pub fn reload(&mut self) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(self.offset as u64))?;
        self.file.read_exact(&mut self.buffer)?;
        Ok(())
    }
    
    /// Resize the mapping (if possible).
    pub fn resize(&mut self, new_length: usize) -> io::Result<()> {
        if self.access == Access::Read {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "read-only mmap"));
        }
        
        // Extend or shrink the buffer
        self.buffer.resize(new_length, 0);
        self.length = new_length;
        
        // Adjust position if needed
        if self.position > new_length {
            self.position = new_length;
        }
        
        Ok(())
    }
    
    /// Move data within the mapping.
    pub fn move_(&mut self, dest: usize, src: usize, count: usize) -> io::Result<()> {
        if self.access == Access::Read {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "read-only mmap"));
        }
        
        if src + count > self.length || dest + count > self.length {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "out of bounds"));
        }
        
        // Use copy_within for overlapping regions
        self.buffer.copy_within(src..src + count, dest);
        
        Ok(())
    }
    
    /// Read a line (up to newline).
    pub fn readline(&mut self) -> Vec<u8> {
        let start = self.position;
        
        while self.position < self.length {
            if self.buffer[self.position] == b'\n' {
                self.position += 1;
                return self.buffer[start..self.position].to_vec();
            }
            self.position += 1;
        }
        
        self.buffer[start..self.position].to_vec()
    }
    
    /// Get all bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }
    
    /// Get mutable bytes.
    pub fn as_bytes_mut(&mut self) -> Option<&mut [u8]> {
        if self.access == Access::Read {
            None
        } else {
            Some(&mut self.buffer)
        }
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        // Flush changes on drop for write access
        if self.access != Access::Read {
            let _ = self.flush();
        }
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Open a file as a read-only memory map.
pub fn mmap(path: impl AsRef<Path>) -> io::Result<Mmap> {
    Mmap::open(path, Access::Read)
}

/// Open a file as a read-write memory map.
pub fn mmap_mut(path: impl AsRef<Path>) -> io::Result<Mmap> {
    Mmap::open(path, Access::Write)
}

/// Create an anonymous memory map.
pub fn mmap_anonymous(length: usize) -> Mmap {
    Mmap::anonymous(length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    
    fn create_temp_file(content: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(content).unwrap();
        file
    }
    
    #[test]
    fn test_mmap_read() {
        let file = create_temp_file(b"Hello, World!");
        let mut mmap = Mmap::open(file.path(), Access::Read).unwrap();
        
        assert_eq!(mmap.len(), 13);
        
        let mut buf = [0u8; 5];
        let n = mmap.read(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(&buf, b"Hello");
    }
    
    #[test]
    fn test_mmap_write() {
        let file = create_temp_file(b"Hello, World!");
        let mut mmap = Mmap::open(file.path(), Access::Write).unwrap();
        
        mmap.seek(0).unwrap();
        mmap.write(b"HELLO").unwrap();
        
        assert_eq!(&mmap.buffer[..5], b"HELLO");
    }
    
    #[test]
    fn test_mmap_find() {
        let file = create_temp_file(b"Hello, World! Hello again!");
        let mmap = Mmap::open(file.path(), Access::Read).unwrap();
        
        assert_eq!(mmap.find(b"World"), Some(7));
        assert_eq!(mmap.find(b"Hello"), Some(0));
        assert_eq!(mmap.find_from(b"Hello", 1), Some(14));
        assert_eq!(mmap.find(b"xyz"), None);
    }
    
    #[test]
    fn test_mmap_anonymous() {
        let mut mmap = Mmap::anonymous(1024);
        
        assert_eq!(mmap.len(), 1024);
        
        mmap.write(b"Anonymous data").unwrap();
        mmap.seek(0).unwrap();
        
        let mut buf = [0u8; 14];
        mmap.read(&mut buf);
        assert_eq!(&buf, b"Anonymous data");
    }
    
    #[test]
    fn test_mmap_slice() {
        let file = create_temp_file(b"Hello, World!");
        let mmap = Mmap::open(file.path(), Access::Read).unwrap();
        
        assert_eq!(mmap.slice(0, 5), Some(b"Hello".as_slice()));
        assert_eq!(mmap.slice(7, 12), Some(b"World".as_slice()));
    }
    
    #[test]
    fn test_mmap_readline() {
        let file = create_temp_file(b"Line 1\nLine 2\nLine 3");
        let mut mmap = Mmap::open(file.path(), Access::Read).unwrap();
        
        assert_eq!(mmap.readline(), b"Line 1\n");
        assert_eq!(mmap.readline(), b"Line 2\n");
        assert_eq!(mmap.readline(), b"Line 3");
    }
}

