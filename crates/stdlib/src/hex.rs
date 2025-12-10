//! Hexadecimal encoding and decoding.

/// Encode bytes to hex string.
pub fn encode(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    
    let mut result = String::with_capacity(data.len() * 2);
    for byte in data {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0xf) as usize] as char);
    }
    result
}

/// Encode bytes to uppercase hex string.
pub fn encode_upper(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    
    let mut result = String::with_capacity(data.len() * 2);
    for byte in data {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0xf) as usize] as char);
    }
    result
}

/// Decode hex string to bytes.
pub fn decode(data: &str) -> Result<Vec<u8>, DecodeError> {
    let data = data.trim();
    
    // Remove optional 0x prefix
    let data = data.strip_prefix("0x").or(data.strip_prefix("0X")).unwrap_or(data);
    
    if data.len() % 2 != 0 {
        return Err(DecodeError::InvalidLength);
    }
    
    let mut result = Vec::with_capacity(data.len() / 2);
    
    let chars: Vec<char> = data.chars().collect();
    for chunk in chars.chunks(2) {
        let high = hex_digit(chunk[0])?;
        let low = hex_digit(chunk[1])?;
        result.push((high << 4) | low);
    }
    
    Ok(result)
}

fn hex_digit(c: char) -> Result<u8, DecodeError> {
    match c {
        '0'..='9' => Ok(c as u8 - b'0'),
        'a'..='f' => Ok(c as u8 - b'a' + 10),
        'A'..='F' => Ok(c as u8 - b'A' + 10),
        _ => Err(DecodeError::InvalidCharacter(c)),
    }
}

/// Encode string to hex.
pub fn encode_str(s: &str) -> String {
    encode(s.as_bytes())
}

/// Decode hex to string.
pub fn decode_str(data: &str) -> Result<String, DecodeError> {
    let bytes = decode(data)?;
    String::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
}

/// Format bytes as hex dump.
pub fn dump(data: &[u8], bytes_per_line: usize) -> String {
    let mut result = String::new();
    
    for (offset, chunk) in data.chunks(bytes_per_line).enumerate() {
        // Offset
        result.push_str(&format!("{:08x}  ", offset * bytes_per_line));
        
        // Hex bytes
        for (i, byte) in chunk.iter().enumerate() {
            result.push_str(&format!("{:02x} ", byte));
            if i == bytes_per_line / 2 - 1 {
                result.push(' ');
            }
        }
        
        // Padding
        for _ in chunk.len()..bytes_per_line {
            result.push_str("   ");
        }
        if chunk.len() <= bytes_per_line / 2 {
            result.push(' ');
        }
        
        // ASCII
        result.push_str(" |");
        for byte in chunk {
            if *byte >= 0x20 && *byte < 0x7f {
                result.push(*byte as char);
            } else {
                result.push('.');
            }
        }
        result.push_str("|\n");
    }
    
    result
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
            DecodeError::InvalidCharacter(c) => write!(f, "Invalid hex character: {}", c),
            DecodeError::InvalidLength => write!(f, "Invalid hex length (must be even)"),
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
        assert_eq!(encode(b"\x00"), "00");
        assert_eq!(encode(b"\xff"), "ff");
        assert_eq!(encode(b"hello"), "68656c6c6f");
    }
    
    #[test]
    fn test_decode() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("00").unwrap(), b"\x00");
        assert_eq!(decode("ff").unwrap(), b"\xff");
        assert_eq!(decode("FF").unwrap(), b"\xff");
        assert_eq!(decode("68656c6c6f").unwrap(), b"hello");
        assert_eq!(decode("0x68656c6c6f").unwrap(), b"hello");
    }
    
    #[test]
    fn test_roundtrip() {
        let original = b"Hello, World!";
        let encoded = encode(original);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
    
    #[test]
    fn test_dump() {
        let data = b"Hello, World!";
        let dump = dump(data, 16);
        assert!(dump.contains("48 65 6c 6c"));
        assert!(dump.contains("|Hello, World!|"));
    }
}

