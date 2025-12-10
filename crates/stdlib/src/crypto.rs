//! Cryptography support for Roast.
//!
//! Provides encryption, hashing, and secure random number generation.

use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// Errors
// =============================================================================

#[derive(Debug, Clone)]
pub enum CryptoError {
    InvalidKeyLength,
    InvalidBlockSize,
    InvalidPadding,
    InvalidSignature,
    InvalidToken,
    TokenExpired,
    EncodingError(String),
    DecryptionFailed,
    EncryptionFailed,
    SignatureVerificationFailed,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::InvalidKeyLength => write!(f, "Invalid key length"),
            CryptoError::InvalidBlockSize => write!(f, "Invalid block size"),
            CryptoError::InvalidPadding => write!(f, "Invalid padding"),
            CryptoError::InvalidSignature => write!(f, "Invalid signature"),
            CryptoError::InvalidToken => write!(f, "Invalid token"),
            CryptoError::TokenExpired => write!(f, "Token expired"),
            CryptoError::EncodingError(s) => write!(f, "Encoding error: {}", s),
            CryptoError::DecryptionFailed => write!(f, "Decryption failed"),
            CryptoError::EncryptionFailed => write!(f, "Encryption failed"),
            CryptoError::SignatureVerificationFailed => write!(f, "Signature verification failed"),
        }
    }
}

impl std::error::Error for CryptoError {}

pub type CryptoResult<T> = Result<T, CryptoError>;

// =============================================================================
// Secure Random
// =============================================================================

/// Secure random number generator using system entropy.
pub struct SecureRandom {
    state: [u64; 4],
}

impl SecureRandom {
    /// Create a new secure random generator.
    pub fn new() -> Self {
        // Seed from system time and address space layout
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        
        Self {
            state: [
                seed as u64,
                (seed >> 64) as u64 ^ 0x5DEECE66D,
                seed as u64 ^ 0xB,
                (seed >> 32) as u64 ^ 0x12345678,
            ],
        }
    }
    
    /// Generate random bytes.
    pub fn bytes(&mut self, count: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(count);
        
        while result.len() < count {
            let val = self.next_u64();
            for i in 0..8 {
                if result.len() >= count {
                    break;
                }
                result.push(((val >> (i * 8)) & 0xFF) as u8);
            }
        }
        
        result
    }
    
    /// Generate a random u64.
    fn next_u64(&mut self) -> u64 {
        // xoshiro256**
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.state[1] << 17;
        
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        
        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);
        
        result
    }
    
    /// Generate a random integer in range [0, max).
    pub fn int(&mut self, max: u64) -> u64 {
        if max == 0 {
            return 0;
        }
        self.next_u64() % max
    }
    
    /// Generate a random alphanumeric string.
    pub fn alphanumeric(&mut self, length: usize) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let bytes = self.bytes(length);
        bytes.iter()
            .map(|&b| CHARS[(b as usize) % CHARS.len()] as char)
            .collect()
    }
    
    /// Generate a UUID v4.
    pub fn uuid(&mut self) -> String {
        let bytes = self.bytes(16);
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5],
            (bytes[6] & 0x0F) | 0x40, bytes[7],
            (bytes[8] & 0x3F) | 0x80, bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        )
    }
}

impl Default for SecureRandom {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Base64 Encoding
// =============================================================================

const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64_URL_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes to base64.
pub fn base64_encode(data: &[u8]) -> String {
    base64_encode_with(data, BASE64_CHARS, true)
}

/// Encode bytes to URL-safe base64.
pub fn base64_url_encode(data: &[u8]) -> String {
    base64_encode_with(data, BASE64_URL_CHARS, false)
}

fn base64_encode_with(data: &[u8], chars: &[u8], padding: bool) -> String {
    let mut result = Vec::with_capacity((data.len() + 2) / 3 * 4);
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        result.push(chars[b0 >> 2]);
        result.push(chars[((b0 & 0x03) << 4) | (b1 >> 4)]);
        
        if chunk.len() > 1 {
            result.push(chars[((b1 & 0x0F) << 2) | (b2 >> 6)]);
        } else if padding {
            result.push(b'=');
        }
        
        if chunk.len() > 2 {
            result.push(chars[b2 & 0x3F]);
        } else if padding {
            result.push(b'=');
        }
    }
    
