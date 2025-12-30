//! URL parsing and encoding module for Roast
//!
//! Provides URL parsing, construction, and encoding/decoding functionality.

/// URL parsing and manipulation
pub const ROAST_URL_SOURCE: &str = r##"
"""
URL parsing and encoding module.

Provides functions for parsing URLs into components and encoding/decoding URL strings.
"""

class URL:
    """Parsed URL components."""
    scheme: str
    host: str
    port: int
    path: str
    query: str
    fragment: str
    username: str
    password: str
    
    def __init__(self, scheme: str = "", host: str = "", port: int = 0, 
                 path: str = "", query: str = "", fragment: str = "",
                 username: str = "", password: str = "") -> None:
        self.scheme = scheme
        self.host = host
        self.port = port
        self.path = path
        self.query = query
        self.fragment = fragment
        self.username = username
        self.password = password
    
    def __repr__(self) -> str:
        return self.to_string()
    
    def to_string(self) -> str:
        """Convert URL back to string representation."""
        result: str = ""
        if self.scheme != "":
            result = self.scheme + "://"
        if self.username != "":
            result = result + self.username
            if self.password != "":
                result = result + ":" + self.password
            result = result + "@"
        result = result + self.host
        if self.port > 0:
            result = result + ":" + str(self.port)
        result = result + self.path
        if self.query != "":
            result = result + "?" + self.query
        if self.fragment != "":
            result = result + "#" + self.fragment
        return result


def parse(url_string: str) -> URL:
    """Parse a URL string into its components.
    
    Args:
        url_string: The URL to parse
        
    Returns:
        URL object with parsed components
    """
    result: URL = URL()
    remaining: str = url_string
    
    # Extract scheme
    if "://" in remaining:
        parts: list[str] = remaining.split("://")
        result.scheme = parts[0]
        remaining = parts[1] if len(parts) > 1 else ""
    
    # Extract fragment
    if "#" in remaining:
        parts = remaining.split("#")
        remaining = parts[0]
        result.fragment = parts[1] if len(parts) > 1 else ""
    
    # Extract query
    if "?" in remaining:
        parts = remaining.split("?")
        remaining = parts[0]
        result.query = parts[1] if len(parts) > 1 else ""
    
    # Extract path
    if "/" in remaining:
        idx: int = remaining.find("/")
        result.path = remaining[idx:]
        remaining = remaining[:idx]
    
    # Extract auth info
    if "@" in remaining:
        parts = remaining.split("@")
        auth: str = parts[0]
        remaining = parts[1] if len(parts) > 1 else ""
        if ":" in auth:
            auth_parts: list[str] = auth.split(":")
            result.username = auth_parts[0]
            result.password = auth_parts[1] if len(auth_parts) > 1 else ""
        else:
            result.username = auth
    
    # Extract port
    if ":" in remaining:
        parts = remaining.split(":")
        result.host = parts[0]
        port_str: str = parts[1] if len(parts) > 1 else "0"
        result.port = int(port_str) if port_str != "" else 0
    else:
        result.host = remaining
    
    return result


def encode(s: str) -> str:
    """URL-encode a string (percent encoding).
    
    Args:
        s: String to encode
        
    Returns:
        URL-encoded string
    """
    result: str = ""
    safe_chars: str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"
    hex_digits: str = "0123456789ABCDEF"
    
    for c in s:
        if c in safe_chars:
            result = result + c
        elif c == " ":
            result = result + "+"
        else:
            # Encode as %XX
            code: int = ord(c)
            result = result + "%" + hex_digits[code // 16] + hex_digits[code % 16]
    
    return result


def decode(s: str) -> str:
    """Decode a URL-encoded string.
    
    Args:
        s: URL-encoded string
        
    Returns:
        Decoded string
    """
    result: str = ""
    i: int = 0
    hex_digits: str = "0123456789ABCDEFabcdef"
    
    while i < len(s):
        c: str = s[i]
        if c == "+":
            result = result + " "
            i = i + 1
        elif c == "%" and i + 2 < len(s):
            h1: str = s[i + 1]
            h2: str = s[i + 2]
            if h1 in hex_digits and h2 in hex_digits:
                val: int = int(h1 + h2, 16)
                result = result + chr(val)
                i = i + 3
            else:
                result = result + c
                i = i + 1
        else:
            result = result + c
            i = i + 1
    
    return result


def parse_query(query: str) -> dict[str, str]:
    """Parse query string into dictionary.
    
    Args:
        query: Query string (without leading ?)
        
    Returns:
        Dictionary of key-value pairs
    """
    result: dict[str, str] = {}
    
    if query == "":
        return result
    
    pairs: list[str] = query.split("&")
    for pair in pairs:
        if "=" in pair:
            parts: list[str] = pair.split("=")
            key: str = decode(parts[0])
            value: str = decode(parts[1]) if len(parts) > 1 else ""
            result[key] = value
        else:
            result[decode(pair)] = ""
    
    return result


def build_query(params: dict[str, str]) -> str:
    """Build query string from dictionary.
    
    Args:
        params: Dictionary of parameters
        
    Returns:
        Query string (without leading ?)
    """
    parts: list[str] = []
    for key in params:
        value: str = params[key]
        parts.append(encode(key) + "=" + encode(value))
    return "&".join(parts)


def join(base: str, path: str) -> str:
    """Join base URL with relative path.
    
    Args:
        base: Base URL
        path: Relative path
        
    Returns:
        Combined URL
    """
    if path.startswith("http://") or path.startswith("https://"):
        return path
    
    if path.startswith("/"):
        # Absolute path - keep scheme and host
        parsed: URL = parse(base)
        return parsed.scheme + "://" + parsed.host + path
    
    # Relative path
    if base.endswith("/"):
        return base + path
    else:
        # Find last slash
        idx: int = base.rfind("/")
        if idx > base.find("://") + 2:
            return base[:idx + 1] + path
        else:
            return base + "/" + path
"##;
