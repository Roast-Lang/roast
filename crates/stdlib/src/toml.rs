//! TOML serialization/deserialization module for Roast
//!
//! Provides TOML parsing and generation functionality.

/// TOML parsing and generation
pub const ROAST_TOML_SOURCE: &str = r##"
"""
TOML serialization and deserialization module.

Provides functions for parsing TOML strings and generating TOML output.
Supports TOML 1.0 basic features.
"""


def parse(toml_string: str) -> dict[str, any]:
    """Parse a TOML string into a dictionary.
    
    Args:
        toml_string: TOML content to parse
        
    Returns:
        Parsed dictionary
    """
    result: dict[str, any] = {}
    current_section: dict[str, any] = result
    lines: list[str] = toml_string.split("\n")
    
    for line in lines:
        stripped: str = line.strip()
        
        # Skip empty lines and comments
        if stripped == "" or stripped.startswith("#"):
            continue
        
        # Section header [section] or [[array]]
        if stripped.startswith("["):
            if stripped.startswith("[[") and stripped.endswith("]]"):
                # Array of tables
                section_name: str = stripped[2:-2].strip()
                current_section = _get_or_create_array_section(result, section_name)
            elif stripped.endswith("]"):
                # Regular section
                section_name = stripped[1:-1].strip()
                current_section = _get_or_create_section(result, section_name)
            continue
        
        # Key = value
        if "=" in stripped:
            parts: list[str] = stripped.split("=", 1)
            key: str = parts[0].strip()
            value_str: str = parts[1].strip() if len(parts) > 1 else ""
            
            # Handle dotted keys
            if "." in key:
                key_parts: list[str] = key.split(".")
                target: dict[str, any] = current_section
                for i in range(len(key_parts) - 1):
                    k: str = key_parts[i].strip()
                    if k not in target:
                        target[k] = {}
                    target = target[k]
                key = key_parts[-1].strip()
                target[key] = _parse_value(value_str)
            else:
                current_section[key] = _parse_value(value_str)
    
    return result


def _get_or_create_section(root: dict[str, any], path: str) -> dict[str, any]:
    """Get or create a nested section by dotted path."""
    current: dict[str, any] = root
    parts: list[str] = path.split(".")
    
    for part in parts:
        part = part.strip()
        if part not in current:
            current[part] = {}
        current = current[part]
    
    return current


def _get_or_create_array_section(root: dict[str, any], path: str) -> dict[str, any]:
    """Get or create an array section (for [[array]] syntax)."""
    parts: list[str] = path.split(".")
    current: dict[str, any] = root
    
    for i in range(len(parts) - 1):
        part: str = parts[i].strip()
        if part not in current:
            current[part] = {}
        current = current[part]
    
    final_key: str = parts[-1].strip()
    if final_key not in current:
        current[final_key] = []
    
    new_item: dict[str, any] = {}
    current[final_key].append(new_item)
    return new_item


def _parse_value(s: str) -> any:
    """Parse a TOML value."""
    s = s.strip()
    
    # Boolean
    if s == "true":
        return True
    if s == "false":
        return False
    
    # String (basic or literal)
    if s.startswith('"""') and s.endswith('"""'):
        return s[3:-3]
    if s.startswith("'''") and s.endswith("'''"):
        return s[3:-3]
    if s.startswith('"') and s.endswith('"'):
        return _parse_basic_string(s[1:-1])
    if s.startswith("'") and s.endswith("'"):
        return s[1:-1]  # Literal string, no escapes
    
    # Array
    if s.startswith("[") and s.endswith("]"):
        return _parse_array(s)
    
    # Inline table
    if s.startswith("{") and s.endswith("}"):
        return _parse_inline_table(s)
    
    # Integer (including hex, octal, binary)
    if s.startswith("0x"):
        return int(s, 16)
    if s.startswith("0o"):
        return int(s, 8)
    if s.startswith("0b"):
        return int(s, 2)
    
    # Try integer
    try:
        # Handle underscores in numbers
        clean: str = s.replace("_", "")
        return int(clean)
    except:
        pass
    
    # Try float
    try:
        clean = s.replace("_", "")
        if clean == "inf" or clean == "+inf":
            return float("inf")
        if clean == "-inf":
            return float("-inf")
        if clean == "nan" or clean == "+nan" or clean == "-nan":
            return float("nan")
        return float(clean)
    except:
        pass
    
    # Return as string
    return s