    String::from_utf8(result).unwrap()
}

/// Decode base64 to bytes.
pub fn base64_decode(data: &str) -> CryptoResult<Vec<u8>> {
    base64_decode_with(data, BASE64_CHARS)
}

/// Decode URL-safe base64 to bytes.
pub fn base64_url_decode(data: &str) -> CryptoResult<Vec<u8>> {
    base64_decode_with(data, BASE64_URL_CHARS)
}

fn base64_decode_with(data: &str, chars: &[u8]) -> CryptoResult<Vec<u8>> {
    let data = data.trim_end_matches('=');
    let mut result = Vec::with_capacity(data.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    
    for byte in data.bytes() {
        let idx = chars.iter().position(|&c| c == byte)
            .ok_or_else(|| CryptoError::EncodingError("Invalid base64 character".into()))?;
        
        buffer = (buffer << 6) | (idx as u32);
        bits += 6;
        
        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
        }
    }
    
    Ok(result)
}

// =============================================================================
// Hex Encoding
// =============================================================================

/// Encode bytes to hex.
pub fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Decode hex to bytes.
pub fn hex_decode(data: &str) -> CryptoResult<Vec<u8>> {
    if data.len() % 2 != 0 {
        return Err(CryptoError::EncodingError("Invalid hex length".into()));
    }
    
    (0..data.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&data[i..i + 2], 16)
                .map_err(|_| CryptoError::EncodingError("Invalid hex character".into()))
        })
        .collect()
}

// =============================================================================
// HMAC
// =============================================================================

/// HMAC-SHA256 implementation.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;
    
    // Prepare key
    let key = if key.len() > BLOCK_SIZE {
        sha256(key).to_vec()
    } else {
        let mut k = key.to_vec();
        k.resize(BLOCK_SIZE, 0);
        k
    };
    
    let mut i_key_pad = [0u8; BLOCK_SIZE];
    let mut o_key_pad = [0u8; BLOCK_SIZE];
    
    for i in 0..BLOCK_SIZE {
        i_key_pad[i] = key.get(i).copied().unwrap_or(0) ^ 0x36;
        o_key_pad[i] = key.get(i).copied().unwrap_or(0) ^ 0x5C;
    }
    
    // Inner hash
    let mut inner = Vec::with_capacity(BLOCK_SIZE + message.len());
    inner.extend_from_slice(&i_key_pad);
    inner.extend_from_slice(message);
    let inner_hash = sha256(&inner);
    
    // Outer hash
    let mut outer = Vec::with_capacity(BLOCK_SIZE + 32);
    outer.extend_from_slice(&o_key_pad);
    outer.extend_from_slice(&inner_hash);
    sha256(&outer).to_vec()
}

// =============================================================================
// SHA-256 (Simplified Implementation)
// =============================================================================

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Compute SHA-256 hash.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    
    // Padding
    let ml = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    for i in (0..8).rev() {
        padded.push((ml >> (i * 8)) as u8);
    }
    
    // Process blocks
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        
        for i in 0..16 {
            w[i] = ((chunk[i * 4] as u32) << 24)
                | ((chunk[i * 4 + 1] as u32) << 16)
                | ((chunk[i * 4 + 2] as u32) << 8)
                | (chunk[i * 4 + 3] as u32);
        }
        
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];
        
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(SHA256_K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    
    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4] = (*val >> 24) as u8;
        result[i * 4 + 1] = (*val >> 16) as u8;
        result[i * 4 + 2] = (*val >> 8) as u8;
        result[i * 4 + 3] = *val as u8;
    }
    result
}

// =============================================================================
// AES (Simplified)
// =============================================================================

const AES_SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

const AES_INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

const RCON: [u8; 11] = [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36];

/// AES-128 cipher.
pub struct Aes128 {
    round_keys: [[u8; 16]; 11],
}

impl Aes128 {
    /// Create a new AES-128 cipher with the given key.
    pub fn new(key: &[u8]) -> CryptoResult<Self> {
        if key.len() != 16 {
            return Err(CryptoError::InvalidKeyLength);
        }
        
        let round_keys = Self::key_expansion(key);
        Ok(Self { round_keys })
    }
    
