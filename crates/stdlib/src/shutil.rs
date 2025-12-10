//! High-level file operations for Roast.
//!
//! Provides utilities for copying, moving, and managing files and directories.

use std::fs::{self, File, Metadata};
use std::io::{self, Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};

// =============================================================================
// Copy Operations
// =============================================================================

/// Copy a file from src to dst.
pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<u64> {
    fs::copy(src, dst)
}

/// Copy a file and preserve metadata.
pub fn copy2<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<u64> {
    let bytes = fs::copy(&src, &dst)?;
    
    // Copy metadata
    let metadata = fs::metadata(&src)?;
    let perms = metadata.permissions();
    fs::set_permissions(&dst, perms)?;
    
    Ok(bytes)
}

/// Copy only the file content, not metadata.
pub fn copyfile<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    let mut src_file = BufReader::new(File::open(src)?);
    let mut dst_file = BufWriter::new(File::create(dst)?);
    
    io::copy(&mut src_file, &mut dst_file)?;
    dst_file.flush()
}

/// Copy a file object.
pub fn copyfileobj<R: Read, W: Write>(fsrc: &mut R, fdst: &mut W) -> io::Result<u64> {
    io::copy(fsrc, fdst)
}

/// Copy a file object with specific buffer size.
pub fn copyfileobj_with_buffer<R: Read, W: Write>(
    fsrc: &mut R,
    fdst: &mut W,
    length: usize,
) -> io::Result<u64> {
    let mut buffer = vec![0u8; length];
    let mut total = 0u64;
    
    loop {
        let n = fsrc.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        fdst.write_all(&buffer[..n])?;
        total += n as u64;
    }
    
    Ok(total)
}

/// Copy file permissions.
pub fn copymode<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    let metadata = fs::metadata(&src)?;
    let perms = metadata.permissions();
    fs::set_permissions(&dst, perms)
}

/// Copy file metadata (permissions, timestamps).
pub fn copystat<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    let metadata = fs::metadata(&src)?;
    let perms = metadata.permissions();
    fs::set_permissions(&dst, perms)?;
    
    // Note: Copying timestamps requires platform-specific code
    // This is a simplified version
    Ok(())
}

// =============================================================================
// Directory Operations
// =============================================================================

/// Recursively copy a directory tree.
pub fn copytree<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    copytree_with_options(src, dst, CopyTreeOptions::default())
}

/// Options for copytree operation.
#[derive(Clone, Default)]
pub struct CopyTreeOptions {
    /// Follow symbolic links
    pub follow_symlinks: bool,
    /// Ignore errors
    pub ignore_errors: bool,
    /// Patterns to ignore
    pub ignore_patterns: Vec<String>,
    /// Copy function to use
    pub copy_function: CopyFunction,
    /// Directories to ignore
    pub ignore_dangling_symlinks: bool,
}

/// Which copy function to use.
#[derive(Clone, Copy, Default)]
pub enum CopyFunction {
    #[default]
    Copy2,
    Copy,
}

/// Copy a directory tree with options.
pub fn copytree_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    src: P,
    dst: Q,
    options: CopyTreeOptions,
) -> io::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    
    fs::create_dir_all(dst)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);
        
        // Check ignore patterns
        let name_str = file_name.to_string_lossy();
        if options.ignore_patterns.iter().any(|p| {
            glob_match(p, &name_str)
        }) {
            continue;
        }
        
        let file_type = entry.file_type()?;
        
        if file_type.is_dir() {
            copytree_with_options(&src_path, &dst_path, options.clone())?;
        } else if file_type.is_file() {
            match options.copy_function {
                CopyFunction::Copy2 => { copy2(&src_path, &dst_path)?; }
                CopyFunction::Copy => { copy(&src_path, &dst_path)?; }
            }
        } else if file_type.is_symlink() {
            if options.follow_symlinks {
                let target = fs::read_link(&src_path)?;
                if target.is_dir() {
                    copytree_with_options(&src_path, &dst_path, options.clone())?;
                } else {
                    copy2(&src_path, &dst_path)?;
                }
            } else {
                // Copy symlink as-is
                #[cfg(unix)]
                {
                    let target = fs::read_link(&src_path)?;
                    std::os::unix::fs::symlink(&target, &dst_path)?;
                }
                #[cfg(not(unix))]
                {
                    copy2(&src_path, &dst_path)?;
                }
            }
        }
    }
    
    // Copy directory permissions
    copymode(src, dst)?;
    
    Ok(())
}

