//! Base64 encoding and decoding.

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes to base64 string.
pub fn encode(data: &[u8]) -> String {
    encode_with_alphabet(data, ALPHABET, true)
}

/// Encode bytes to URL-safe base64.
pub fn encode_url(data: &[u8]) -> String {
    encode_with_alphabet(data, URL_ALPHABET, false)
}

fn encode_with_alphabet(data: &[u8], alphabet: &[u8], padding: bool) -> String {
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let n = match chunk.len() {
            1 => ((chunk[0] as u32) << 16, 2),
            2 => (((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8), 3),
            3 => (((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32), 4),
            _ => unreachable!(),
        };
        
        for i in 0..n.1 {
            let idx = ((n.0 >> (18 - 6 * i)) & 0x3F) as usize;
            result.push(alphabet[idx] as char);
        }
        
        if padding {
            for _ in n.1..4 {
                result.push('=');
            }
        }
    }
    
    result
}

/// Decode base64 string to bytes.
pub fn decode(data: &str) -> Result<Vec<u8>, DecodeError> {
    decode_with_alphabet(data, ALPHABET)
}

/// Decode URL-safe base64 string.
pub fn decode_url(data: &str) -> Result<Vec<u8>, DecodeError> {
    decode_with_alphabet(data, URL_ALPHABET)
}

fn decode_with_alphabet(data: &str, alphabet: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let data = data.trim_end_matches('=');
    let mut result = Vec::new();
    
    let mut decode_table = [0i8; 128];
    for (i, &c) in alphabet.iter().enumerate() {
        decode_table[c as usize] = i as i8;
    }
    
    let bytes: Vec<u8> = data.bytes().collect();
    
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        let mut count = 0;
        
        for &b in chunk {
            if b >= 128 {
                return Err(DecodeError::InvalidCharacter(b as char));
            }
            let val = decode_table[b as usize];
            if val < 0 && b != b'=' {
                return Err(DecodeError::InvalidCharacter(b as char));
            }
            if val >= 0 {
                n = (n << 6) | (val as u32);
                count += 1;
            }
        }
        
        match count {
            2 => result.push((n >> 4) as u8),
            3 => {
                result.push((n >> 10) as u8);
                result.push((n >> 2) as u8);
            }
            4 => {
                result.push((n >> 16) as u8);
                result.push((n >> 8) as u8);
                result.push(n as u8);
            }
            _ => {}
        }
    }
    
    Ok(result)
}

/// Encode string to base64.
pub fn encode_str(s: &str) -> String {
    encode(s.as_bytes())
}

/// Decode base64 to string.
pub fn decode_str(data: &str) -> Result<String, DecodeError> {
    let bytes = decode(data)?;
    String::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
}

/// Decode error.
#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidCharacter(char),
    InvalidLength,
    InvalidUtf8,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::InvalidCharacter(c) => write!(f, "Invalid base64 character: {}", c),
            DecodeError::InvalidLength => write!(f, "Invalid base64 length"),
            DecodeError::InvalidUtf8 => write!(f, "Invalid UTF-8 in decoded data"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encode() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }
    
    #[test]
    fn test_decode() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
    }
    
    #[test]
    fn test_roundtrip() {
        let original = b"Hello, World!";
        let encoded = encode(original);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
    
    #[test]
    fn test_url_safe() {
        let data = b"\xfb\xff\xfe";
        let encoded = encode_url(data);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        
        let decoded = decode_url(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}