    fn key_expansion(key: &[u8]) -> [[u8; 16]; 11] {
        let mut w = [[0u8; 4]; 44];
        
        // First 4 words are the key
        for i in 0..4 {
            w[i] = [key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]];
        }
        
        // Generate remaining words
        for i in 4..44 {
            let mut temp = w[i - 1];
            if i % 4 == 0 {
                // RotWord
                temp = [temp[1], temp[2], temp[3], temp[0]];
                // SubWord
                temp = [
                    AES_SBOX[temp[0] as usize],
                    AES_SBOX[temp[1] as usize],
                    AES_SBOX[temp[2] as usize],
                    AES_SBOX[temp[3] as usize],
                ];
                temp[0] ^= RCON[i / 4];
            }
            for j in 0..4 {
                w[i][j] = w[i - 4][j] ^ temp[j];
            }
        }
        
        // Convert to round keys
        let mut round_keys = [[0u8; 16]; 11];
        for r in 0..11 {
            for i in 0..4 {
                for j in 0..4 {
                    round_keys[r][i * 4 + j] = w[r * 4 + i][j];
                }
            }
        }
        round_keys
    }
    
    /// Encrypt a 16-byte block.
    pub fn encrypt_block(&self, block: &[u8]) -> CryptoResult<[u8; 16]> {
        if block.len() != 16 {
            return Err(CryptoError::InvalidBlockSize);
        }
        
        let mut state = [[0u8; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                state[j][i] = block[i * 4 + j];
            }
        }
        
        // Initial round key addition
        self.add_round_key(&mut state, 0);
        
        // Main rounds
        for round in 1..10 {
            self.sub_bytes(&mut state);
            self.shift_rows(&mut state);
            self.mix_columns(&mut state);
            self.add_round_key(&mut state, round);
        }
        
        // Final round
        self.sub_bytes(&mut state);
        self.shift_rows(&mut state);
        self.add_round_key(&mut state, 10);
        
        // Convert state to output
        let mut output = [0u8; 16];
        for i in 0..4 {
            for j in 0..4 {
                output[i * 4 + j] = state[j][i];
            }
        }
        Ok(output)
    }
    
    /// Decrypt a 16-byte block.
    pub fn decrypt_block(&self, block: &[u8]) -> CryptoResult<[u8; 16]> {
        if block.len() != 16 {
            return Err(CryptoError::InvalidBlockSize);
        }
        
        let mut state = [[0u8; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                state[j][i] = block[i * 4 + j];
            }
        }
        
        // Initial round key addition
        self.add_round_key(&mut state, 10);
        
        // Main rounds
        for round in (1..10).rev() {
            self.inv_shift_rows(&mut state);
            self.inv_sub_bytes(&mut state);
            self.add_round_key(&mut state, round);
            self.inv_mix_columns(&mut state);
        }
        
        // Final round
        self.inv_shift_rows(&mut state);
        self.inv_sub_bytes(&mut state);
        self.add_round_key(&mut state, 0);
        
        // Convert state to output
        let mut output = [0u8; 16];
        for i in 0..4 {
            for j in 0..4 {
                output[i * 4 + j] = state[j][i];
            }
        }
        Ok(output)
    }
    
    fn sub_bytes(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            for j in 0..4 {
                state[i][j] = AES_SBOX[state[i][j] as usize];
            }
        }
    }
    
    fn inv_sub_bytes(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            for j in 0..4 {
                state[i][j] = AES_INV_SBOX[state[i][j] as usize];
            }
        }
    }
    
    fn shift_rows(&self, state: &mut [[u8; 4]; 4]) {
        // Row 1: shift left by 1
        let temp = state[1][0];
        state[1][0] = state[1][1];
        state[1][1] = state[1][2];
        state[1][2] = state[1][3];
        state[1][3] = temp;
        
        // Row 2: shift left by 2
        let temp = [state[2][0], state[2][1]];
        state[2][0] = state[2][2];
        state[2][1] = state[2][3];
        state[2][2] = temp[0];
        state[2][3] = temp[1];
        
        // Row 3: shift left by 3 (= shift right by 1)
        let temp = state[3][3];
        state[3][3] = state[3][2];
        state[3][2] = state[3][1];
        state[3][1] = state[3][0];
        state[3][0] = temp;
    }
    
    fn inv_shift_rows(&self, state: &mut [[u8; 4]; 4]) {
        // Row 1: shift right by 1
        let temp = state[1][3];
        state[1][3] = state[1][2];
        state[1][2] = state[1][1];
        state[1][1] = state[1][0];
        state[1][0] = temp;
        
        // Row 2: shift right by 2
        let temp = [state[2][2], state[2][3]];
        state[2][2] = state[2][0];
        state[2][3] = state[2][1];
        state[2][0] = temp[0];
        state[2][1] = temp[1];
        
        // Row 3: shift right by 3 (= shift left by 1)
        let temp = state[3][0];
        state[3][0] = state[3][1];
        state[3][1] = state[3][2];
        state[3][2] = state[3][3];
        state[3][3] = temp;
    }
    
    fn mix_columns(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            let a = state[0][i];
            let b = state[1][i];
            let c = state[2][i];
            let d = state[3][i];
            
            state[0][i] = gf_mul(a, 2) ^ gf_mul(b, 3) ^ c ^ d;
            state[1][i] = a ^ gf_mul(b, 2) ^ gf_mul(c, 3) ^ d;
            state[2][i] = a ^ b ^ gf_mul(c, 2) ^ gf_mul(d, 3);
            state[3][i] = gf_mul(a, 3) ^ b ^ c ^ gf_mul(d, 2);
        }
    }
    
    fn inv_mix_columns(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            let a = state[0][i];
            let b = state[1][i];
            let c = state[2][i];
            let d = state[3][i];
            
            state[0][i] = gf_mul(a, 14) ^ gf_mul(b, 11) ^ gf_mul(c, 13) ^ gf_mul(d, 9);
            state[1][i] = gf_mul(a, 9) ^ gf_mul(b, 14) ^ gf_mul(c, 11) ^ gf_mul(d, 13);
            state[2][i] = gf_mul(a, 13) ^ gf_mul(b, 9) ^ gf_mul(c, 14) ^ gf_mul(d, 11);
            state[3][i] = gf_mul(a, 11) ^ gf_mul(b, 13) ^ gf_mul(c, 9) ^ gf_mul(d, 14);
        }
    }
    
    fn add_round_key(&self, state: &mut [[u8; 4]; 4], round: usize) {
        let key = &self.round_keys[round];
        for i in 0..4 {
            for j in 0..4 {
                state[j][i] ^= key[i * 4 + j];
            }
        }
    }
}