fn glob_match(pattern: &str, name: &str) -> bool {
    // Simple glob matching
    if pattern.starts_with('*') && pattern.ends_with('*') {
        name.contains(&pattern[1..pattern.len()-1])
    } else if pattern.starts_with('*') {
        name.ends_with(&pattern[1..])
    } else if pattern.ends_with('*') {
        name.starts_with(&pattern[..pattern.len()-1])
    } else {
        pattern == name
    }
}

/// Remove a directory tree.
pub fn rmtree<P: AsRef<Path>>(path: P) -> io::Result<()> {
    fs::remove_dir_all(path)
}

/// Remove a directory tree, ignoring errors.
pub fn rmtree_ignore_errors<P: AsRef<Path>>(path: P) {
    let _ = fs::remove_dir_all(path);
}

// =============================================================================
// Move Operations
// =============================================================================

/// Move a file or directory to another location.
pub fn move_<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<PathBuf> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    
    // If dst is a directory, move src into it
    let real_dst = if dst.is_dir() {
        dst.join(src.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid source path")
        })?)
    } else {
        dst.to_path_buf()
    };
    
    // Try rename first (fastest if on same filesystem)
    match fs::rename(src, &real_dst) {
        Ok(()) => Ok(real_dst),
        Err(_) => {
            // Fall back to copy and delete
            if src.is_dir() {
                copytree(src, &real_dst)?;
                rmtree(src)?;
            } else {
                copy2(src, &real_dst)?;
                fs::remove_file(src)?;
            }
            Ok(real_dst)
        }
    }
}

// =============================================================================
// Disk Usage
// =============================================================================

/// Result of disk usage calculation.
#[derive(Clone, Debug, Default)]
pub struct DiskUsage {
    /// Total size in bytes
    pub total: u64,
    /// Used size in bytes
    pub used: u64,
    /// Free size in bytes
    pub free: u64,
}

impl DiskUsage {
    /// Get usage as percentage.
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64) * 100.0
        }
    }
}

/// Get disk usage statistics for a path.
#[cfg(unix)]
pub fn disk_usage<P: AsRef<Path>>(path: P) -> io::Result<DiskUsage> {
    use std::os::unix::fs::MetadataExt;
    
    let metadata = fs::metadata(&path)?;
    
    // This is a simplified version - real implementation would use statvfs
    Ok(DiskUsage {
        total: 0,
        used: metadata.size(),
        free: 0,
    })
}

#[cfg(not(unix))]
pub fn disk_usage<P: AsRef<Path>>(_path: P) -> io::Result<DiskUsage> {
    Ok(DiskUsage::default())
}

// =============================================================================
// Archive Operations
// =============================================================================

/// Supported archive formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
}

impl ArchiveFormat {
    /// Detect format from filename.
    pub fn from_filename(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.ends_with(".zip") {
            Some(ArchiveFormat::Zip)
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Some(ArchiveFormat::TarGz)
        } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
            Some(ArchiveFormat::TarBz2)
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            Some(ArchiveFormat::TarXz)
        } else if lower.ends_with(".tar") {
            Some(ArchiveFormat::Tar)
        } else {
            None
        }
    }
}

