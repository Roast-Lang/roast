//! Compression utilities.
//!
//! Provides gzip, zlib, and deflate compression/decompression.
//! Based on the DEFLATE algorithm (RFC 1951).

use std::io::{self, Read, Write};

/// Compression level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionLevel {
    /// No compression.
    None,
    /// Fast compression (level 1).
    Fast,
    /// Best compression (level 9).
    Best,
    /// Default compression (level 6).
    Default,
    /// Custom level (0-9).
    Custom(u32),
}

impl CompressionLevel {
    /// Get the numeric level (0-9).
    pub fn level(&self) -> u32 {
        match self {
            CompressionLevel::None => 0,
            CompressionLevel::Fast => 1,
            CompressionLevel::Best => 9,
            CompressionLevel::Default => 6,
            CompressionLevel::Custom(l) => (*l).min(9),
        }
    }
}

impl Default for CompressionLevel {
    fn default() -> Self {
        CompressionLevel::Default
    }
}

/// Compression format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionFormat {
    /// Gzip format (RFC 1952).
    Gzip,
    /// Zlib format (RFC 1950).
    Zlib,
    /// Raw deflate (RFC 1951).
    Deflate,
}

/// Compression error.
#[derive(Debug)]
pub enum CompressionError {
    /// IO error.
    Io(io::Error),
    /// Invalid data.
    InvalidData(String),
    /// Buffer too small.
    BufferTooSmall,
}

impl std::fmt::Display for CompressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionError::Io(e) => write!(f, "IO error: {}", e),
            CompressionError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
            CompressionError::BufferTooSmall => write!(f, "Buffer too small"),
        }
    }
}

impl std::error::Error for CompressionError {}

impl From<io::Error> for CompressionError {
    fn from(e: io::Error) -> Self {
        CompressionError::Io(e)
    }
}

pub type CompressionResult<T> = Result<T, CompressionError>;

// =============================================================================
// Simple Compression Implementation (without external deps)
// =============================================================================

/// Compress data using a simple run-length encoding.
/// Note: This is a placeholder. For production, use the flate2 crate.
pub fn compress(data: &[u8], _level: CompressionLevel, format: CompressionFormat) -> CompressionResult<Vec<u8>> {
    let mut output = Vec::with_capacity(data.len());
    
    // Add format header
    match format {
        CompressionFormat::Gzip => {
            // Gzip header (simplified)
            output.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff]);
        }
        CompressionFormat::Zlib => {
            // Zlib header
            output.extend_from_slice(&[0x78, 0x9c]);
        }
        CompressionFormat::Deflate => {
            // No header for raw deflate
        }
    }
    
    // Simple compression: store blocks
    // In production, this would use actual DEFLATE algorithm
    compress_deflate_simple(data, &mut output)?;
    
    // Add format trailer
    match format {
        CompressionFormat::Gzip => {
            // CRC32 and original size
            let crc = crc32(data);
            output.extend_from_slice(&crc.to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        }
        CompressionFormat::Zlib => {
            // Adler32 checksum
            let adler = adler32(data);
            output.extend_from_slice(&adler.to_be_bytes());
        }
        CompressionFormat::Deflate => {
            // No trailer
        }
    }
    
    Ok(output)
}

/// Decompress data.
pub fn decompress(data: &[u8], format: CompressionFormat) -> CompressionResult<Vec<u8>> {
    let (payload, _expected_checksum) = match format {
        CompressionFormat::Gzip => {
            if data.len() < 18 {
                return Err(CompressionError::InvalidData("Data too short for gzip".into()));
            }
            if data[0] != 0x1f || data[1] != 0x8b {
                return Err(CompressionError::InvalidData("Invalid gzip header".into()));
            }
            // Skip header (10 bytes) and trailer (8 bytes)
            (&data[10..data.len()-8], Some(&data[data.len()-8..]))
        }
        CompressionFormat::Zlib => {
            if data.len() < 6 {
                return Err(CompressionError::InvalidData("Data too short for zlib".into()));
            }
            // Skip header (2 bytes) and trailer (4 bytes)
            (&data[2..data.len()-4], Some(&data[data.len()-4..]))
        }
        CompressionFormat::Deflate => {
            (data, None)
        }
    };
    
    decompress_deflate_simple(payload)
}