fn gf_mul(a: u8, b: u8) -> u8 {
    let mut result = 0u8;
    let mut a = a;
    let mut b = b;
    
    while b != 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        if a & 0x80 != 0 {
            a = (a << 1) ^ 0x1B;
        } else {
            a <<= 1;
        }
        b >>= 1;
    }
    result
}

/// AES-CBC encryption mode.
pub struct AesCbc {
    cipher: Aes128,
    iv: [u8; 16],
}

impl AesCbc {
    /// Create new AES-CBC with key and IV.
    pub fn new(key: &[u8], iv: &[u8]) -> CryptoResult<Self> {
        if iv.len() != 16 {
            return Err(CryptoError::InvalidBlockSize);
        }
        
        let cipher = Aes128::new(key)?;
        let mut iv_arr = [0u8; 16];
        iv_arr.copy_from_slice(iv);
        
        Ok(Self { cipher, iv: iv_arr })
    }
    
    /// Encrypt data with PKCS7 padding.
    pub fn encrypt(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        // PKCS7 padding
        let pad_len = 16 - (data.len() % 16);
        let mut padded = data.to_vec();
        for _ in 0..pad_len {
            padded.push(pad_len as u8);
        }
        
        let mut result = Vec::with_capacity(padded.len());
        let mut prev = self.iv;
        
        for chunk in padded.chunks(16) {
            let mut block = [0u8; 16];
            for i in 0..16 {
                block[i] = chunk[i] ^ prev[i];
            }
            prev = self.cipher.encrypt_block(&block)?;
            result.extend_from_slice(&prev);
        }
        
        Ok(result)
    }
    
