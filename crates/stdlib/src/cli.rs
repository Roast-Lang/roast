//! Command-line argument parsing module for Roast
//!
//! Provides argument parsing, flag handling, and help generation.

/// CLI argument parsing
pub const ROAST_CLI_SOURCE: &str = r##"
"""
Command-line interface module.

Provides argument parsing, flag handling, and automatic help generation.
"""

class Argument:
    """Definition of a command-line argument."""
    name: str
    short: str
    description: str
    required: bool
    default: str
    is_flag: bool
    
    def __init__(self, name: str, short: str = "", description: str = "",
                 required: bool = False, default: str = "", is_flag: bool = False) -> None:
        self.name = name
        self.short = short
        self.description = description
        self.required = required
        self.default = default
        self.is_flag = is_flag


class ParseResult:
    """Result of parsing command-line arguments."""
    args: dict[str, str]
    flags: dict[str, bool]
    positional: list[str]
    errors: list[str]
    
    def __init__(self) -> None:
        self.args = {}
        self.flags = {}
        self.positional = []
        self.errors = []
    
    def get(self, name: str, default: str = "") -> str:
        """Get argument value by name."""
        if name in self.args:
            return self.args[name]
        return default
    
    def has_flag(self, name: str) -> bool:
        """Check if flag is set."""
        if name in self.flags:
            return self.flags[name]
        return False
    
    def is_valid(self) -> bool:
        """Check if parsing succeeded without errors."""
        return len(self.errors) == 0


class Parser:
    """Command-line argument parser."""
    name: str
    description: str
    version: str
    arguments: list[Argument]
    
    def __init__(self, name: str, description: str = "", version: str = "1.0.0") -> None:
        self.name = name
        self.description = description
        self.version = version
        self.arguments = []
    
    def add_argument(self, name: str, short: str = "", description: str = "",
                     required: bool = False, default: str = "") -> None:
        """Add a named argument (--name value)."""
        arg: Argument = Argument(name, short, description, required, default, False)
        self.arguments.append(arg)
    
    def add_flag(self, name: str, short: str = "", description: str = "") -> None:
        """Add a boolean flag (--flag)."""
        arg: Argument = Argument(name, short, description, False, "", True)
        self.arguments.append(arg)
    
    def parse(self, argv: list[str]) -> ParseResult:
        """Parse command-line arguments.
        
        Args:
            argv: List of arguments (typically sys.argv[1:])
            
        Returns:
            ParseResult with parsed values
        """
        result: ParseResult = ParseResult()
        
        # Set defaults
        for arg in self.arguments:
            if arg.is_flag:
                result.flags[arg.name] = False
            elif arg.default != "":
                result.args[arg.name] = arg.default
        
        i: int = 0
        while i < len(argv):
            current: str = argv[i]
            
            # Check for help
            if current == "-h" or current == "--help":
                result.args["help"] = "true"
                i = i + 1
                continue
            
            # Check for version
            if current == "-v" or current == "--version":
                result.args["version"] = "true"
                i = i + 1
                continue
            
            # Long argument (--name or --name=value)
            if current.startswith("--"):
                name: str = current[2:]
                value: str = ""
                
                if "=" in name:
                    parts: list[str] = name.split("=")
                    name = parts[0]
                    value = parts[1] if len(parts) > 1 else ""
                
                # Find matching argument
                found: bool = False
                for arg in self.arguments:
                    if arg.name == name:
                        found = True
                        if arg.is_flag:
                            result.flags[name] = True
                        else:
                            if value == "" and i + 1 < len(argv):
                                i = i + 1
                                value = argv[i]
                            result.args[name] = value
                        break
                
                if not found:
                    result.errors.append("Unknown argument: --" + name)
            
            # Short argument (-n or -n value)
            elif current.startswith("-") and len(current) == 2:
                short: str = current[1:]
                
                found = False
                for arg in self.arguments:
                    if arg.short == short:
                        found = True
                        if arg.is_flag:
                            result.flags[arg.name] = True
                        else:
                            if i + 1 < len(argv):
                                i = i + 1
                                result.args[arg.name] = argv[i]
                            else:
                                result.errors.append("Missing value for -" + short)
                        break
                
                if not found:
                    result.errors.append("Unknown argument: -" + short)
            
            # Positional argument
            else:
                result.positional.append(current)
            
            i = i + 1
        
        # Check required arguments
        for arg in self.arguments:
            if arg.required and not arg.is_flag:
                if arg.name not in result.args:
                    result.errors.append("Missing required argument: --" + arg.name)
        
        return result
    
    def help(self) -> str:
        """Generate help text."""
        lines: list[str] = []
        lines.append(self.name + " v" + self.version)
        if self.description != "":
            lines.append("")
            lines.append(self.description)
        
        lines.append("")
        lines.append("Usage:")
        lines.append("  " + self.name + " [OPTIONS]")
        
        if len(self.arguments) > 0:
            lines.append("")
            lines.append("Options:")
            for arg in self.arguments:
                opt: str = "  "
                if arg.short != "":
                    opt = opt + "-" + arg.short + ", "
                opt = opt + "--" + arg.name
                if not arg.is_flag:
                    opt = opt + " <VALUE>"
                if arg.required:
                    opt = opt + " (required)"
                lines.append(opt)
                if arg.description != "":
                    lines.append("      " + arg.description)
        
        lines.append("")
        lines.append("  -h, --help     Show this help message")
        lines.append("  -v, --version  Show version")
        
        return "\n".join(lines)


def parse_args(argv: list[str]) -> dict[str, str]:
    """Simple argument parsing without a parser.
    
    Args:
        argv: Command-line arguments
        
    Returns:
        Dictionary of argument names to values
    """
    result: dict[str, str] = {}
    i: int = 0
    
    while i < len(argv):
        arg: str = argv[i]
        if arg.startswith("--"):
            name: str = arg[2:]
            if "=" in name:
                parts: list[str] = name.split("=")
                result[parts[0]] = parts[1] if len(parts) > 1 else ""
            elif i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                result[name] = argv[i + 1]
                i = i + 1
            else:
                result[name] = "true"
        elif arg.startswith("-") and len(arg) == 2:
            if i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                result[arg[1:]] = argv[i + 1]
                i = i + 1
            else:
                result[arg[1:]] = "true"
        i = i + 1
    
    return result
"##;