/// Simple deflate compression (stored blocks only).
fn compress_deflate_simple(data: &[u8], output: &mut Vec<u8>) -> CompressionResult<()> {
    // Use stored blocks for simplicity
    // Block format: BFINAL (1 bit) + BTYPE (2 bits) = 0b001 for final stored block
    
    let max_block_size = 65535;
    let mut offset = 0;
    
    while offset < data.len() {
        let remaining = data.len() - offset;
        let block_size = remaining.min(max_block_size);
        let is_final = offset + block_size >= data.len();
        
        // Block header: BFINAL=1/0, BTYPE=00 (stored)
        output.push(if is_final { 0x01 } else { 0x00 });
        
        // LEN and NLEN
        let len = block_size as u16;
        let nlen = !len;
        output.extend_from_slice(&len.to_le_bytes());
        output.extend_from_slice(&nlen.to_le_bytes());
        
        // Data
        output.extend_from_slice(&data[offset..offset + block_size]);
        offset += block_size;
    }
    
    Ok(())
}

/// Simple deflate decompression.
fn decompress_deflate_simple(data: &[u8]) -> CompressionResult<Vec<u8>> {
    let mut output = Vec::new();
    let mut offset = 0;
    
    while offset < data.len() {
        if offset >= data.len() {
            break;
        }
        
        let header = data[offset];
        let bfinal = header & 0x01;
        let btype = (header >> 1) & 0x03;
        offset += 1;
        
        match btype {
            0 => {
                // Stored block
                if offset + 4 > data.len() {
                    return Err(CompressionError::InvalidData("Truncated stored block".into()));
                }
                
                let len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
                let nlen = u16::from_le_bytes([data[offset + 2], data[offset + 3]]);
                offset += 4;
                
                if !len as u16 != nlen {
                    return Err(CompressionError::InvalidData("Invalid stored block length".into()));
                }
                
                if offset + len > data.len() {
                    return Err(CompressionError::InvalidData("Truncated stored block data".into()));
                }
                
                output.extend_from_slice(&data[offset..offset + len]);
                offset += len;
            }
            1 | 2 => {
                // Fixed or dynamic Huffman - not implemented in simple version
                return Err(CompressionError::InvalidData(
                    "Huffman compression not supported in simple mode".into()
                ));
            }
            _ => {
                return Err(CompressionError::InvalidData("Invalid block type".into()));
            }
        }
        
        if bfinal != 0 {
            break;
        }
    }
    
    Ok(output)
}

/// CRC32 checksum (IEEE polynomial).
pub fn crc32(data: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = generate_crc32_table();
    
    let mut crc = 0xFFFFFFFF_u32;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC32_TABLE[index] ^ (crc >> 8);
    }
    !crc
}

const fn generate_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = 0xEDB88320 ^ (crc >> 1);
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// Adler-32 checksum.
pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    
    for &byte in data {
        a = (a + byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    
    (b << 16) | a
}

// =============================================================================
// Streaming API
// =============================================================================

/// Gzip encoder (streaming).
pub struct GzipEncoder<W: Write> {
    inner: W,
    buffer: Vec<u8>,
    crc: u32,
    size: u32,
    header_written: bool,
}

impl<W: Write> GzipEncoder<W> {
    /// Create a new gzip encoder.
    pub fn new(inner: W, _level: CompressionLevel) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            crc: 0xFFFFFFFF,
            size: 0,
            header_written: false,
        }
    }
    
    /// Finish compression and return the inner writer.
    pub fn finish(mut self) -> io::Result<W> {
        self.flush_buffer(true)?;
        
        // Write trailer
        let crc = !self.crc;
        self.inner.write_all(&crc.to_le_bytes())?;
        self.inner.write_all(&self.size.to_le_bytes())?;
        
        Ok(self.inner)
    }
    
    fn write_header(&mut self) -> io::Result<()> {
        if !self.header_written {
            self.inner.write_all(&[0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff])?;
            self.header_written = true;
        }
        Ok(())
    }
    
    fn flush_buffer(&mut self, is_final: bool) -> io::Result<()> {
        if self.buffer.is_empty() && !is_final {
            return Ok(());
        }
        
        self.write_header()?;
        
        // Write stored block
        let header = if is_final { 0x01 } else { 0x00 };
        self.inner.write_all(&[header])?;
        
        let len = self.buffer.len() as u16;
        let nlen = !len;
        self.inner.write_all(&len.to_le_bytes())?;
        self.inner.write_all(&nlen.to_le_bytes())?;
        self.inner.write_all(&self.buffer)?;
        
        self.buffer.clear();
        Ok(())
    }
    
    fn update_crc(&mut self, data: &[u8]) {
        const CRC32_TABLE: [u32; 256] = generate_crc32_table();
        for &byte in data {
            let index = ((self.crc ^ byte as u32) & 0xFF) as usize;
            self.crc = CRC32_TABLE[index] ^ (self.crc >> 8);
        }
    }
}

