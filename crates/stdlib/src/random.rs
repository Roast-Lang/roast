//! Random number generation.
//!
//! Pseudorandom number generators.

use std::time::{SystemTime, UNIX_EPOCH};

/// Xorshift64 PRNG.
pub struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// Create with a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 1 } else { seed } }
    }
    
    /// Create with a random seed.
    pub fn new_seeded() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::new(seed)
    }
    
    /// Generate the next random u64.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    
    /// Generate a random u32.
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    
    /// Generate a random f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64)
    }
    
    /// Generate a random f64 in [min, max).
    pub fn next_f64_range(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }
    
    /// Generate a random integer in [0, n).
    pub fn next_usize(&mut self, n: usize) -> usize {
        (self.next_u64() as usize) % n
    }
    
    /// Generate a random integer in [min, max).
    pub fn next_range(&mut self, min: i64, max: i64) -> i64 {
        let range = (max - min) as u64;
        min + (self.next_u64() % range) as i64
    }
    
    /// Generate a random bool.
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
    
    /// Generate a random bool with probability p.
    pub fn next_bool_weighted(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
    
    /// Choose a random element from a slice.
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.next_usize(items.len())])
        }
    }
    
    /// Shuffle a slice in place.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.next_usize(i + 1);
            items.swap(i, j);
        }
    }
    
    /// Generate n random bytes.
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(n);
        let mut i = 0;
        
        while i < n {
            let x = self.next_u64();
            for j in 0..8 {
                if i + j < n {
                    result.push((x >> (j * 8)) as u8);
                }
            }
            i += 8;
        }
        
        result.truncate(n);
        result
    }
}

impl Default for Xorshift64 {
    fn default() -> Self {
        Self::new_seeded()
    }
}

/// PCG (Permuted Congruential Generator) - higher quality.
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    /// Create with a seed.
    pub fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }
    
    /// Create with a random seed.
    pub fn new_seeded() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::new(seed)
    }
    
    /// Generate the next random u32.
    pub fn next_u32(&mut self) -> u32 {
        let old_state = self.state;
        self.state = old_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(self.inc);
        
        let xorshifted = (((old_state >> 18) ^ old_state) >> 27) as u32;
        let rot = (old_state >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
    
    /// Generate a random u64.
    pub fn next_u64(&mut self) -> u64 {
        ((self.next_u32() as u64) << 32) | (self.next_u32() as u64)
    }
    
    /// Generate a random f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u32() as f64) / (u32::MAX as f64)
    }
    
    /// Generate a random integer in [min, max).
    pub fn next_range(&mut self, min: i32, max: i32) -> i32 {
        let range = (max - min) as u32;
        min + (self.next_u32() % range) as i32
    }
}

impl Default for Pcg32 {
    fn default() -> Self {
        Self::new_seeded()
    }
}

/// Thread-local random number generator.
pub fn random() -> u64 {
    thread_local! {
        static RNG: std::cell::RefCell<Xorshift64> = std::cell::RefCell::new(Xorshift64::new_seeded());
    }
    
    RNG.with(|rng| rng.borrow_mut().next_u64())
}

/// Random f64 in [0, 1).
pub fn random_f64() -> f64 {
    (random() as f64) / (u64::MAX as f64)
}

/// Random integer in [min, max).
pub fn random_range(min: i64, max: i64) -> i64 {
    let range = (max - min) as u64;
    min + (random() % range) as i64
}

/// Random choice from a slice.
pub fn choice<T>(items: &[T]) -> Option<&T> {
    if items.is_empty() {
        None
    } else {
        Some(&items[(random() as usize) % items.len()])
    }
}

/// Shuffle a slice.
pub fn shuffle<T>(items: &mut [T]) {
    thread_local! {
        static RNG: std::cell::RefCell<Xorshift64> = std::cell::RefCell::new(Xorshift64::new_seeded());
    }
    
    RNG.with(|rng| rng.borrow_mut().shuffle(items));
}

/// Generate a random UUID v4.
pub fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    let mut rng = Xorshift64::new_seeded();
    
    for i in 0..2 {
        let x = rng.next_u64();
        for j in 0..8 {
            bytes[i * 8 + j] = (x >> (j * 8)) as u8;
        }
    }
    
    // Set version (4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_xorshift() {
        let mut rng = Xorshift64::new(12345);
        
        // Should produce different values
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b);
        
        // Same seed should produce same sequence
        let mut rng2 = Xorshift64::new(12345);
        assert_eq!(a, rng2.next_u64());
    }
    
    #[test]
    fn test_range() {
        let mut rng = Xorshift64::new(12345);
        
        for _ in 0..100 {
            let n = rng.next_range(10, 20);
            assert!(n >= 10 && n < 20);
        }
    }
    
    #[test]
    fn test_shuffle() {
        let mut rng = Xorshift64::new(12345);
        let mut items = vec![1, 2, 3, 4, 5];
        let original = items.clone();
        
        rng.shuffle(&mut items);
        
        // Should still contain same elements
        items.sort();
        assert_eq!(items, original);
    }
    
    #[test]
    fn test_uuid() {
        let uuid = uuid_v4();
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4"); // Version 4
    }
}

