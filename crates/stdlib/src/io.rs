//! I/O operations.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

/// Read entire file as string.
pub fn read_file(path: &str) -> io::Result<String> {
    std::fs::read_to_string(path)
}

/// Read file as bytes.
pub fn read_bytes(path: &str) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

/// Write string to file.
pub fn write_file(path: &str, contents: &str) -> io::Result<()> {
    std::fs::write(path, contents)
}

/// Write bytes to file.
pub fn write_bytes(path: &str, contents: &[u8]) -> io::Result<()> {
    std::fs::write(path, contents)
}

/// Append to file.
pub fn append_file(path: &str, contents: &str) -> io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    file.write_all(contents.as_bytes())
}

/// Read lines from file.
pub fn read_lines(path: &str) -> io::Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    reader.lines().collect()
}

/// Check if path exists.
pub fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Check if path is file.
pub fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

/// Check if path is directory.
pub fn is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Create directory.
pub fn mkdir(path: &str) -> io::Result<()> {
    std::fs::create_dir(path)
}

/// Create directory and parents.
pub fn makedirs(path: &str) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Remove file.
pub fn remove(path: &str) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Remove directory.
pub fn rmdir(path: &str) -> io::Result<()> {
    std::fs::remove_dir(path)
}

/// List directory contents.
pub fn listdir(path: &str) -> io::Result<Vec<String>> {
    let entries = std::fs::read_dir(path)?;
    entries
        .map(|entry| {
            entry.map(|e| e.file_name().to_string_lossy().to_string())
        })
        .collect()
}

/// Read from stdin.
pub fn input(prompt: &str) -> io::Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim_end().to_string())
}