impl<W: Write> Write for GzipEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.update_crc(buf);
        self.size = self.size.wrapping_add(buf.len() as u32);
        self.buffer.extend_from_slice(buf);
        
        // Flush if buffer is large
        if self.buffer.len() >= 65535 {
            self.flush_buffer(false)?;
        }
        
        Ok(buf.len())
    }
    
    fn flush(&mut self) -> io::Result<()> {
        self.flush_buffer(false)?;
        self.inner.flush()
    }
}

/// Gzip decoder (streaming).
pub struct GzipDecoder<R: Read> {
    inner: R,
    buffer: Vec<u8>,
    position: usize,
    finished: bool,
}

impl<R: Read> GzipDecoder<R> {
    /// Create a new gzip decoder.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            position: 0,
            finished: false,
        }
    }
    
    fn fill_buffer(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        
        let mut all_data = Vec::new();
        self.inner.read_to_end(&mut all_data)?;
        
        if all_data.len() < 18 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Data too short"));
        }
        
        // Verify gzip header
        if all_data[0] != 0x1f || all_data[1] != 0x8b {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid gzip header"));
        }
        
        // Decompress
        let payload = &all_data[10..all_data.len()-8];
        self.buffer = decompress_deflate_simple(payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        
        self.finished = true;
        Ok(())
    }
}

impl<R: Read> Read for GzipDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() && !self.finished {
            self.fill_buffer()?;
        }
        
        if self.position >= self.buffer.len() {
            return Ok(0);
        }
        
        let available = &self.buffer[self.position..];
        let to_copy = available.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&available[..to_copy]);
        self.position += to_copy;
        
        Ok(to_copy)
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Compress data using gzip.
pub fn gzip_compress(data: &[u8]) -> CompressionResult<Vec<u8>> {
    compress(data, CompressionLevel::Default, CompressionFormat::Gzip)
}

/// Decompress gzip data.
pub fn gzip_decompress(data: &[u8]) -> CompressionResult<Vec<u8>> {
    decompress(data, CompressionFormat::Gzip)
}

/// Compress data using zlib.
pub fn zlib_compress(data: &[u8]) -> CompressionResult<Vec<u8>> {
    compress(data, CompressionLevel::Default, CompressionFormat::Zlib)
}

/// Decompress zlib data.
pub fn zlib_decompress(data: &[u8]) -> CompressionResult<Vec<u8>> {
    decompress(data, CompressionFormat::Zlib)
}

/// Compress data using raw deflate.
pub fn deflate_compress(data: &[u8]) -> CompressionResult<Vec<u8>> {
    compress(data, CompressionLevel::Default, CompressionFormat::Deflate)
}

/// Decompress raw deflate data.
pub fn deflate_decompress(data: &[u8]) -> CompressionResult<Vec<u8>> {
    decompress(data, CompressionFormat::Deflate)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_crc32() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"hello"), 0x3610A686);
    }
    
    #[test]
    fn test_adler32() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"hello"), 0x062C0215);
    }
    
    #[test]
    fn test_gzip_roundtrip() {
        let original = b"Hello, World! This is a test of gzip compression.";
        let compressed = gzip_compress(original).unwrap();
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }
    
    #[test]
    fn test_zlib_roundtrip() {
        let original = b"Hello, World! This is a test of zlib compression.";
        let compressed = zlib_compress(original).unwrap();
        let decompressed = zlib_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }
    
    #[test]
    fn test_deflate_roundtrip() {
        let original = b"Hello, World! This is a test of deflate compression.";
        let compressed = deflate_compress(original).unwrap();
        let decompressed = deflate_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }
    
    #[test]
    fn test_streaming_gzip() {
        use std::io::Write;
        
        let mut encoder = GzipEncoder::new(Vec::new(), CompressionLevel::Default);
        encoder.write_all(b"Hello, ").unwrap();
        encoder.write_all(b"World!").unwrap();
        let compressed = encoder.finish().unwrap();
        
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, b"Hello, World!");
    }
    
    #[test]
    fn test_compression_level() {
        assert_eq!(CompressionLevel::None.level(), 0);
        assert_eq!(CompressionLevel::Fast.level(), 1);
        assert_eq!(CompressionLevel::Best.level(), 9);
        assert_eq!(CompressionLevel::Default.level(), 6);
        assert_eq!(CompressionLevel::Custom(5).level(), 5);
        assert_eq!(CompressionLevel::Custom(100).level(), 9); // Clamped
    }
}
