//! UUID generation module for Roast
//!
//! Provides UUID v4 (random) and v7 (time-based) generation.

/// UUID generation and manipulation
pub const ROAST_UUID_SOURCE: &str = r##"
"""
UUID generation module.

Provides UUID generation for v4 (random) and v7 (time-sorted).
"""

import random
import time


class UUID:
    """UUID representation."""
    bytes: list[int]
    version: int
    
    def __init__(self, bytes: list[int] = [], version: int = 4) -> None:
        self.bytes = bytes if len(bytes) == 16 else [0] * 16
        self.version = version
    
    def __repr__(self) -> str:
        return self.to_string()
    
    def to_string(self) -> str:
        """Convert UUID to standard string format (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)."""
        hex_chars: str = "0123456789abcdef"
        result: str = ""
        
        for i in range(16):
            b: int = self.bytes[i]
            result = result + hex_chars[b // 16] + hex_chars[b % 16]
            if i == 3 or i == 5 or i == 7 or i == 9:
                result = result + "-"
        
        return result
    
    def to_hex(self) -> str:
        """Convert UUID to hex string without dashes."""
        hex_chars: str = "0123456789abcdef"
        result: str = ""
        
        for i in range(16):
            b: int = self.bytes[i]
            result = result + hex_chars[b // 16] + hex_chars[b % 16]
        
        return result
    
    def __eq__(self, other: UUID) -> bool:
        if len(self.bytes) != len(other.bytes):
            return False
        for i in range(16):
            if self.bytes[i] != other.bytes[i]:
                return False
        return True


def uuid4() -> UUID:
    """Generate a random UUID (version 4).
    
    Returns:
        Random UUID
    """
    bytes: list[int] = []
    
    for i in range(16):
        bytes.append(random.randint(0, 255))
    
    # Set version to 4 (random) in byte 6
    bytes[6] = (bytes[6] & 0x0f) | 0x40
    
    # Set variant to RFC 4122 in byte 8
    bytes[8] = (bytes[8] & 0x3f) | 0x80
    
    return UUID(bytes, 4)


def uuid7() -> UUID:
    """Generate a time-sorted UUID (version 7).
    
    Returns:
        Time-sorted UUID with millisecond precision
    """
    bytes: list[int] = []
    
    # Get current timestamp in milliseconds
    ts: int = int(time.time() * 1000)
    
    # First 48 bits are timestamp
    bytes.append((ts >> 40) & 0xff)
    bytes.append((ts >> 32) & 0xff)
    bytes.append((ts >> 24) & 0xff)
    bytes.append((ts >> 16) & 0xff)
    bytes.append((ts >> 8) & 0xff)
    bytes.append(ts & 0xff)
    
    # Next 4 bits are version (7), remaining are random
    bytes.append(0x70 | (random.randint(0, 15)))
    bytes.append(random.randint(0, 255))
    
    # Variant bits + random
    bytes.append(0x80 | (random.randint(0, 63)))
    
    # Remaining 7 bytes are random
    for i in range(7):
        bytes.append(random.randint(0, 255))
    
    return UUID(bytes, 7)


def from_string(s: str) -> UUID:
    """Parse UUID from string.
    
    Args:
        s: UUID string (with or without dashes)
        
    Returns:
        Parsed UUID
    """
    # Remove dashes
    hex_str: str = s.replace("-", "")
    
    if len(hex_str) != 32:
        return UUID()  # Invalid, return nil UUID
    
    bytes: list[int] = []
    hex_chars: str = "0123456789abcdef"
    
    i: int = 0
    while i < 32:
        high: str = hex_str[i].lower()
        low: str = hex_str[i + 1].lower()
        
        high_val: int = hex_chars.find(high)
        low_val: int = hex_chars.find(low)
        
        if high_val < 0 or low_val < 0:
            return UUID()  # Invalid hex
        
        bytes.append(high_val * 16 + low_val)
        i = i + 2
    
    # Detect version from byte 6
    version: int = (bytes[6] >> 4) & 0x0f
    
    return UUID(bytes, version)


def nil() -> UUID:
    """Return nil UUID (all zeros).
    
    Returns:
        Nil UUID
    """
    return UUID([0] * 16, 0)


def is_valid(s: str) -> bool:
    """Check if string is a valid UUID.
    
    Args:
        s: String to check
        
    Returns:
        True if valid UUID format
    """
    hex_str: str = s.replace("-", "")
    
    if len(hex_str) != 32:
        return False
    
    hex_chars: str = "0123456789abcdefABCDEF"
    for c in hex_str:
        if c not in hex_chars:
            return False
    
    return True
"##;
