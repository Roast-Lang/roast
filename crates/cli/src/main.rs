//! Roast compiler CLI (roastc).
//!
//! Roast is a compiled language with Python-like syntax and Rust-like performance.
//! By default, code is compiled to native binaries via LLVM.

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

mod commands;
mod repl;
mod bench;

#[derive(Parser)]
#[command(name = "roastc")]
#[command(author = "Roast Language Team")]
#[command(version)]
#[command(about = "The Roast programming language compiler - Python syntax, Rust speed!", long_about = None)]
#[command(after_help = "EXAMPLES:
    roastc run hello.roast        Run a Roast file (compiles to native)
    roastc run fib.roast -O3      Run with maximum optimization
    roastc build src/main.roast   Compile to native binary
    roastc repl                   Start interactive REPL
    roastc check src/             Type-check a directory
    roastc init my_project        Create a new project")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Enable debug mode (dump AST, MIR, etc.)
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a Roast project or file
    Build {
        /// Input file or directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "2")]
        opt_level: u32,

        /// Compile to native executable via Cranelift (FAST - like Rust!)
        #[arg(long)]
        native: bool,

        /// Compile to native executable via LLVM (FASTEST - true Rust speed!)
        #[arg(long)]
        llvm: bool,

        /// Emit bytecode instead of executable
        #[arg(long)]
        emit_bytecode: bool,

        /// Emit MIR for debugging
        #[arg(long)]
        emit_mir: bool,

        /// Emit AST for debugging
        #[arg(long)]
        emit_ast: bool,
    },

    /// Build and run a Roast file (compiles to native via LLVM by default)
    Run {
        /// Input file
        file: PathBuf,

        /// Arguments to pass to the program
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Optimization level (0-3, default: 2 for good balance)
        #[arg(short = 'O', long, default_value = "2")]
        opt_level: u32,

        /// Use Cranelift JIT instead of LLVM (faster compile, slower run)
        #[arg(long)]
        cranelift: bool,

        /// Use bytecode VM interpreter (for debugging, very slow)
        #[arg(long, hide = true)]
        vm: bool,
    },

    /// Start interactive REPL
    #[command(alias = "shell")]
    Repl {
        /// Don't print the banner
        #[arg(long)]
        no_banner: bool,
    },

    /// Evaluate a Roast expression
    Eval {
        /// Expression to evaluate
        expr: String,
    },

    /// Type-check without building
    Check {
        /// Input file or directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Show all warnings
        #[arg(short = 'W', long)]
        warnings: bool,
    },

    /// Format Roast source files
    Fmt {
        /// Input file or directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,

        /// Write changes in-place
        #[arg(short, long)]
        write: bool,
    },

    /// Run tests
    Test {
        /// Test filter pattern
        filter: Option<String>,

        /// Run tests in verbose mode
        #[arg(short, long)]
        verbose: bool,

        /// Run tests in parallel
        #[arg(short, long)]
        parallel: bool,

        /// Show output from passing tests
        #[arg(long)]
        show_output: bool,
    },

    /// Generate documentation
    Doc {
        /// Input file or directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "docs")]
        output: PathBuf,

        /// Open in browser after generating
        #[arg(long)]
        open: bool,

        /// Generate private items too
        #[arg(long)]
        private: bool,
    },

    /// Start the language server
    Lsp,

    /// Lint source files for errors and style issues
    Lint {
        /// Input file or directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Fix auto-fixable issues
        #[arg(long)]
        fix: bool,

        /// Show only errors (no warnings)
        #[arg(long)]
        errors_only: bool,

        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,

        /// Ignore specific rules (comma-separated)
        #[arg(long)]
        ignore: Option<String>,

        /// Enable specific rules (comma-separated)
        #[arg(long)]
        select: Option<String>,
    },

    /// Initialize a new Roast project
    Init {
        /// Project name (defaults to current directory name)
        name: Option<String>,

        /// Create a library instead of binary
        #[arg(long)]
        lib: bool,

        /// Initialize with git repository
        #[arg(long)]
        git: bool,
    },

    /// Create a new Roast file
    New {
        /// File name
        name: String,

        /// Create in src directory
        #[arg(long)]
        src: bool,
    },

    /// Clean build artifacts
    Clean {
        /// Also remove cached dependencies
        #[arg(long)]
        all: bool,
    },

    /// Migrate Python code to Roast
    Migrate {
        /// Input file or directory
        source: PathBuf,

        /// Output file or directory
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Add type annotations
        #[arg(long, default_value = "true")]
        types: bool,

        /// Add ownership annotations
        #[arg(long)]
        ownership: bool,

        /// Convert Python 2 idioms
        #[arg(long, default_value = "true")]
        py2: bool,

        /// Dry run (show changes without writing)
        #[arg(long)]
        dry_run: bool,
    },

    /// Show compiler version and configuration
    Version {
        /// Show detailed version info
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "error:".red().bold(), e);

        // Print causes
        for cause in e.chain().skip(1) {
            eprintln!("  {} {}", "caused by:".yellow(), cause);
        }

        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging
    let log_level = if cli.quiet {
        tracing::Level::ERROR
    } else if cli.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .without_time()
        .init();

    // Handle commands
    match cli.command {
        Some(Commands::Build {
            path,
            output,
            opt_level,
            native,
            llvm: _,  // LLVM is now the default
            emit_bytecode,
            emit_mir,
            emit_ast,
        }) => {
            // Default to LLVM unless bytecode/mir/ast is requested or native (Cranelift) is specified
            if emit_bytecode || emit_mir || emit_ast {
                commands::build(&path, output.as_deref(), opt_level, emit_bytecode, emit_mir, emit_ast, cli.debug)
            } else if native {
                commands::build_native(&path, output.as_deref(), opt_level, cli.debug)
            } else {
                // Default: LLVM native compilation (Rust-like speed!)
                commands::build_llvm(&path, output.as_deref(), opt_level, cli.debug)
            }
        }
        Some(Commands::Run { file, args, opt_level, cranelift, vm }) => {
            if vm {
                // Legacy VM mode (hidden, for debugging)
                commands::run(&file, &args, opt_level, cli.debug)
            } else if cranelift {
                // Cranelift JIT (faster compile, slower run)
                commands::run_native(&file, &args, opt_level, cli.debug)
            } else {
                // Default: LLVM native compilation (Rust-like speed!)
                commands::run_llvm(&file, &args, opt_level, cli.debug)
            }
        }
        Some(Commands::Repl { no_banner }) => {
            repl::start(!no_banner)
        }
        Some(Commands::Eval { expr }) => {
            commands::eval(&expr)
        }
        Some(Commands::Check { path, warnings }) => {
            commands::check(&path, warnings)
        }
        Some(Commands::Fmt { path, check, write }) => {
            commands::fmt(&path, check, write)
        }
        Some(Commands::Test { filter, verbose, parallel, show_output }) => {
            commands::test(filter.as_deref(), verbose, parallel, show_output)
        }
        Some(Commands::Doc { path, output, open, private }) => {
            commands::doc(&path, &output, open, private)
        }
        Some(Commands::Lsp) => {
            commands::lsp()
        }
        Some(Commands::Lint { path, fix, errors_only, format, ignore, select }) => {
            commands::lint(&path, fix, errors_only, &format, ignore.as_deref(), select.as_deref())
        }
        Some(Commands::Init { name, lib, git }) => {
            commands::init(name.as_deref(), lib, git)
        }
        Some(Commands::New { name, src }) => {
            commands::new_file(&name, src)
        }
        Some(Commands::Clean { all }) => {
            commands::clean(all)
        }
        Some(Commands::Migrate { source, output, types, ownership, py2, dry_run }) => {
            commands::migrate(&source, output.as_deref(), types, ownership, py2, dry_run)
        }
        Some(Commands::Version { verbose }) => {
            commands::version(verbose)
        }
        None => {
            // No subcommand - show help
            use clap::CommandFactory;
            Cli::command().print_help()?;
            Ok(())
        }
    }
}