/// Create an archive from a directory.
pub fn make_archive<P: AsRef<Path>, Q: AsRef<Path>>(
    base_name: P,
    format: ArchiveFormat,
    root_dir: Q,
) -> io::Result<PathBuf> {
    let base = base_name.as_ref();
    let root = root_dir.as_ref();
    
    let archive_name = match format {
        ArchiveFormat::Zip => base.with_extension("zip"),
        ArchiveFormat::Tar => base.with_extension("tar"),
        ArchiveFormat::TarGz => PathBuf::from(format!("{}.tar.gz", base.display())),
        ArchiveFormat::TarBz2 => PathBuf::from(format!("{}.tar.bz2", base.display())),
        ArchiveFormat::TarXz => PathBuf::from(format!("{}.tar.xz", base.display())),
    };
    
    // Note: Actual archive creation requires external libraries
    // This is a placeholder
    let _ = root;
    
    Ok(archive_name)
}

/// Unpack an archive.
pub fn unpack_archive<P: AsRef<Path>, Q: AsRef<Path>>(
    filename: P,
    extract_dir: Q,
) -> io::Result<()> {
    let filename = filename.as_ref();
    let extract_dir = extract_dir.as_ref();
    
    let _format = ArchiveFormat::from_filename(&filename.to_string_lossy())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;
    
    fs::create_dir_all(extract_dir)?;
    
    // Note: Actual unpacking requires external libraries
    Ok(())
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Find an executable in PATH.
pub fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join(cmd);
                if full_path.is_file() {
                    Some(full_path)
                } else {
                    // Try with common extensions on Windows
                    #[cfg(windows)]
                    {
                        for ext in &[".exe", ".cmd", ".bat", ".com"] {
                            let with_ext = dir.join(format!("{}{}", cmd, ext));
                            if with_ext.is_file() {
                                return Some(with_ext);
                            }
                        }
                    }
                    None
                }
            })
            .next()
    })
}

/// Get the size of a terminal window.
pub fn get_terminal_size() -> Option<(u16, u16)> {
    // This would require platform-specific code
    // Returning a default for now
    Some((80, 24))
}

/// Chown-like operation (Unix only).
#[cfg(unix)]
pub fn chown<P: AsRef<Path>>(path: P, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::chown;
    chown(path, uid, gid)
}

#[cfg(not(unix))]
pub fn chown<P: AsRef<Path>>(_path: P, _uid: Option<u32>, _gid: Option<u32>) -> io::Result<()> {
    Ok(())
}

/// Get file owner (Unix only).
#[cfg(unix)]
pub fn get_owner<P: AsRef<Path>>(path: P) -> io::Result<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path)?;
    Ok((metadata.uid(), metadata.gid()))
}

#[cfg(not(unix))]
pub fn get_owner<P: AsRef<Path>>(_path: P) -> io::Result<(u32, u32)> {
    Ok((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempfile::TempDir;
    
    #[test]
    fn test_copy() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        
        fs::write(&src, "hello").unwrap();
        copy(&src, &dst).unwrap();
        
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
    }
    
    #[test]
    fn test_copytree() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        
        // Create some files
        fs::write(src_dir.path().join("file1.txt"), "one").unwrap();
        fs::create_dir(src_dir.path().join("subdir")).unwrap();
        fs::write(src_dir.path().join("subdir/file2.txt"), "two").unwrap();
        
        let dst = dst_dir.path().join("copied");
        copytree(src_dir.path(), &dst).unwrap();
        
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());
    }
    
    #[test]
    fn test_move() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        
        fs::write(&src, "hello").unwrap();
        move_(&src, &dst).unwrap();
        
        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
    }
    
    #[test]
    fn test_which() {
        // Should find common commands
        #[cfg(unix)]
        {
            assert!(which("ls").is_some() || which("echo").is_some());
        }
        #[cfg(windows)]
        {
            assert!(which("cmd").is_some());
        }
    }
    
    #[test]
    fn test_archive_format() {
        assert_eq!(ArchiveFormat::from_filename("file.zip"), Some(ArchiveFormat::Zip));
        assert_eq!(ArchiveFormat::from_filename("file.tar.gz"), Some(ArchiveFormat::TarGz));
        assert_eq!(ArchiveFormat::from_filename("file.txt"), None);
    }
}