    /// Decrypt data and remove PKCS7 padding.
    pub fn decrypt(&self, data: &[u8]) -> CryptoResult<Vec<u8>> {
        if data.len() % 16 != 0 {
            return Err(CryptoError::InvalidBlockSize);
        }
        
        let mut result = Vec::with_capacity(data.len());
        let mut prev = self.iv;
        
        for chunk in data.chunks(16) {
            let decrypted = self.cipher.decrypt_block(chunk)?;
            for i in 0..16 {
                result.push(decrypted[i] ^ prev[i]);
            }
            prev.copy_from_slice(chunk);
        }
        
        // Remove PKCS7 padding
        if let Some(&pad_len) = result.last() {
            if pad_len as usize <= 16 && pad_len > 0 {
                let padding_valid = result.iter().rev().take(pad_len as usize).all(|&b| b == pad_len);
                if padding_valid {
                    result.truncate(result.len() - pad_len as usize);
                } else {
                    return Err(CryptoError::InvalidPadding);
                }
            }
        }
        
        Ok(result)
    }
}

// =============================================================================
// JWT (JSON Web Tokens)
// =============================================================================

/// JWT claims.
#[derive(Clone, Debug)]
pub struct JwtClaims {
    pub data: HashMap<String, String>,
}

impl JwtClaims {
    pub fn new() -> Self {
        Self { data: HashMap::new() }
    }
    
    pub fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
    
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }
    
    pub fn set_exp(&mut self, timestamp: u64) {
        self.data.insert("exp".to_string(), timestamp.to_string());
    }
    
    pub fn set_sub(&mut self, subject: &str) {
        self.data.insert("sub".to_string(), subject.to_string());
    }
    
    pub fn set_iat(&mut self, timestamp: u64) {
        self.data.insert("iat".to_string(), timestamp.to_string());
    }
}

impl Default for JwtClaims {
    fn default() -> Self {
        Self::new()
    }
}

/// JWT encoder/decoder using HS256.
pub struct Jwt {
    secret: Vec<u8>,
}

impl Jwt {
    /// Create a new JWT handler with the given secret.
    pub fn new(secret: &[u8]) -> Self {
        Self { secret: secret.to_vec() }
    }
    
    /// Encode claims to a JWT string.
    pub fn encode(&self, claims: &JwtClaims) -> String {
        // Header
        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let header_b64 = base64_url_encode(header.as_bytes());
        
        // Payload
        let payload = claims_to_json(claims);
        let payload_b64 = base64_url_encode(payload.as_bytes());
        
        // Signature
        let message = format!("{}.{}", header_b64, payload_b64);
        let signature = hmac_sha256(&self.secret, message.as_bytes());
        let signature_b64 = base64_url_encode(&signature);
        
        format!("{}.{}.{}", header_b64, payload_b64, signature_b64)
    }
    
    /// Decode and verify a JWT string.
    pub fn decode(&self, token: &str) -> CryptoResult<JwtClaims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(CryptoError::InvalidToken);
        }
        
        let message = format!("{}.{}", parts[0], parts[1]);
        let expected_sig = hmac_sha256(&self.secret, message.as_bytes());
        let actual_sig = base64_url_decode(parts[2])?;
        
        if expected_sig.as_slice() != actual_sig.as_slice() {
            return Err(CryptoError::InvalidSignature);
        }
        
        let payload = base64_url_decode(parts[1])?;
        let payload_str = String::from_utf8(payload)
            .map_err(|_| CryptoError::EncodingError("Invalid UTF-8".into()))?;
        
        let claims = json_to_claims(&payload_str)?;
        
        // Check expiration
        if let Some(exp) = claims.get("exp") {
            if let Ok(exp_time) = exp.parse::<u64>() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now > exp_time {
                    return Err(CryptoError::TokenExpired);
                }
            }
        }
        
        Ok(claims)
    }
}

