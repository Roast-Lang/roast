//! Interactive REPL for Roast with syntax highlighting and tab completion.

use anyhow::Result;
use colored::Colorize;
use roast_common::{DiagnosticSink, Interner};
use roast_parser::parse_module;
use roast_typer::{TypeContext, TypeChecker};
use roast_vm::{VM, VMConfig};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Cmd, Config, Editor, EventHandler, KeyCode, KeyEvent, Modifiers};
use std::io::{self, Write};

const BANNER: &str = r#"
  ____                 _   
 |  _ \ ___   __ _ ___| |_ 
 | |_) / _ \ / _` / __| __|
 |  _ < (_) | (_| \__ \ |_ 
 |_| \_\___/ \__,_|___/\__|
                           
"#;

// =============================================================================
// Keywords and Built-ins for completion
// =============================================================================

const KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from",
    "global", "if", "import", "in", "is", "lambda", "match", "case",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", "True", "False", "None",
];

const TYPES: &[&str] = &[
    "int", "float", "str", "bool", "bytes", "list", "dict", "set", "tuple",
    "List", "Dict", "Set", "Tuple", "Optional", "Union", "Any", "Callable",
    "Iterator", "Iterable", "Result", "Option",
];

const BUILTINS: &[&str] = &[
    "print", "len", "range", "enumerate", "zip", "map", "filter", "sorted",
    "reversed", "sum", "min", "max", "abs", "round", "pow", "divmod",
    "isinstance", "issubclass", "hasattr", "getattr", "setattr", "delattr",
    "type", "id", "hash", "repr", "str", "int", "float", "bool", "list",
    "dict", "set", "tuple", "bytes", "bytearray", "open", "input",
    "iter", "next", "all", "any", "ord", "chr", "hex", "oct", "bin",
    "format", "vars", "dir", "help", "callable", "compile", "eval", "exec",
];

// =============================================================================
// Syntax Highlighting
// =============================================================================

fn highlight_code(code: &str) -> String {
    let mut result = String::new();
    let mut current_word = String::new();
    let mut in_string = false;
    let mut string_char = ' ';
    
    for c in code.chars() {
        if in_string {
            if c == string_char {
                in_string = false;
                current_word.push(c);
                result.push_str(&format!("{}", current_word.green()));
                current_word.clear();
            } else if c == '\\' {
                current_word.push(c);
            } else {
                current_word.push(c);
            }
            continue;
        }
        
        if c == '"' || c == '\'' {
            // Flush current word
            if !current_word.is_empty() {
                result.push_str(&highlight_word(&current_word));
                current_word.clear();
            }
            in_string = true;
            string_char = c;
            current_word.push(c);
            continue;
        }
        
        if c == '#' {
            // Flush current word
            if !current_word.is_empty() {
                result.push_str(&highlight_word(&current_word));
                current_word.clear();
            }
            // Comment - rest of input is gray
            result.push_str(&format!("{}", format!("#{}", code.split('#').last().unwrap_or("")).bright_black()));
            break;
        }
        
        if c.is_alphanumeric() || c == '_' {
            current_word.push(c);
        } else {
            // Flush current word
            if !current_word.is_empty() {
                result.push_str(&highlight_word(&current_word));
                current_word.clear();
            }
            
            // Colorize operators and brackets
            match c {
                '(' | ')' | '[' | ']' | '{' | '}' => {
                    result.push_str(&format!("{}", c.to_string().cyan()));
                }
                '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' => {
                    result.push_str(&format!("{}", c.to_string().yellow()));
                }
                ':' => {
                    result.push_str(&format!("{}", c.to_string().bright_white()));
                }
                _ => {
                    result.push(c);
                }
            }
        }
    }
    
    // Flush remaining word
    if !current_word.is_empty() {
        if in_string {
            result.push_str(&format!("{}", current_word.green()));
        } else {
            result.push_str(&highlight_word(&current_word));
        }
    }
    
    result
}

fn highlight_word(word: &str) -> String {
    // Keywords
    if KEYWORDS.contains(&word) {
        return format!("{}", word.magenta().bold());
    }
    
    // Types
    if TYPES.contains(&word) {
        return format!("{}", word.blue());
    }
    
    // Builtins
    if BUILTINS.contains(&word) {
        return format!("{}", word.cyan());
    }
    
    // Numbers
    if word.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '_' || c == 'e' || c == 'E') 
       && word.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return format!("{}", word.yellow());
    }
    
    word.to_string()
}

// =============================================================================
// Tab Completion
// =============================================================================

fn get_completions(input: &str) -> Vec<String> {
    let words: Vec<&str> = input.split_whitespace().collect();
    let prefix = words.last().unwrap_or(&"");
    
    if prefix.is_empty() {
        return vec![];
    }
    
    let mut completions = Vec::new();
    
    // Match keywords
    for kw in KEYWORDS {
        if kw.starts_with(prefix) {
            completions.push(kw.to_string());
        }
    }
    
    // Match types
    for ty in TYPES {
        if ty.starts_with(prefix) {
            completions.push(ty.to_string());
        }
    }
    
    // Match builtins
    for bi in BUILTINS {
        if bi.starts_with(prefix) {
            completions.push(bi.to_string());
        }
    }
    
    completions.sort();
    completions.dedup();
    completions
}

// =============================================================================
// REPL
// =============================================================================

struct Repl {
    interner: Interner,
    type_ctx: TypeContext,
    vm: VM,
    line_number: usize,
}

impl Repl {
    fn new() -> Self {
        let config = VMConfig {
            debug: false,
            ..Default::default()
        };
        
        let interner = Interner::new();
        let type_ctx = TypeContext::with_builtins(&interner);
        
        Self {
            interner,
            type_ctx,
            vm: VM::with_config(config),
            line_number: 1,
        }
    }

    fn run(&mut self) -> Result<()> {
        let config = Config::builder()
            .history_ignore_space(true)
            .build();
        
        let mut rl: Editor<(), DefaultHistory> = Editor::with_config(config)?;
        
        // Load history
        let history_path = dirs::data_dir()
            .map(|d| d.join("roast").join("history.txt"));
        if let Some(ref path) = history_path {
            let _ = rl.load_history(path);
        }
        
        // Bind Ctrl+L to clear screen
        rl.bind_sequence(
            KeyEvent(KeyCode::Char('l'), Modifiers::CTRL),
            EventHandler::Simple(Cmd::ClearScreen),
        );
        
        let mut multiline_buffer = String::new();
        let mut in_multiline = false;
        
        loop {
            let prompt = if in_multiline {
                format!("{}  ", "...".bright_black())
            } else {
                format!("{} ", format!("[{}]", self.line_number).bright_green())
            };
            
            match rl.readline(&prompt) {
                Ok(line) => {
                    let trimmed = line.trim();
                    
                    // Handle tab completion display
                    if trimmed.ends_with('\t') {
                        let completions = get_completions(&line.trim_end_matches('\t'));
                        if !completions.is_empty() {
                            println!("\n{}", completions.join("  ").bright_black());
                        }
                        continue;
                    }
                    
                    // Check for multiline continuation
                    if trimmed.ends_with(':') || trimmed.ends_with('\\') || in_multiline {
                        if trimmed.is_empty() && in_multiline {
                            in_multiline = false;
                            let code = std::mem::take(&mut multiline_buffer);
                            let _ = rl.add_history_entry(&code);
                            self.execute(&code);
                            self.line_number += 1;
                        } else {
                            multiline_buffer.push_str(trimmed.trim_end_matches('\\'));
                            multiline_buffer.push('\n');
                            in_multiline = true;
                        }
                        continue;
                    }
                    
                    if trimmed.is_empty() {
                        continue;
                    }
                    
                    // Add to history
                    let _ = rl.add_history_entry(trimmed);
                    
                    // Handle commands
                    if trimmed.starts_with(':') {
                        if self.handle_command(trimmed)? {
                            break;
                        }
                        continue;
                    }
                    
                    // Display highlighted input
                    // println!(">>> {}", highlight_code(trimmed));
                    
                    // Execute
                    self.execute(trimmed);
                    self.line_number += 1;
                }
                Err(ReadlineError::Interrupted) => {
                    if in_multiline {
                        println!("{}", "^C (multiline cancelled)".bright_black());
                        multiline_buffer.clear();
                        in_multiline = false;
                    } else {
                        println!("{}", "^C".bright_black());
                    }
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("{}", "Goodbye!".bright_yellow());
                    break;
                }
                Err(err) => {
                    eprintln!("{} {:?}", "Error:".red(), err);
                    break;
                }
            }
        }
        
        // Save history
        if let Some(ref path) = history_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = rl.save_history(path);
        }
        
        Ok(())
    }

    fn handle_command(&mut self, cmd: &str) -> Result<bool> {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0];
        let arg = parts.get(1).copied();

        match command {
            ":quit" | ":q" | ":exit" => {
                println!("{}", "Goodbye!".bright_yellow());
                return Ok(true);
            }
            ":help" | ":h" | ":?" => {
                self.print_help();
            }
            ":clear" | ":cls" => {
                print!("\x1B[2J\x1B[1;1H");
                io::stdout().flush()?;
            }
            ":type" | ":t" => {
                if let Some(expr) = arg {
                    self.show_type(expr);
                } else {
                    println!("{} :type <expression>", "Usage:".yellow());
                }
            }
            ":ast" => {
                if let Some(code) = arg {
                    self.show_ast(code);
                } else {
                    println!("{} :ast <code>", "Usage:".yellow());
                }
            }
            ":load" | ":l" => {
                if let Some(path) = arg {
                    self.load_file(path)?;
                } else {
                    println!("{} :load <file>", "Usage:".yellow());
                }
            }
            ":reset" => {
                self.type_ctx = TypeContext::with_builtins(&self.interner);
                self.vm = VM::new();
                self.line_number = 1;
                println!("{}", "State reset.".green());
            }
            ":version" | ":v" => {
                println!("Roast {}", env!("CARGO_PKG_VERSION"));
            }
            ":completions" => {
                if let Some(prefix) = arg {
                    let completions = get_completions(prefix);
                    if completions.is_empty() {
                        println!("{}", "No completions found".bright_black());
                    } else {
                        for c in completions {
                            println!("  {}", c.cyan());
                        }
                    }
                } else {
                    println!("{} :completions <prefix>", "Usage:".yellow());
                }
            }
            _ => {
                println!("{} Unknown command: {}", "Error:".red(), command);
                println!("Type {} for available commands", ":help".cyan());
            }
        }

        Ok(false)
    }

    fn print_help(&self) {
        println!("{}", "\n📚 REPL Commands".bright_yellow().bold());
        println!();
        println!("  {}          - Show this help", ":help, :h, :?".cyan());
        println!("  {}               - Exit REPL", ":quit, :q".cyan());
        println!("  {}          - Clear screen", ":clear, :cls".cyan());
        println!("  {}     - Show type of expression", ":type <expr>".cyan());
        println!("  {}       - Show AST of code", ":ast <code>".cyan());
        println!("  {}     - Load and execute file", ":load <file>".cyan());
        println!("  {}             - Reset interpreter state", ":reset".cyan());
        println!("  {}         - Show version", ":version, :v".cyan());
        println!("  {} - Show completions", ":completions <prefix>".cyan());
        println!();
        println!("{}", "⌨️  Keyboard Shortcuts".bright_yellow().bold());
        println!();
        println!("  {}              - Previous command", "↑ / Ctrl+P".cyan());
        println!("  {}              - Next command", "↓ / Ctrl+N".cyan());
        println!("  {}                  - Clear screen", "Ctrl+L".cyan());
        println!("  {}                  - Cancel input", "Ctrl+C".cyan());
        println!("  {}                  - Exit REPL", "Ctrl+D".cyan());
        println!();
        println!("{}", "💡 Tips".bright_yellow().bold());
        println!();
        println!("  • Use {} for line continuation", "\\".cyan());
        println!("  • Lines ending with {} continue on next line", ":".cyan());
        println!("  • History is saved between sessions");
        println!("  • Use {} to see completions for a prefix", ":completions".cyan());
        println!();
    }

    fn execute(&mut self, code: &str) {
        // Pretty print the input with highlighting
        println!(">>> {}", highlight_code(code));
        
        let wrapped = if !code.contains('\n') && !code.starts_with("def ") && !code.starts_with("class ") {
            format!("__result__ = {}", code)
        } else {
            code.to_string()
        };

        let mut diagnostics = DiagnosticSink::new();

        match parse_module(&wrapped, "<repl>", &self.interner, &mut diagnostics) {
            Ok(module) => {
                let mut checker = TypeChecker::new(&mut self.type_ctx, &self.interner, &mut diagnostics);
                checker.check_module(&module);

                if diagnostics.has_errors() {
                    for diag in diagnostics.diagnostics() {
                        eprintln!("{}", format!("{}", diag).red());
                    }
                    return;
                }

                println!("{}", "=> (execution not yet implemented)".bright_black());
            }
            Err(e) => {
                eprintln!("{} {}", "Parse error:".red(), e);
            }
        }
    }

    fn show_type(&mut self, expr: &str) {
        let code = format!("__type_check__: type = {}", expr);
        let mut diagnostics = DiagnosticSink::new();

        match parse_module(&code, "<repl>", &self.interner, &mut diagnostics) {
            Ok(module) => {
                let mut checker = TypeChecker::new(&mut self.type_ctx, &self.interner, &mut diagnostics);
                checker.check_module(&module);
                println!("{} (type inference in progress)", "<type>".cyan());
            }
            Err(e) => {
                eprintln!("{} {}", "Parse error:".red(), e);
            }
        }
    }

    fn show_ast(&mut self, code: &str) {
        let mut diagnostics = DiagnosticSink::new();

        match parse_module(code, "<repl>", &self.interner, &mut diagnostics) {
            Ok(module) => {
                println!("{:#?}", module);
            }
            Err(e) => {
                eprintln!("{} {}", "Parse error:".red(), e);
            }
        }
    }

    fn load_file(&mut self, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        println!("{} {}", "Loading".green(), path);
        
        for line in content.lines() {
            if !line.trim().is_empty() && !line.trim().starts_with('#') {
                self.execute(line);
            }
        }
        
        Ok(())
    }
}

/// Starts the interactive REPL.
pub fn start(show_banner: bool) -> Result<()> {
    if show_banner {
        println!("{}", BANNER.bright_yellow());
        println!(
            "Roast {} - Python syntax with Rust performance",
            env!("CARGO_PKG_VERSION")
        );
        println!("Type {} for help, {} to exit\n", ":help".cyan(), ":quit".cyan());
    }

    let mut repl = Repl::new();
    repl.run()
}
