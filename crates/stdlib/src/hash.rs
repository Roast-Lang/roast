//! Hashing utilities.
//!
//! Hash functions and hashers.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Calculate the default hash of a value.
pub fn hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// FNV-1a hash (fast, non-cryptographic).
pub fn fnv1a(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    
    let mut hash = FNV_OFFSET;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// FNV-1a hash for strings.
pub fn fnv1a_str(s: &str) -> u64 {
    fnv1a(s.as_bytes())
}

/// DJB2 hash (simple, fast).
pub fn djb2(data: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(*byte as u64);
    }
    hash
}

/// SDBM hash.
pub fn sdbm(data: &[u8]) -> u64 {
    let mut hash: u64 = 0;
    for byte in data {
        hash = (*byte as u64)
            .wrapping_add(hash.wrapping_shl(6))
            .wrapping_add(hash.wrapping_shl(16))
            .wrapping_sub(hash);
    }
    hash
}

/// Simple 32-bit hash.
pub fn hash32(data: &[u8]) -> u32 {
    let h = fnv1a(data);
    ((h >> 32) as u32) ^ (h as u32)
}

/// MurmurHash3 finalizer (for mixing).
pub fn mix64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
}

/// Combine two hashes.
pub fn combine(h1: u64, h2: u64) -> u64 {
    // Boost's hash_combine approach
    h1 ^ (h2.wrapping_add(0x9e3779b9).wrapping_add(h1 << 6).wrapping_add(h1 >> 2))
}

/// Incremental hasher.
pub struct IncrementalHasher {
    state: u64,
}

impl IncrementalHasher {
    /// Create a new hasher.
    pub fn new() -> Self {
        Self { state: 0xcbf29ce484222325 }
    }
    
    /// Update with bytes.
    pub fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.state ^= *byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }
    
    /// Update with a string.
    pub fn update_str(&mut self, s: &str) {
        self.update(s.as_bytes());
    }
    
    /// Update with a u64.
    pub fn update_u64(&mut self, n: u64) {
        self.update(&n.to_le_bytes());
    }
    
    /// Get the current hash.
    pub fn finish(&self) -> u64 {
        self.state
    }
    
    /// Reset the hasher.
    pub fn reset(&mut self) {
        self.state = 0xcbf29ce484222325;
    }
}

impl Default for IncrementalHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// CRC32 checksum.
pub fn crc32(data: &[u8]) -> u32 {
    const CRC_TABLE: [u32; 256] = generate_crc_table();
    
    let mut crc = 0xffffffff;
    for byte in data {
        let index = ((crc ^ (*byte as u32)) & 0xff) as usize;
        crc = CRC_TABLE[index] ^ (crc >> 8);
    }
    !crc
}

const fn generate_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 == 1 {
                crc = 0xedb88320 ^ (crc >> 1);
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
    
    for byte in data {
        a = (a + *byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fnv1a() {
        assert_ne!(fnv1a(b"hello"), fnv1a(b"world"));
        assert_eq!(fnv1a(b"hello"), fnv1a(b"hello"));
    }
    
    #[test]
    fn test_crc32() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
    }
    
    #[test]
    fn test_incremental() {
        let mut h = IncrementalHasher::new();
        h.update(b"hello ");
        h.update(b"world");
        
        assert_eq!(h.finish(), fnv1a(b"hello world"));
    }
}