fn claims_to_json(claims: &JwtClaims) -> String {
    let pairs: Vec<String> = claims.data.iter()
        .map(|(k, v)| format!(r#""{}":"{}""#, k, v))
        .collect();
    format!("{{{}}}", pairs.join(","))
}

fn json_to_claims(json: &str) -> CryptoResult<JwtClaims> {
    let json = json.trim().trim_start_matches('{').trim_end_matches('}');
    let mut claims = JwtClaims::new();
    
    for pair in json.split(',') {
        let pair = pair.trim();
        if let Some(colon) = pair.find(':') {
            let key = pair[..colon].trim().trim_matches('"');
            let value = pair[colon + 1..].trim().trim_matches('"');
            claims.data.insert(key.to_string(), value.to_string());
        }
    }
    
    Ok(claims)
}

// =============================================================================
// Password Hashing
// =============================================================================

/// PBKDF2-HMAC-SHA256 key derivation.
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, key_len: usize) -> Vec<u8> {
    let mut result = Vec::new();
    let mut block_num = 1u32;
    
    while result.len() < key_len {
        let mut u = {
            let mut msg = salt.to_vec();
            msg.extend_from_slice(&block_num.to_be_bytes());
            hmac_sha256(password, &msg)
        };
        let mut block = u.clone();
        
        for _ in 1..iterations {
            u = hmac_sha256(password, &u);
            for i in 0..block.len() {
                block[i] ^= u[i];
            }
        }
        
        result.extend_from_slice(&block);
        block_num += 1;
    }
    
    result.truncate(key_len);
    result
}

/// Hash a password for storage.
pub fn hash_password(password: &str) -> String {
    let mut rng = SecureRandom::new();
    let salt = rng.bytes(16);
    let hash = pbkdf2_sha256(password.as_bytes(), &salt, 100_000, 32);
    
    format!("$pbkdf2-sha256$100000${}${}", 
        base64_encode(&salt),
        base64_encode(&hash))
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    let parts: Vec<&str> = hash.split('$').filter(|s| !s.is_empty()).collect();
    if parts.len() != 4 || parts[0] != "pbkdf2-sha256" {
        return false;
    }
    
    let iterations: u32 = parts[1].parse().unwrap_or(0);
    let salt = base64_decode(parts[2]).unwrap_or_default();
    let expected_hash = base64_decode(parts[3]).unwrap_or_default();
    
    let computed_hash = pbkdf2_sha256(password.as_bytes(), &salt, iterations, 32);
    
    // Constant-time comparison
    if computed_hash.len() != expected_hash.len() {
        return false;
    }
    
    let mut result = 0u8;
    for (a, b) in computed_hash.iter().zip(expected_hash.iter()) {
        result |= a ^ b;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_base64() {
        let data = b"Hello, World!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
    
    #[test]
    fn test_hex() {
        let data = b"\x00\x01\x02\xff";
        let encoded = hex_encode(data);
        assert_eq!(encoded, "000102ff");
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
    
    #[test]
    fn test_sha256() {
        let hash = sha256(b"hello");
        let expected = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert_eq!(hex_encode(&hash), expected);
    }
    
    #[test]
    fn test_aes_128() {
        let key = b"0123456789abcdef";
        let aes = Aes128::new(key).unwrap();
        
        let plaintext = b"Test message 123";
        let ciphertext = aes.encrypt_block(plaintext).unwrap();
        let decrypted = aes.decrypt_block(&ciphertext).unwrap();
        
        assert_eq!(&decrypted, plaintext);
    }
    
    #[test]
    fn test_aes_cbc() {
        let key = b"0123456789abcdef";
        let iv = b"fedcba9876543210";
        let aes = AesCbc::new(key, iv).unwrap();
        
        let plaintext = b"Hello, World! This is a test message.";
        let ciphertext = aes.encrypt(plaintext).unwrap();
        let decrypted = aes.decrypt(&ciphertext).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }
    
    #[test]
    fn test_jwt() {
        let jwt = Jwt::new(b"secret-key");
        
        let mut claims = JwtClaims::new();
        claims.set_sub("user123");
        claims.set("name", "John Doe");
        
        let token = jwt.encode(&claims);
        let decoded = jwt.decode(&token).unwrap();
        
        assert_eq!(decoded.get("sub"), Some("user123"));
        assert_eq!(decoded.get("name"), Some("John Doe"));
    }
    
    #[test]
    fn test_password_hashing() {
        let password = "my_secure_password";
        let hash = hash_password(password);
        
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }
    
    #[test]
    fn test_secure_random() {
        let mut rng = SecureRandom::new();
        
        let bytes = rng.bytes(32);
        assert_eq!(bytes.len(), 32);
        
        let uuid = rng.uuid();
        assert_eq!(uuid.len(), 36);
        assert!(uuid.contains('-'));
    }
}

