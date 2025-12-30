//! YAML serialization/deserialization module for Roast
//!
//! Provides YAML parsing and generation functionality.

/// YAML parsing and generation
pub const ROAST_YAML_SOURCE: &str = r##"
"""
YAML serialization and deserialization module.

Provides functions for parsing YAML strings and generating YAML output.
Supports basic YAML features: scalars, lists, and dictionaries.
"""


def parse(yaml_string: str) -> dict[str, any]:
    """Parse a YAML string into a dictionary.
    
    Args:
        yaml_string: YAML content to parse
        
    Returns:
        Parsed dictionary
    """
    result: dict[str, any] = {}
    lines: list[str] = yaml_string.split("\n")
    current_indent: int = 0
    stack: list[dict[str, any]] = [result]
    keys: list[str] = [""]
    
    for line in lines:
        # Skip empty lines and comments
        stripped: str = line.strip()
        if stripped == "" or stripped.startswith("#"):
            continue
        
        # Calculate indentation
        indent: int = 0
        for c in line:
            if c == " ":
                indent = indent + 1
            else:
                break
        
        # Parse key: value
        if ":" in stripped:
            parts: list[str] = stripped.split(":", 1)
            key: str = parts[0].strip()
            value_str: str = parts[1].strip() if len(parts) > 1 else ""
            
            # Determine which dict to add to based on indent
            level: int = indent // 2
            while len(stack) > level + 1:
                stack.pop()
                keys.pop()
            
            current_dict: dict[str, any] = stack[-1]
            
            if value_str == "":
                # Nested dict
                new_dict: dict[str, any] = {}
                current_dict[key] = new_dict
                stack.append(new_dict)
                keys.append(key)
            elif value_str.startswith("[") and value_str.endswith("]"):
                # Inline list
                current_dict[key] = _parse_inline_list(value_str)
            elif value_str.startswith("{") and value_str.endswith("}"):
                # Inline dict
                current_dict[key] = _parse_inline_dict(value_str)
            else:
                # Scalar value
                current_dict[key] = _parse_scalar(value_str)
        
        elif stripped.startswith("- "):
            # List item
            item_value: str = stripped[2:].strip()
            level = indent // 2
            while len(stack) > level + 1:
                stack.pop()
                keys.pop()
            
            current_dict = stack[-1]
            parent_key: str = keys[-1]
            
            if parent_key in current_dict and isinstance(current_dict[parent_key], list):
                current_dict[parent_key].append(_parse_scalar(item_value))
            else:
                current_dict[parent_key] = [_parse_scalar(item_value)]
    
    return result


def _parse_scalar(s: str) -> any:
    """Parse a scalar value (int, float, bool, str)."""
    if s == "true" or s == "True":
        return True
    if s == "false" or s == "False":
        return False
    if s == "null" or s == "None" or s == "~":
        return None
    
    # Try integer
    try:
        return int(s)
    except:
        pass
    
    # Try float
    try:
        return float(s)
    except:
        pass
    
    # String - remove quotes if present
    if (s.startswith("\"") and s.endswith("\"")) or (s.startswith("'") and s.endswith("'")):
        return s[1:-1]
    
    return s


def _parse_inline_list(s: str) -> list[any]:
    """Parse an inline YAML list like [1, 2, 3]."""
    result: list[any] = []
    inner: str = s[1:-1].strip()
    if inner == "":
        return result
    
    items: list[str] = inner.split(",")
    for item in items:
        result.append(_parse_scalar(item.strip()))
    
    return result


def _parse_inline_dict(s: str) -> dict[str, any]:
    """Parse an inline YAML dict like {a: 1, b: 2}."""
    result: dict[str, any] = {}
    inner: str = s[1:-1].strip()
    if inner == "":
        return result
    
    pairs: list[str] = inner.split(",")
    for pair in pairs:
        if ":" in pair:
            parts: list[str] = pair.split(":", 1)
            key: str = parts[0].strip()
            value: str = parts[1].strip() if len(parts) > 1 else ""
            result[key] = _parse_scalar(value)
    
    return result


def dump(data: dict[str, any], indent: int = 2) -> str:
    """Convert a dictionary to YAML string.
    
    Args:
        data: Dictionary to convert
        indent: Number of spaces for indentation
        
    Returns:
        YAML string
    """
    return _dump_value(data, 0, indent)


def _dump_value(value: any, level: int, indent: int) -> str:
    """Recursively dump a value to YAML."""
    prefix: str = " " * (level * indent)
    
    if value is None:
        return "null"
    
    if isinstance(value, bool):
        return "true" if value else "false"
    
    if isinstance(value, int) or isinstance(value, float):
        return str(value)
    
    if isinstance(value, str):
        # Quote strings with special characters
        if ":" in value or "#" in value or "\n" in value:
            return "\"" + value.replace("\"", "\\\"") + "\""
        return value
    
    if isinstance(value, list):
        if len(value) == 0:
            return "[]"
        lines: list[str] = []
        for item in value:
            item_str: str = _dump_value(item, level + 1, indent)
            if isinstance(item, dict):
                lines.append(prefix + "- ")
                dict_lines: list[str] = item_str.split("\n")
                for i, dl in enumerate(dict_lines):
                    if i == 0:
                        lines[-1] = lines[-1] + dl
                    else:
                        lines.append(prefix + "  " + dl)
            else:
                lines.append(prefix + "- " + item_str)
        return "\n".join(lines)
    
    if isinstance(value, dict):
        if len(value) == 0:
            return "{}"
        lines = []
        for key in value:
            val: any = value[key]
            key_prefix: str = prefix if level > 0 else ""
            if isinstance(val, dict) or isinstance(val, list):
                lines.append(key_prefix + key + ":")
                lines.append(_dump_value(val, level + 1, indent))
            else:
                lines.append(key_prefix + key + ": " + _dump_value(val, 0, indent))
        return "\n".join(lines)
    
    return str(value)


def loads(yaml_string: str) -> dict[str, any]:
    """Alias for parse()."""
    return parse(yaml_string)


def dumps(data: dict[str, any]) -> str:
    """Alias for dump()."""
    return dump(data)
"##;