def _parse_basic_string(s: str) -> str:
    """Parse a basic TOML string with escape sequences."""
    result: str = ""
    i: int = 0
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s):
            next_char: str = s[i + 1]
            if next_char == "n":
                result = result + "\n"
            elif next_char == "t":
                result = result + "\t"
            elif next_char == "r":
                result = result + "\r"
            elif next_char == "\\":
                result = result + "\\"
            elif next_char == "\"":
                result = result + "\""
            else:
                result = result + s[i]
                i = i + 1
                continue
            i = i + 2
        else:
            result = result + s[i]
            i = i + 1
    return result


def _parse_array(s: str) -> list[any]:
    """Parse a TOML array."""
    result: list[any] = []
    inner: str = s[1:-1].strip()
    
    if inner == "":
        return result
    
    # Simple split by comma (doesn't handle nested structures well)
    items: list[str] = []
    current: str = ""
    depth: int = 0
    in_string: bool = False
    
    for c in inner:
        if c == '"' and (len(current) == 0 or current[-1] != "\\"):
            in_string = not in_string
        if not in_string:
            if c == "[" or c == "{":
                depth = depth + 1
            elif c == "]" or c == "}":
                depth = depth - 1
            elif c == "," and depth == 0:
                items.append(current.strip())
                current = ""
                continue
        current = current + c
    
    if current.strip() != "":
        items.append(current.strip())
    
    for item in items:
        result.append(_parse_value(item))
    
    return result


def _parse_inline_table(s: str) -> dict[str, any]:
    """Parse a TOML inline table."""
    result: dict[str, any] = {}
    inner: str = s[1:-1].strip()
    
    if inner == "":
        return result
    
    # Split by comma
    pairs: list[str] = inner.split(",")
    for pair in pairs:
        if "=" in pair:
            parts: list[str] = pair.split("=", 1)
            key: str = parts[0].strip()
            value: str = parts[1].strip() if len(parts) > 1 else ""
            result[key] = _parse_value(value)
    
    return result


def dump(data: dict[str, any]) -> str:
    """Convert a dictionary to TOML string.
    
    Args:
        data: Dictionary to convert
        
    Returns:
        TOML string
    """
    lines: list[str] = []
    
    # First, output simple key-value pairs
    for key in data:
        value: any = data[key]
        if not isinstance(value, dict):
            lines.append(key + " = " + _value_to_toml(value))
    
    # Then, output sections
    for key in data:
        value = data[key]
        if isinstance(value, dict):
            lines.append("")
            lines.append("[" + key + "]")
            for subkey in value:
                subvalue: any = value[subkey]
                if isinstance(subvalue, dict):
                    lines.append("")
                    lines.append("[" + key + "." + subkey + "]")
                    for k in subvalue:
                        lines.append(k + " = " + _value_to_toml(subvalue[k]))
                else:
                    lines.append(subkey + " = " + _value_to_toml(subvalue))
    
    return "\n".join(lines)


def _value_to_toml(value: any) -> str:
    """Convert a value to TOML representation."""
    if value is None:
        return '""'  # TOML doesn't have null, use empty string
    
    if isinstance(value, bool):
        return "true" if value else "false"
    
    if isinstance(value, int):
        return str(value)
    
    if isinstance(value, float):
        return str(value)
    
    if isinstance(value, str):
        # Escape and quote
        escaped: str = value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\t", "\\t")
        return "\"" + escaped + "\""
    
    if isinstance(value, list):
        items: list[str] = []
        for item in value:
            items.append(_value_to_toml(item))
        return "[" + ", ".join(items) + "]"
    
    if isinstance(value, dict):
        pairs: list[str] = []
        for k in value:
            pairs.append(k + " = " + _value_to_toml(value[k]))
        return "{ " + ", ".join(pairs) + " }"
    
    return str(value)


def loads(toml_string: str) -> dict[str, any]:
    """Alias for parse()."""
    return parse(toml_string)


def dumps(data: dict[str, any]) -> str:
    """Alias for dump()."""
    return dump(data)
"##;
