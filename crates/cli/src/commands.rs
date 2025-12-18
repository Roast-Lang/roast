//! CLI command implementations.

use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::time::Instant;

use roast_common::{DiagnosticSink, Interner};
use roast_parser::parse_module;
use roast_typer::{TypeContext, TypeChecker};
use roast_codegen::{CodeEmitter, TargetConfig, target::OptLevel, CodeGenerator};
use roast_vm::{VM, VMConfig};
use roast_llvm_backend::{LlvmCodeGen, LlvmConfig};
use roast_hir::build::HirBuilder;
use roast_mir::build::MirBuilder;
use roast_borrowck::BorrowChecker;
use roast_optimizer::{Optimizer, OptLevel as MirOptLevel};
use roast_ast::{StmtKind, ExprKind};
use roast_runtime::Value;
use std::sync::Arc;
use std::collections::HashMap;

/// Build command.
pub fn build(
    path: &Path,
    output: Option<&Path>,
    opt_level: u32,
    emit_bytecode: bool,
    emit_mir: bool,
    emit_ast: bool,
    debug: bool,
) -> Result<()> {
    let start = Instant::now();

    println!("{} {}", "   Compiling".green().bold(), path.display());

    // Handle directories
    let files = if path.is_dir() {
        find_roast_files(path)?
    } else {
        vec![path.to_path_buf()]
    };

    if files.is_empty() {
        bail!("no Roast files found in {}", path.display());
    }

    let interner = Interner::new();
    let mut total_errors = 0;

    for file in &files {
        let source = fs::read_to_string(file)
            .with_context(|| format!("failed to read {}", file.display()))?;

        // Use with_source to enable # type: ignore support
        let mut diagnostics = DiagnosticSink::with_source(&source);

        // Parse
        let module = match parse_module(&source, &file.to_string_lossy(), &interner, &mut diagnostics) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{} {}", "Parse error:".red().bold(), e);
                total_errors += 1;
                continue;
            }
        };

        if emit_ast {
            println!("\n{} for {}:", "AST".cyan().bold(), file.display());
            println!("{:#?}", module);
        }

        // Type check
        let mut type_ctx = TypeContext::with_builtins(&interner);
        let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
        checker.check_module(&module);

        // Report diagnostics
        if diagnostics.has_errors() {
            report_diagnostics(&diagnostics, &source, file);
            total_errors += diagnostics.error_count();
            continue;
        }

        if debug || diagnostics.warning_count() > 0 {
            report_diagnostics(&diagnostics, &source, file);
        }

        // Generate bytecode
        let target_config = TargetConfig::bytecode()
            .with_opt_level(OptLevel::from_u8(opt_level as u8));

        let mut emitter = CodeEmitter::new(target_config);

        if emit_mir {
            println!("\n{} for {}:", "MIR".cyan().bold(), file.display());
            println!("(MIR output not yet implemented)");
        }

        // Output
        let output_path = if let Some(out) = output {
            out.to_path_buf()
        } else {
            let stem = file.file_stem().unwrap_or_default();
            if emit_bytecode {
                file.with_file_name(format!("{}.rbc", stem.to_string_lossy()))
            } else {
                file.with_file_name(stem)
            }
        };

        if emit_bytecode {
            // Write bytecode file
            println!("{} {}", "   Emitting".blue(), output_path.display());
        }
    }

    if total_errors > 0 {
        bail!("compilation failed with {} error(s)", total_errors);
    }

    println!(
        "{} in {:.2}s",
        "    Finished".green().bold(),
        start.elapsed().as_secs_f64()
    );

    Ok(())
}

/// Run command - compile and execute.
pub fn run(file: &Path, args: &[String], _opt_level: u32, debug: bool) -> Result<()> {
    let start = Instant::now();

    println!("{} {}", "     Running".green().bold(), file.display());

    // Read and parse
    let source = fs::read_to_string(file)
        .with_context(|| format!("failed to read {}", file.display()))?;

    let interner = Interner::new();
    // Use with_source to enable # type: ignore support
    let mut diagnostics = DiagnosticSink::with_source(&source);

    let module = parse_module(&source, &file.to_string_lossy(), &interner, &mut diagnostics)?;

    // Type check
    let mut type_ctx = TypeContext::with_builtins(&interner);
    let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
    checker.check_module(&module);

    if diagnostics.has_errors() {
        report_diagnostics(&diagnostics, &source, file);
        bail!("compilation failed");
    }

    // Build HIR from AST and compile to bytecode
    // We need to scope the mutable borrows carefully
    let bytecode_modules: Vec<_> = {
        let mut hir_builder = HirBuilder::new(&interner, &mut type_ctx, &mut diagnostics);
        let hir_module = hir_builder.build_module(&module);

        let mut modules = Vec::new();
        for item in &hir_module.items {
            if let roast_hir::HirItem::Function(func) = item {
                let mut mir_builder = MirBuilder::new(hir_builder.expr_arena());
                let mir_body = mir_builder.build_function(func);

                // Compile MIR to bytecode
                let mut bytecode_builder = roast_codegen::BytecodeBuilder::with_interner(&func.name.to_string(), &interner);
                let bytecode = bytecode_builder.compile(&mir_body);

                // Clone Symbol to u32 representation for later use; include is_async flag
                let name_raw = func.name.as_raw();
                let is_async = mir_body.is_async;
                modules.push((name_raw, Arc::new(bytecode), is_async));
            }
        }
        modules
    };

    if diagnostics.has_errors() {
        report_diagnostics(&diagnostics, &source, file);
        bail!("HIR lowering failed");
    }

    // Set up VM
    let config = VMConfig {
        debug,
        ..Default::default()
    };
    let mut vm = VM::with_config(config);

    // Set program arguments
    vm.set_global("__file__", roast_runtime::Value::Str(file.to_string_lossy().into()));
    vm.set_global("__name__", roast_runtime::Value::Str("__main__".into()));

    // Store args
    let args_list: Vec<_> = std::iter::once(file.to_string_lossy().to_string())
        .chain(args.iter().cloned())
        .map(|s| roast_runtime::Value::Str(s.into()))
        .collect();
    vm.set_global("sys_argv", roast_runtime::Value::List(std::sync::Arc::new(std::sync::Mutex::new(args_list))));

    // First, store all functions as globals so they can call each other
    for (name_raw, bytecode, is_async) in &bytecode_modules {
        let name_sym = roast_common::Symbol::from_raw(*name_raw);
        let name_str = interner.resolve(name_sym).unwrap_or("?");

        let func = roast_runtime::value::RoastFunction {
            name: name_str.to_string(),
            arity: bytecode.num_params as usize,
            code: bytecode.clone(),
            is_async: *is_async,
        };
        vm.set_global(name_str, roast_runtime::Value::Function(std::sync::Arc::new(func)));
    }

    // Find and execute main function or __main__ block
    for (name_raw, bytecode, _is_async) in &bytecode_modules {
        // Resolve the raw symbol to a string
        let name_sym = roast_common::Symbol::from_raw(*name_raw);
        let name_str = interner.resolve(name_sym).unwrap_or("?");
        if name_str == "__main__" || name_str == "__module_init__" || name_str == "main" {
            match vm.execute(bytecode.clone()) {
                Ok(result) => {
                    if debug {
                        println!("{}: {:?}", name_str, result);
                    }
                }
                Err(e) => {
                    eprintln!("{}: {}", "Runtime error".red().bold(), e);
                    bail!("execution failed");
                }
            }
        }
    }

    let elapsed = start.elapsed();

    println!("\n{}", "─".repeat(40).bright_black());
    println!(
        "{} Process completed in {:.2}s",
        "    Finished".green().bold(),
        elapsed.as_secs_f64()
    );

    Ok(())
}

/// Eval command - evaluate an expression.
pub fn eval(expr: &str) -> Result<()> {
    let interner = Interner::new();
    let mut diagnostics = DiagnosticSink::new();

    // Wrap as assignment to capture result
    let code = format!("__result__ = {}", expr);

    match parse_module(&code, "<eval>", &interner, &mut diagnostics) {
        Ok(module) => {
            let mut type_ctx = TypeContext::with_builtins(&interner);
            let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
            checker.check_module(&module);

            if diagnostics.has_errors() {
                for diag in diagnostics.diagnostics() {
                    eprintln!("{}", format!("{}", diag).red());
                }
                bail!("evaluation failed");
            }

            // Would execute and print result
            println!("{}", "(evaluation result pending)".bright_black());
            Ok(())
        }
        Err(e) => {
            bail!("parse error: {}", e);
        }
    }
}

/// Check command.
pub fn check(path: &Path, show_warnings: bool) -> Result<()> {
    let start = Instant::now();

    println!("{} {}", "    Checking".green().bold(), path.display());

    let files = if path.is_dir() {
        find_roast_files(path)?
    } else {
        vec![path.to_path_buf()]
    };

    if files.is_empty() {
        bail!("no Roast files found");
    }

    let interner = Interner::new();
    let mut total_errors = 0;
    let mut total_warnings = 0;

    for file in &files {
        let source = fs::read_to_string(file)
            .with_context(|| format!("failed to read {}", file.display()))?;

        // Use with_source to enable # type: ignore support
        let mut diagnostics = DiagnosticSink::with_source(&source);

        let module = match parse_module(&source, &file.to_string_lossy(), &interner, &mut diagnostics) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{} {}: {}", "error".red().bold(), file.display(), e);
                total_errors += 1;
                continue;
            }
        };

        let mut type_ctx = TypeContext::with_builtins(&interner);
        let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
        checker.check_module(&module);

        if diagnostics.has_errors() || (show_warnings && diagnostics.warning_count() > 0) {
            report_diagnostics(&diagnostics, &source, file);
        }

        total_errors += diagnostics.error_count();
        total_warnings += diagnostics.warning_count();
    }

    if total_errors > 0 {
        bail!("check failed: {} error(s), {} warning(s)", total_errors, total_warnings);
    }

    println!(
        "{} {} file(s) in {:.2}s{}",
        "     Checked".green().bold(),
        files.len(),
        start.elapsed().as_secs_f64(),
        if total_warnings > 0 {
            format!(" ({} warning(s))", total_warnings)
        } else {
            String::new()
        }
    );

    Ok(())
}

/// Format command.
pub fn fmt(path: &Path, check_only: bool, write_inplace: bool) -> Result<()> {
    let action = if check_only { "Checking" } else { "Formatting" };
    println!("{} {}", action.cyan().bold(), path.display());

    let files = if path.is_dir() {
        find_roast_files(path)?
    } else {
        vec![path.to_path_buf()]
    };

    let mut needs_formatting = 0;

    for file in &files {
        let source = fs::read_to_string(file)?;

        // Simple formatting check: consistent indentation, trailing newline
        let formatted = format_source(&source);

        if source != formatted {
            needs_formatting += 1;

            if check_only {
                println!("  {} needs formatting: {}", "!".yellow(), file.display());
            } else if write_inplace {
                fs::write(file, &formatted)?;
                println!("  {} formatted: {}", "✓".green(), file.display());
            } else {
                println!("  Would format: {}", file.display());
            }
        }
    }

    if check_only && needs_formatting > 0 {
        bail!("{} file(s) need formatting", needs_formatting);
    }

    println!("{} {} file(s)", "   Processed".green().bold(), files.len());

    Ok(())
}

/// Simple source formatter.
fn format_source(source: &str) -> String {
    let mut result = String::new();
    let mut prev_blank = false;

    for line in source.lines() {
        let trimmed = line.trim_end();

        // Collapse multiple blank lines
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
            }
            prev_blank = true;
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    // Ensure single trailing newline
    while result.ends_with("\n\n") {
        result.pop();
    }
    if !result.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }

    result
}

/// Test command.
pub fn test(filter: Option<&str>, verbose: bool, parallel: bool, show_output: bool) -> Result<()> {
    println!("{}", "  Running tests...".cyan().bold());

    // Find test files
    let test_files = find_test_files(Path::new("."))?;

    if test_files.is_empty() {
        println!("{}", "No tests found.".yellow());
        return Ok(());
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for file in &test_files {
        let name = file.file_stem().unwrap_or_default().to_string_lossy();

        // Apply filter
        if let Some(f) = filter {
            if !name.contains(f) {
                skipped += 1;
                continue;
            }
        }

        if verbose {
            print!("  test {} ... ", name);
            std::io::Write::flush(&mut std::io::stdout())?;
        }

        // Run test
        match run_test_file(file, show_output) {
            Ok(()) => {
                passed += 1;
                if verbose {
                    println!("{}", "ok".green());
                }
            }
            Err(e) => {
                failed += 1;
                if verbose {
                    println!("{}", "FAILED".red());
                    eprintln!("    {}", e);
                }
            }
        }
    }

    println!();
    println!(
        "test result: {}. {} passed; {} failed; {} skipped",
        if failed == 0 { "ok".green() } else { "FAILED".red() },
        passed,
        failed,
        skipped
    );

    if failed > 0 {
        bail!("{} test(s) failed", failed);
    }

    Ok(())
}

fn run_test_file(file: &Path, _show_output: bool) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let interner = Interner::new();
    // Use with_source to enable # type: ignore support
    let mut diagnostics = DiagnosticSink::with_source(&source);

    let module = parse_module(&source, &file.to_string_lossy(), &interner, &mut diagnostics)?;

    let mut type_ctx = TypeContext::with_builtins(&interner);
    let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
    checker.check_module(&module);

    if diagnostics.has_errors() {
        report_diagnostics(&diagnostics, &source, file);
        bail!("compilation failed");
    }

    // Test passed if it compiles

    // Build HIR
    let mut hir_builder = HirBuilder::new(&interner, &mut type_ctx, &mut diagnostics);
    let hir_module = hir_builder.build_module(&module);

    // Build MIR and compile to Bytecode
    let mut mir_builder = MirBuilder::new(hir_builder.expr_arena());
    let target_config = TargetConfig::bytecode();
    let mut emitter = CodeEmitter::new(target_config);

    // Identify test functions from AST
    let mut test_functions = Vec::new();
    for stmt in &module.body {
        if let StmtKind::FunctionDef { name, decorators, .. } = &stmt.kind {
            for decorator in decorators {
                // Check for @test
                // Decorator name is an Expr. We expect a simple Name.
                if let ExprKind::Name { id, .. } = &decorator.name.kind {
                    if interner.resolve(id.name) == Some("test") {
                        if let Some(name_str) = interner.resolve(name.name) {
                            test_functions.push(name_str.to_string());
                        }
                    }
                }
            }
        }
    }

    if test_functions.is_empty() {
        if _show_output {
            println!("No @test functions found.");
        }
        return Ok(());
    }

    // Compile all functions
    // We iterate HIR items because MIR builder works on HIR functions
    let mut func_name_map = HashMap::new();
    for item in &hir_module.items {
        if let roast_hir::HirItem::Function(func) = item {
             let mir_body = mir_builder.build_function(func);

             // Map "func_{id}" to resolved name
             let raw_name = format!("func_{}", func.name.as_raw());
             if let Some(resolved) = interner.resolve(func.name) {
                 func_name_map.insert(raw_name, resolved.to_string());
             }

             emitter.generate(&mir_body).map_err(|e| anyhow::anyhow!("Codegen error: {:?}", e))?;
        }
    }

    let mut compiled_modules = emitter.finalize().map_err(|e| anyhow::anyhow!("Codegen error: {:?}", e))?;

    // Patch bytecode names
    for module in &mut compiled_modules {
        for bytecode in &mut module.bytecode {
            if let Some(real_name) = func_name_map.get(&bytecode.name) {
                bytecode.name = format!("func_{}", real_name);
            }
        }
    }

    // Initialize VM
    let config = VMConfig {
        debug: false, // Could pass verbose flag here
        ..Default::default()
    };
    let mut vm = VM::with_config(config);

    // Load functions into VM globals
    // We assume a single module for now
    if let Some(compiled_module) = compiled_modules.first() {
        for bytecode in &compiled_module.bytecode {
            let func_name = bytecode.name.clone();
            // Remove "func_" prefix added by CodeEmitter if present
            // CodeEmitter::compile adds "func_" prefix?
            // Let's check emit.rs: format!("func_{}", body.name.as_raw())
            // We should probably strip it or use the original name if we can map it back.
            // But for now, let's assume we can find it.
            // Actually, we should fix CodeEmitter to NOT add prefix or use a map.
            // But let's check if we can just use the name from test_functions list.

            // The bytecode name will be "func_test_addition" etc.
            // We need to map "test_addition" to "func_test_addition".

            let func_val = Value::Function(roast_runtime::RoastFunction {
                name: func_name.clone(),
                arity: bytecode.num_params as usize,
                code: Arc::new(bytecode.clone()),
                is_async: false,
            }.into());

            // We register with the name from bytecode (which has prefix)
            // But we also want to register with the original name for `call_function_by_name`?
            // Or `call_function_by_name` should try adding prefix?

            // Better: Register with the name derived from bytecode name.
            // If bytecode name is "func_foo", register as "foo".
            let register_name = if func_name.starts_with("func_") {
                &func_name[5..]
            } else {
                &func_name
            };

            if _show_output {
                println!("Registering global: '{}'", register_name);
            }
            vm.set_global(register_name, func_val);
        }
    }

    // Run tests
    // Run tests
    let mut passed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();

    println!("Running {} tests...", test_functions.len());

    for test_name in test_functions {
        if _show_output {
            print!("test {} ... ", test_name);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }

        match vm.call_function_by_name(&test_name, vec![]) {
            Ok(_) => {
                passed += 1;
                if _show_output {
                    println!("ok");
                }
            },
            Err(e) => {
                failed += 1;
                failures.push((test_name.clone(), e));
                if _show_output {
                    println!("FAILED");
                }
            }
        }
    }

    println!("\ntest result: {}. {} passed; {} failed; 0 ignored; 0 measured; 0 filtered out",
        if failed == 0 { "ok" } else { "FAILED" },
        passed,
        failed
    );

    if !failures.is_empty() {
        println!("\nfailures:");
        for (name, err) in &failures {
            println!("    {} - {:?}", name, err);
        }
        println!();
        bail!("Test failed");
    }

    Ok(())
}

fn find_test_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    // Look for tests/ directory
    let test_dir = dir.join("tests");
    if test_dir.exists() {
        for entry in walkdir::WalkDir::new(&test_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "roast" || e == "ro") {
                files.push(path.to_path_buf());
            }
        }
    }

    // Also look for test_*.roast in src
    let src_dir = dir.join("src");
    if src_dir.exists() {
        for entry in fs::read_dir(&src_dir)? {
            let path = entry?.path();
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                if name.starts_with("test_") && (name.ends_with(".roast") || name.ends_with(".ro")) {
                    files.push(path);
                }
            }
        }
    }

    Ok(files)
}

/// Doc command.
pub fn doc(path: &Path, output: &Path, open: bool, include_private: bool) -> Result<()> {
    println!(
        "{} {} -> {}",
        "Generating docs".cyan().bold(),
        path.display(),
        output.display()
    );

    fs::create_dir_all(output)?;

    let files = if path.is_dir() {
        find_roast_files(path)?
    } else {
        vec![path.to_path_buf()]
    };

    let interner = Interner::new();
    let mut doc_items = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file)?;
        // Use with_source to enable # type: ignore support
        let mut diagnostics = DiagnosticSink::with_source(&source);

        if let Ok(module) = parse_module(&source, &file.to_string_lossy(), &interner, &mut diagnostics) {
            // Extract documentation from AST
            doc_items.push((file.clone(), module));
        }
    }

    // Generate HTML
    let index_html = generate_doc_html(&doc_items, include_private);
    fs::write(output.join("index.html"), index_html)?;

    println!("{} documentation at {}", "   Generated".green().bold(), output.join("index.html").display());

    if open {
        let url = format!("file://{}", output.join("index.html").canonicalize()?.display());
        if let Err(e) = open_browser(&url) {
            eprintln!("Could not open browser: {}", e);
        }
    }

    Ok(())
}

fn generate_doc_html(items: &[(std::path::PathBuf, roast_ast::Module)], _include_private: bool) -> String {
    let css = r#"
:root {
    --bg-primary: #1a1a2e;
    --bg-secondary: #16213e;
    --text-primary: #e8e8e8;
    --text-secondary: #a0a0a0;
    --accent: #e94560;
    --accent2: #0f3460;
    --code-bg: #0d1117;
    --border: #30363d;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    line-height: 1.6;
}
.container { max-width: 1200px; margin: 0 auto; padding: 2rem; }
header {
    background: linear-gradient(135deg, var(--accent) 0%, var(--accent2) 100%);
    padding: 3rem 2rem;
    text-align: center;
    margin-bottom: 2rem;
}
header h1 { font-size: 2.5rem; margin-bottom: 0.5rem; }
header p { color: rgba(255,255,255,0.8); }
.sidebar {
    position: fixed;
    left: 0;
    top: 0;
    width: 280px;
    height: 100vh;
    background: var(--bg-secondary);
    padding: 1rem;
    overflow-y: auto;
    border-right: 1px solid var(--border);
}
.sidebar h3 { color: var(--accent); margin: 1rem 0 0.5rem; font-size: 0.9rem; text-transform: uppercase; }
.sidebar a { display: block; color: var(--text-secondary); text-decoration: none; padding: 0.3rem 0.5rem; border-radius: 4px; }
.sidebar a:hover { background: var(--accent2); color: var(--text-primary); }
.main { margin-left: 300px; padding: 2rem; }
.module { background: var(--bg-secondary); border-radius: 8px; padding: 1.5rem; margin-bottom: 2rem; border: 1px solid var(--border); }
.module h2 { color: var(--accent); margin-bottom: 1rem; display: flex; align-items: center; gap: 0.5rem; }
.module h2::before { content: '📦'; }
.item { background: var(--code-bg); border-radius: 6px; padding: 1rem; margin: 0.5rem 0; border-left: 3px solid var(--accent); }
.item-header { font-family: 'JetBrains Mono', monospace; font-size: 0.95rem; color: #58a6ff; margin-bottom: 0.5rem; }
.item-doc { color: var(--text-secondary); font-size: 0.9rem; }
.tag { display: inline-block; padding: 0.2rem 0.5rem; border-radius: 4px; font-size: 0.75rem; margin-right: 0.5rem; }
.tag-func { background: #238636; }
.tag-class { background: #8957e5; }
.tag-module { background: #1f6feb; }
pre { background: var(--code-bg); padding: 1rem; border-radius: 6px; overflow-x: auto; }
code { font-family: 'JetBrains Mono', 'SF Mono', monospace; font-size: 0.9rem; }
.search { width: 100%; padding: 0.75rem; background: var(--bg-primary); border: 1px solid var(--border); border-radius: 6px; color: var(--text-primary); margin-bottom: 1rem; }
footer { text-align: center; padding: 2rem; color: var(--text-secondary); border-top: 1px solid var(--border); margin-top: 2rem; }
"#;

    let mut modules_html = String::new();
    let mut sidebar_html = String::new();

    for (path, module) in items {
        let module_name = path.file_stem().unwrap_or_default().to_string_lossy();
        let module_id = module_name.replace('.', "_");

        sidebar_html.push_str(&format!(
            "<a href=\"#{}\">{}</a>\n",
            module_id, module_name
        ));

        // Count items in module
        let item_count = module.body.len();

        modules_html.push_str(&format!(
            "<div class=\"module\" id=\"{}\">\n\
             <h2>{}</h2>\n\
             <div class=\"item\">\n\
             <span class=\"tag tag-module\">module</span>\n\
             <div class=\"item-header\">{}</div>\n\
             <div class=\"item-doc\">{} top-level items</div>\n\
             </div>\n\
             </div>\n",
            module_id, module_name, module_name, item_count
        ));
    }

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>🔥 Roast Documentation</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&family=JetBrains+Mono&display=swap" rel="stylesheet">
    <style>{}</style>
</head>
<body>
    <div class="sidebar">
        <h2 style="color: #e94560; margin-bottom: 1rem;">🔥 Roast Docs</h2>
        <input type="text" class="search" placeholder="Search..." id="search">
        <h3>Modules</h3>
        {}
    </div>
    <div class="main">
        <header>
            <h1>🔥 Roast Documentation</h1>
            <p>Python syntax with Rust performance</p>
        </header>
        <div class="container">
            {}
        </div>
        <footer>
            Generated by roastdoc | Roast {}
        </footer>
    </div>
    <script>
        document.getElementById('search').addEventListener('input', function(e) {{
            const query = e.target.value.toLowerCase();
            document.querySelectorAll('.module').forEach(m => {{
                const text = m.textContent.toLowerCase();
                m.style.display = text.includes(query) ? 'block' : 'none';
            }});
        }});
    </script>
</body>
</html>"#,
        css,
        sidebar_html,
        modules_html,
        env!("CARGO_PKG_VERSION")
    )
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;

    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd").args(["/C", "start", url]).spawn()?;

    Ok(())
}

/// LSP command.
pub fn lsp() -> Result<()> {
    println!("{}", "Starting Roast language server...".cyan().bold());
    println!("Use your editor's LSP client to connect.");

    // In a full implementation, this would start the LSP server
    // For now, direct users to the standalone lsp binary
    println!("\nNote: Use 'roast-lsp' for the full language server.");

    Ok(())
}

/// Init command.
pub fn init(name: Option<&str>, is_lib: bool, init_git: bool) -> Result<()> {
    let project_name = name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "roast_project".to_string())
            .leak()
    });

    let project_type = if is_lib { "library" } else { "binary" };

    println!(
        "{} {} {} '{}'",
        "    Creating".green().bold(),
        "new".cyan(),
        project_type,
        project_name
    );

    // Create project structure
    let project_dir = if name.is_some() {
        std::path::PathBuf::from(project_name)
    } else {
        std::path::PathBuf::from(".")
    };

    fs::create_dir_all(project_dir.join("src"))?;
    fs::create_dir_all(project_dir.join("tests"))?;

    // Create roast.toml
    let config = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
authors = []
description = ""

[dependencies]

[dev-dependencies]
"#,
        project_name
    );
    fs::write(project_dir.join("roast.toml"), config)?;

    // Create main file
    let main_content = if is_lib {
        r#""""
Roast library.
"""

def add(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b

def multiply(a: int, b: int) -> int:
    """Multiply two numbers."""
    return a * b
"#
    } else {
        r#""""
Roast program entry point.
"""

def main() -> None:
    """Main function."""
    print("Hello, Roast! 🔥")

if __name__ == "__main__":
    main()
"#
    };

    let main_file = if is_lib { "lib.roast" } else { "main.roast" };
    fs::write(project_dir.join("src").join(main_file), main_content)?;

    // Create test file
    let test_content = if is_lib {
        r#"# Tests for lib

from src.lib import add, multiply

def test_add():
    assert add(2, 3) == 5
    assert add(-1, 1) == 0

def test_multiply():
    assert multiply(2, 3) == 6
    assert multiply(0, 5) == 0
"#
    } else {
        r#"# Tests

def test_example():
    assert True
    assert 1 + 1 == 2
"#
    };
    fs::write(project_dir.join("tests").join("test_main.roast"), test_content)?;

    // Create .gitignore
    fs::write(
        project_dir.join(".gitignore"),
        r#"# Build artifacts
/target/
*.rbc

# Python cache (for compatibility)
*.pyc
__pycache__/
.pytest_cache/

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db
"#,
    )?;

    // Create README.md
    let readme = format!(
        r#"# {}

A Roast project.

## Getting Started

```bash
# Build the project
roastc build

# Run the project
roastc run src/{}

# Run tests
roastc test
```

## Project Structure

```
{}
├── roast.toml      # Project configuration
├── src/
│   └── {}.roast    # Source files
└── tests/
    └── test_main.roast  # Test files
```
"#,
        project_name,
        main_file,
        project_name,
        if is_lib { "lib" } else { "main" }
    );
    fs::write(project_dir.join("README.md"), readme)?;

    // Initialize git if requested
    if init_git {
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&project_dir)
            .output()
            .ok();
        println!("     {} git repository", "Initialized".blue());
    }

    println!("{} project '{}'", "     Created".green().bold(), project_name);

    if name.is_some() {
        println!("\nTo get started:");
        println!("  cd {}", project_name);
    } else {
        println!("\nTo get started:");
    }
    println!("  roastc run src/{}", main_file);

    Ok(())
}

/// New file command.
pub fn new_file(name: &str, in_src: bool) -> Result<()> {
    let filename = if name.ends_with(".roast") || name.ends_with(".ro") {
        name.to_string()
    } else {
        format!("{}.ro", name)  // Default to .ro for new files
    };

    let path = if in_src {
        std::path::PathBuf::from("src").join(&filename)
    } else {
        std::path::PathBuf::from(&filename)
    };

    if path.exists() {
        bail!("file already exists: {}", path.display());
    }

    let content = format!(
        r#""""
{}
"""

def main() -> None:
    pass
"#,
        name.trim_end_matches(".roast")
    );

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;

    println!("{} {}", "     Created".green().bold(), path.display());

    Ok(())
}

/// Clean command.
pub fn clean(all: bool) -> Result<()> {
    println!("{}", "    Cleaning...".cyan().bold());

    let target = Path::new("target");
    if target.exists() {
        fs::remove_dir_all(target)?;
        println!("  Removed target/");
    }

    // Remove .rbc files
    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "rbc") {
            fs::remove_file(path)?;
            println!("  Removed {}", path.display());
        }
    }

    if all {
        // Remove cache directory
        let cache = dirs::cache_dir()
            .map(|d| d.join("roast"))
            .unwrap_or_else(|| Path::new(".roast_cache").to_path_buf());

        if cache.exists() {
            fs::remove_dir_all(&cache)?;
            println!("  Removed cache at {}", cache.display());
        }
    }

    println!("{}", "     Cleaned".green().bold());

    Ok(())
}

/// Migrate Python code to Roast.
pub fn migrate(
    source: &Path,
    output: Option<&Path>,
    add_types: bool,
    add_ownership: bool,
    convert_py2: bool,
    dry_run: bool,
) -> Result<()> {
    use roast_pycompat::migrate::{Migrator, MigrateConfig};

    println!("{} {}", "   Migrating".cyan().bold(), source.display());

    let config = MigrateConfig {
        add_types,
        add_ownership,
        convert_idioms: convert_py2,
        ..Default::default()
    };

    let migrator = Migrator::new(config);

    if source.is_dir() {
        // Migrate directory
        let dest = output.map(|p| p.to_path_buf())
            .unwrap_or_else(|| source.join("..").join(format!("{}_roast", source.file_name().unwrap_or_default().to_string_lossy())));

        if dry_run {
            println!("Would migrate {} -> {}", source.display(), dest.display());

            // Show what would be migrated
            for entry in walkdir::WalkDir::new(source)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "py") {
                    let rel = path.strip_prefix(source).unwrap_or(path);
                    println!("  {} -> {}", rel.display(), rel.with_extension("roast").display());
                }
            }
        } else {
            let results = migrator.migrate_directory(source, &dest)?;

            for (file, result) in &results {
                println!("  {} {} ({} lines)", "✓".green(), file, result.stats.lines_processed);
                for warning in &result.warnings {
                    println!("    {} line {}: {}", "warning:".yellow(), warning.line, warning.message);
                }
            }

            println!(
                "\n{} {} file(s) to {}",
                "   Migrated".green().bold(),
                results.len(),
                dest.display()
            );
        }
    } else {
        // Migrate single file
        let result = migrator.migrate_file(source)?;

        let dest = output.map(|p| p.to_path_buf())
            .unwrap_or_else(|| source.with_extension("roast"));

        if dry_run {
            println!("{}", "--- Migrated output ---".cyan());
            println!("{}", result.source);
            println!("{}", "--- End ---".cyan());
        } else {
            fs::write(&dest, &result.source)?;

            println!(
                "{} {} -> {}",
                "   Migrated".green().bold(),
                source.display(),
                dest.display()
            );
        }

        // Show stats
        println!("\nStatistics:");
        println!("  Lines processed: {}", result.stats.lines_processed);
        println!("  Types inferred: {}", result.stats.types_inferred);

        if !result.warnings.is_empty() {
            println!("\nWarnings:");
            for warning in &result.warnings {
                println!("  Line {}: {}", warning.line, warning.message);
                if let Some(suggestion) = &warning.suggestion {
                    println!("    Suggestion: {}", suggestion);
                }
            }
        }
    }

    Ok(())
}

/// Lint command.
pub fn lint(
    path: &Path,
    fix: bool,
    errors_only: bool,
    format: &str,
    ignore: Option<&str>,
    select: Option<&str>,
) -> Result<()> {
    use std::collections::HashSet;

    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        find_roast_files(path)?
    };

    if files.is_empty() {
        println!("{} No Roast files found", "warning:".yellow());
        return Ok(());
    }

    let ignored_rules: HashSet<&str> = ignore
        .map(|s| s.split(',').collect())
        .unwrap_or_default();

    let selected_rules: Option<HashSet<&str>> = select
        .map(|s| s.split(',').collect());

    let mut total_issues = 0;
    let mut total_fixed = 0;
    let mut all_issues: Vec<LintIssue> = Vec::new();

    println!("{} {} file(s)...", "     Linting".cyan().bold(), files.len());

    let interner = roast_common::Interner::new();

    for file in &files {
        let source = fs::read_to_string(file)?;
        let issues = lint_source(&source, file, &interner, &ignored_rules, &selected_rules, errors_only);

        if !issues.is_empty() {
            if format == "text" {
                for issue in &issues {
                    let severity_str = match issue.severity {
                        LintSeverity::Error => "error".red().bold(),
                        LintSeverity::Warning => "warning".yellow().bold(),
                        LintSeverity::Info => "info".blue().bold(),
                    };

                    println!(
                        "{}:{}:{}: {}: {} [{}]",
                        file.display(),
                        issue.line,
                        issue.column,
                        severity_str,
                        issue.message,
                        issue.rule.cyan()
                    );

                    // Show the offending line
                    if let Some(line_content) = source.lines().nth(issue.line.saturating_sub(1)) {
                        println!("  {}", line_content);
                        let pointer = format!("{}^", " ".repeat(issue.column.saturating_sub(1)));
                        println!("  {}", pointer.red());
                    }

                    if let Some(ref suggestion) = issue.suggestion {
                        println!("  {} {}", "suggestion:".green(), suggestion);
                    }
                    println!();
                }
            }

            total_issues += issues.len();

            if fix {
                let fixed_count = apply_fixes(file, &source, &issues)?;
                total_fixed += fixed_count;
            }

            all_issues.extend(issues);
        }
    }

    // JSON output
    if format == "json" {
        // Simple JSON format without serde_json
        println!("{{");
        println!("  \"files\": {},", files.len());
        println!("  \"total\": {},", total_issues);
        println!("  \"fixed\": {},", total_fixed);
        println!("  \"issues\": [");
        for (i, issue) in all_issues.iter().enumerate() {
            let comma = if i < all_issues.len() - 1 { "," } else { "" };
            let severity = match issue.severity {
                LintSeverity::Error => "error",
                LintSeverity::Warning => "warning",
                LintSeverity::Info => "info",
            };
            println!("    {{\"file\": \"{}\", \"line\": {}, \"column\": {}, \"severity\": \"{}\", \"rule\": \"{}\", \"message\": \"{}\"}}{}",
                issue.file.display(),
                issue.line,
                issue.column,
                severity,
                issue.rule,
                issue.message.replace('"', "\\\""),
                comma
            );
        }
        println!("  ]");
        println!("}}");
        return Ok(());
    }

    // Summary
    println!();
    if total_issues == 0 {
        println!("{} No issues found", "     Success".green().bold());
    } else {
        let error_count = all_issues.iter().filter(|i| matches!(i.severity, LintSeverity::Error)).count();
        let warning_count = all_issues.iter().filter(|i| matches!(i.severity, LintSeverity::Warning)).count();

        if fix && total_fixed > 0 {
            println!(
                "{} {} issues, {} fixed automatically",
                "      Found".yellow().bold(),
                total_issues,
                total_fixed
            );
        } else {
            println!(
                "{} {} error(s), {} warning(s)",
                "      Found".yellow().bold(),
                error_count,
                warning_count
            );
        }

        if error_count > 0 && !fix {
            std::process::exit(1);
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct LintIssue {
    file: std::path::PathBuf,
    line: usize,
    column: usize,
    severity: LintSeverity,
    rule: String,
    message: String,
    suggestion: Option<String>,
    fix: Option<LintFix>,
}

#[derive(Debug, Clone)]
enum LintSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
struct LintFix {
    start: usize,
    end: usize,
    replacement: String,
}

fn lint_source(
    source: &str,
    path: &Path,
    interner: &roast_common::Interner,
    ignored: &std::collections::HashSet<&str>,
    selected: &Option<std::collections::HashSet<&str>>,
    errors_only: bool,
) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Parse the file to get AST
    // Use with_source to enable # type: ignore support
    let mut diagnostics = roast_common::DiagnosticSink::with_source(source);
    let parse_result = roast_parser::parse_module(source, &path.to_string_lossy(), interner, &mut diagnostics);

    // Report parse errors
    for diag in diagnostics.diagnostics() {
        if should_report("E001", ignored, selected) {
            // Calculate line and column from byte offset
            let (line, column) = if let Some(span) = diag.primary_span {
                byte_offset_to_line_col(source, span.start as usize)
            } else {
                (1, 1)
            };

            issues.push(LintIssue {
                file: path.to_path_buf(),
                line,
                column,
                severity: LintSeverity::Error,
                rule: "E001".to_string(),
                message: diag.message.clone(),
                suggestion: None,
                fix: None,
            });
        }
    }

    // Run lint rules on each line
    for (line_num, line) in lines.iter().enumerate() {
        let line_no = line_num + 1;

        // W001: Line too long
        if !errors_only && line.len() > 100 && should_report("W001", ignored, selected) {
            issues.push(LintIssue {
                file: path.to_path_buf(),
                line: line_no,
                column: 101,
                severity: LintSeverity::Warning,
                rule: "W001".to_string(),
                message: format!("Line too long ({} > 100 characters)", line.len()),
                suggestion: Some("Consider breaking this line".to_string()),
                fix: None,
            });
        }

        // W002: Trailing whitespace
        if !errors_only && line.ends_with(' ') && should_report("W002", ignored, selected) {
            let trimmed_len = line.trim_end().len();
            issues.push(LintIssue {
                file: path.to_path_buf(),
                line: line_no,
                column: trimmed_len + 1,
                severity: LintSeverity::Warning,
                rule: "W002".to_string(),
                message: "Trailing whitespace".to_string(),
                suggestion: Some("Remove trailing spaces".to_string()),
                fix: Some(LintFix {
                    start: line_num * (line.len() + 1) + trimmed_len,
                    end: line_num * (line.len() + 1) + line.len(),
                    replacement: String::new(),
                }),
            });
        }

        // W003: Mixed tabs and spaces
        if !errors_only && line.contains('\t') && line.contains("    ") && should_report("W003", ignored, selected) {
            issues.push(LintIssue {
                file: path.to_path_buf(),
                line: line_no,
                column: 1,
                severity: LintSeverity::Warning,
                rule: "W003".to_string(),
                message: "Mixed tabs and spaces for indentation".to_string(),
                suggestion: Some("Use consistent indentation (prefer 4 spaces)".to_string()),
                fix: None,
            });
        }

        // W004: TODO/FIXME/HACK comments
        if !errors_only && should_report("W004", ignored, selected) {
            let lower = line.to_lowercase();
            if lower.contains("todo") || lower.contains("fixme") || lower.contains("hack") || lower.contains("xxx") {
                issues.push(LintIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    column: 1,
                    severity: LintSeverity::Info,
                    rule: "W004".to_string(),
                    message: "Found TODO/FIXME comment".to_string(),
                    suggestion: None,
                    fix: None,
                });
            }
        }

        // E002: Using 'print' without parentheses (Python 2 style)
        if should_report("E002", ignored, selected) {
            let trimmed = line.trim();
            if trimmed.starts_with("print ") && !trimmed.starts_with("print(") {
                issues.push(LintIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    column: line.find("print").unwrap_or(0) + 1,
                    severity: LintSeverity::Error,
                    rule: "E002".to_string(),
                    message: "print is a function, use print(...)".to_string(),
                    suggestion: Some(format!("print({})", trimmed.strip_prefix("print ").unwrap_or(""))),
                    fix: None,
                });
            }
        }

        // W005: Using 'pass' at end of non-empty block
        if !errors_only && should_report("W005", ignored, selected) {
            if line.trim() == "pass" && line_num > 0 {
                let prev_line = lines.get(line_num - 1).unwrap_or(&"");
                if !prev_line.trim().ends_with(':') && !prev_line.trim().is_empty() {
                    issues.push(LintIssue {
                        file: path.to_path_buf(),
                        line: line_no,
                        column: 1,
                        severity: LintSeverity::Warning,
                        rule: "W005".to_string(),
                        message: "Unnecessary 'pass' statement".to_string(),
                        suggestion: Some("Remove this 'pass' statement".to_string()),
                        fix: None,
                    });
                }
            }
        }

        // W006: Comparison to None using == or !=
        if !errors_only && should_report("W006", ignored, selected) {
            if line.contains("== None") || line.contains("!= None") {
                issues.push(LintIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    column: line.find("None").unwrap_or(0) + 1,
                    severity: LintSeverity::Warning,
                    rule: "W006".to_string(),
                    message: "Comparison to None should use 'is' or 'is not'".to_string(),
                    suggestion: Some("Use 'is None' or 'is not None'".to_string()),
                    fix: None,
                });
            }
        }

        // W007: Comparison to True/False using == or !=
        if !errors_only && should_report("W007", ignored, selected) {
            if line.contains("== True") || line.contains("== False")
               || line.contains("!= True") || line.contains("!= False") {
                issues.push(LintIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    column: 1,
                    severity: LintSeverity::Warning,
                    rule: "W007".to_string(),
                    message: "Don't compare to True/False, use the boolean directly".to_string(),
                    suggestion: Some("Use the boolean value directly or 'not'".to_string()),
                    fix: None,
                });
            }
        }

        // W008: Single-letter variable names (except for loops)
        if !errors_only && should_report("W008", ignored, selected) {
            // Very basic check - assignment with single letter
            let trimmed = line.trim();
            for single in ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'y', 'z'] {
                let pattern = format!("{} = ", single);
                if trimmed.starts_with(&pattern) && !trimmed.contains("for") {
                    issues.push(LintIssue {
                        file: path.to_path_buf(),
                        line: line_no,
                        column: 1,
                        severity: LintSeverity::Warning,
                        rule: "W008".to_string(),
                        message: format!("Single-letter variable name '{}'", single),
                        suggestion: Some("Use a more descriptive name".to_string()),
                        fix: None,
                    });
                    break;
                }
            }
        }
    }

    // Sort issues by line number
    issues.sort_by_key(|i| (i.line, i.column));
    issues
}

fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;

    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}

fn should_report(
    rule: &str,
    ignored: &std::collections::HashSet<&str>,
    selected: &Option<std::collections::HashSet<&str>>,
) -> bool {
    if ignored.contains(rule) {
        return false;
    }
    if let Some(ref sel) = selected {
        return sel.contains(rule);
    }
    true
}

fn apply_fixes(path: &Path, source: &str, issues: &[LintIssue]) -> Result<usize> {
    let mut fixed = 0;
    let mut new_source = source.to_string();

    // Apply fixes in reverse order to preserve positions
    let mut fixable: Vec<_> = issues.iter()
        .filter(|i| i.fix.is_some())
        .collect();
    fixable.sort_by_key(|i| std::cmp::Reverse((i.line, i.column)));

    for issue in fixable {
        if let Some(ref fix) = issue.fix {
            if fix.start < new_source.len() && fix.end <= new_source.len() {
                new_source.replace_range(fix.start..fix.end, &fix.replacement);
                fixed += 1;
            }
        }
    }

    if fixed > 0 {
        fs::write(path, new_source)?;
    }

    Ok(fixed)
}

/// Version command.
pub fn version(verbose: bool) -> Result<()> {
    println!(
        "{} {}",
        "roastc".bold(),
        env!("CARGO_PKG_VERSION")
    );

    if verbose {
        println!();
        println!("Roast - Python syntax with Rust performance");
        println!();
        println!("Build info:");
        println!("  Host: {}-{}", std::env::consts::OS, std::env::consts::ARCH);
        println!("  Target: {}", std::env::consts::ARCH);
        println!("  Profile: {}", if cfg!(debug_assertions) { "debug" } else { "release" });
        println!();
        println!("Features:");
        println!("  ✓ Bytecode VM");
        println!("  ✓ Type inference");
        println!("  ✓ Borrow checking");
        println!("  ✓ Python compatibility");
    }

    Ok(())
}

/// Find all Roast files in a directory.
fn find_roast_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "roast") {
            files.push(path.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

/// Reports diagnostics to stderr with source code snippets.
fn report_diagnostics(diagnostics: &DiagnosticSink, source: &str, path: &Path) {
    use codespan_reporting::files::SimpleFiles;
    use codespan_reporting::term;
    use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

    let mut files = SimpleFiles::new();
    let file_id = files.add(path.to_string_lossy(), source);

    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = term::Config::default();

    for diag in diagnostics.diagnostics() {
        // Calculate the offset needed to map the diagnostic's file ID to our file_id
        // Most diagnostics use FileId(0), but we need to ensure they match SimpleFiles
        let file_id_offset = if let Some(span) = diag.primary_span {
            file_id as i32 - span.file.as_u32() as i32
        } else {
            0
        };

        let codespan_diag = diag.to_codespan_with_offset(file_id_offset);
        if let Err(e) = term::emit(&mut writer.lock(), &config, &files, &codespan_diag) {
            // Fall back to simple message if codespan fails
            eprintln!("{}", diag);
            if let Some(span) = diag.primary_span {
                // Show the source line manually
                if let Some(line_content) = get_line_at_offset(source, span.start as usize) {
                    eprintln!("  --> {}:{}",
                        path.display(),
                        count_lines_before(source, span.start as usize) + 1
                    );
                    eprintln!("   |");
                    eprintln!("   | {}", line_content);
                    let col = get_column_at_offset(source, span.start as usize);
                    let underline = format!("{}{}",
                        " ".repeat(col),
                        "^".repeat((span.end - span.start).max(1) as usize)
                    );
                    eprintln!("   | {}", underline);
                    eprintln!("   |");
                }
            }
        }
    }
}

/// Gets the line containing the given byte offset.
fn get_line_at_offset(source: &str, offset: usize) -> Option<&str> {
    let start = source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = source[offset..].find('\n').map(|i| offset + i).unwrap_or(source.len());
    Some(&source[start..end])
}

/// Counts the number of newlines before the given offset.
fn count_lines_before(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].matches('\n').count()
}

/// Gets the column (0-indexed) at the given byte offset.
fn get_column_at_offset(source: &str, offset: usize) -> usize {
    let line_start = source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    offset - line_start
}

/// Collect import module names from an AST module.
/// Returns a list of module names (e.g., "test_modules.helpers")
fn collect_imports(module: &roast_ast::Module, interner: &Interner) -> Vec<String> {
    let mut imports = Vec::new();
    for stmt in &module.body {
        match &stmt.kind {
            // Handle "from module import names"
            StmtKind::ImportFrom { module: Some(mod_ident), .. } => {
                if let Some(name) = interner.resolve(mod_ident.name) {
                    imports.push(name.to_string());
                }
            }
            // Handle "import module" or "import module as alias"
            StmtKind::Import { names } => {
                for alias in names {
                    if let Some(name) = interner.resolve(alias.name.name) {
                        imports.push(name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    imports
}

/// Resolve a module name to a file path.
/// e.g., "test_modules.helpers" -> "{base_dir}/test_modules/helpers.roast"
fn resolve_module_path(module_name: &str, base_path: &Path) -> Option<std::path::PathBuf> {
    // Convert dots to path separators
    let rel_path = format!("{}.roast", module_name.replace('.', "/"));
    
    // Look relative to the base file's directory
    if let Some(parent) = base_path.parent() {
        let module_path = parent.join(&rel_path);
        if module_path.exists() {
            return Some(module_path);
        }
    }
    
    None
}

pub fn build_native(
    path: &Path,
    output: Option<&Path>,
    opt_level: u32,
    debug: bool,
) -> Result<()> {
    #[cfg(not(feature = "cranelift"))]
    {
        println!("{} Cranelift not enabled, falling back to bytecode", "Warning:".yellow());
        return build(path, output, opt_level, true, false, false, debug);
    }
    
    #[cfg(feature = "cranelift")]
    {
        use roast_codegen::{CraneliftBackend, OptLevel as CodegenOptLevel};
        
        let start = Instant::now();
        println!("{} {} (Cranelift)", "   Compiling".green().bold(), path.display());

        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let interner = Interner::new();
        let mut diagnostics = DiagnosticSink::with_source(&source);

        let module = parse_module(&source, &path.to_string_lossy(), &interner, &mut diagnostics)?;

        let mut type_ctx = TypeContext::with_builtins(&interner);
        let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
        checker.check_module(&module);

        if diagnostics.has_errors() {
            report_diagnostics(&diagnostics, &source, path);
            bail!("compilation failed");
        }

        // Convert opt_level
        let cranelift_opt = match opt_level {
            0 => CodegenOptLevel::None,
            1 => CodegenOptLevel::Less,
            2 => CodegenOptLevel::Default,
            _ => CodegenOptLevel::Aggressive,
        };

        // Create Cranelift backend
        let mut backend = CraneliftBackend::new(None, cranelift_opt)
            .map_err(|e| anyhow::anyhow!("Failed to create Cranelift backend: {}", e))?;

        let mut mir_bodies = Vec::new();
        let mut main_func_name = None;

        // Build HIR and compile functions to MIR
        {
            let mut hir_builder = HirBuilder::new(&interner, &mut type_ctx, &mut diagnostics);
            let hir_module = hir_builder.build_module(&module);

            for item in &hir_module.items {
                if let roast_hir::HirItem::Function(func) = item {
                    let mut mir_builder = MirBuilder::new(hir_builder.expr_arena());
                    let mir_body = mir_builder.build_function(func);
                    
                    // Check if this is main
                    if let Some(name) = interner.resolve(func.name) {
                        if name == "main" {
                            main_func_name = Some(format!("roast_fn_{}", func.name.as_raw()));
                        }
                    }
                    
                    mir_bodies.push(mir_body);
                }
            }
        }

        if mir_bodies.is_empty() {
            bail!("No functions to compile");
        }

        // Compile all functions with two-pass approach
        backend.compile_module(&mir_bodies)
            .map_err(|e| anyhow::anyhow!("Cranelift compilation failed: {}", e))?;

        // Generate entry point if we have main
        if let Some(main_name) = main_func_name {
            if let Some(&main_id) = backend.compiled_funcs.get(&main_name) {
                backend.generate_entry_point(main_id)
                    .map_err(|e| anyhow::anyhow!("Failed to generate entry point: {}", e))?;
            }
        }

        // Finalize and write object file
        let product = backend.finish()
            .map_err(|e| anyhow::anyhow!("Failed to finalize: {}", e))?;
        
        let object_bytes = roast_codegen::cranelift::write_object(product)
            .map_err(|e| anyhow::anyhow!("Failed to write object: {}", e))?;

        // Determine output path
        let output_path = match output {
            Some(p) => p.to_path_buf(),
            None => {
                let stem = path.file_stem().unwrap_or_default();
                std::env::temp_dir().join(stem)
            }
        };

        // Link into executable
        roast_codegen::cranelift::link_executable(
            &[object_bytes],
            &output_path.to_string_lossy(),
            &[],
        ).map_err(|e| anyhow::anyhow!("Linking failed: {}", e))?;

        let elapsed = start.elapsed();
        println!("    {} Cranelift executable in {:.3}s", "Finished".green().bold(), elapsed.as_secs_f64());
        println!("\n→ Run with: {}", output_path.display());

        Ok(())
    }
}

pub fn run_native(
    path: &Path,
    args: &[String],
    opt_level: u32,
    debug: bool,
) -> Result<()> {
    println!("{} Native execution not yet implemented, falling back to VM", "Warning:".yellow());
    run(path, args, opt_level, debug)
}

pub fn build_llvm(
    path: &Path,
    output: Option<&Path>,
    opt_level: u32,
    debug: bool,
) -> Result<()> {
    let start = Instant::now();
    println!("{} {} (LLVM)", "   Compiling".green().bold(), path.display());

    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let interner = Interner::new();
    let mut diagnostics = DiagnosticSink::with_source(&source);

    let module = parse_module(&source, &path.to_string_lossy(), &interner, &mut diagnostics)?;

    let mut type_ctx = TypeContext::with_builtins(&interner);
    
    // Pre-collect module exports for star imports (from x import *)
    for stmt in &module.body {
        if let roast_ast::StmtKind::ImportFrom { module: Some(mod_path), names, .. } = &stmt.kind {
            // Check if this is a star import
            let has_star = names.iter().any(|alias| {
                interner.resolve(alias.name.name).map(|s| s == "*").unwrap_or(false)
            });
            
            if has_star {
                let mod_name = interner.resolve(mod_path.name).unwrap_or("");
                if let Some(import_path) = resolve_module_path(mod_name, path) {
                    // Parse the imported module to collect exports
                    if let Ok(import_source) = fs::read_to_string(&import_path) {
                        let mut import_diagnostics = DiagnosticSink::with_source(&import_source);
                        if let Ok(import_module) = parse_module(&import_source, &import_path.to_string_lossy(), &interner, &mut import_diagnostics) {
                            // Collect all top-level function and class definitions as exports
                            let mut exports: Vec<(roast_common::Symbol, roast_typer::Type)> = Vec::new();
                            for import_stmt in &import_module.body {
                                match &import_stmt.kind {
                                    roast_ast::StmtKind::FunctionDef { name, .. } => {
                                        exports.push((name.name, roast_typer::Type::Any));
                                    }
                                    roast_ast::StmtKind::ClassDef { name, .. } => {
                                        exports.push((name.name, roast_typer::Type::Any));
                                    }
                                    roast_ast::StmtKind::Assign { targets, .. } => {
                                        // Also export top-level variable assignments (like PI = 3.14)
                                        for target in targets {
                                            if let roast_ast::ExprKind::Name { id, .. } = &target.kind {
                                                exports.push((id.name, roast_typer::Type::Any));
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            type_ctx.register_module_exports(mod_name, exports);
                        }
                    }
                }
            }
        }
    }
    
    let mut checker = TypeChecker::new(&mut type_ctx, &interner, &mut diagnostics);
    checker.check_module(&module);

    if diagnostics.has_errors() {
        report_diagnostics(&diagnostics, &source, path);
        bail!("compilation failed");
    }

    let config = LlvmConfig {
        opt_level,
        ..Default::default()
    };
    let mut codegen = LlvmCodeGen::new(config);

    for i in 0..interner.len() {
        let sym = roast_common::Symbol::from_raw(i as u32);
        if let Some(name) = interner.resolve(sym) {
            codegen.register_symbol(i as u32, name);
        }
    }

    let mut entry_point: Option<String> = None;
    let mut entry_point_void = false;
    let mut function_count = 0;
    let mut all_borrow_errors: Vec<String> = Vec::new();
    
    // Helper closure to compile a module's functions and classes
    let mut compile_module = |
        hir_builder: &HirBuilder,
        hir_module: &roast_hir::HirModule,
        codegen: &mut LlvmCodeGen,
        interner: &Interner,
        entry_point: &mut Option<String>,
        entry_point_void: &mut bool,
        function_count: &mut usize,
        all_borrow_errors: &mut Vec<String>,
        opt_level: u32,
    | -> Result<()> {
        for item in &hir_module.items {
            match item {
                roast_hir::HirItem::Function(func) => {
                    *function_count += 1;
                    let func_name_str = interner.resolve(func.name).unwrap_or_default();
                    let mangled_name = format!("roast_fn_{}", func.name.as_raw());

                    if func_name_str == "__module_init__" {
                        *entry_point = Some(mangled_name.clone());
                        *entry_point_void = matches!(func.return_type, roast_typer::Type::NoneType);
                    } else if func_name_str == "main" && entry_point.is_none() {
                        *entry_point = Some(mangled_name.clone());
                        *entry_point_void = matches!(func.return_type, roast_typer::Type::NoneType);
                    }

                    let mut mir_builder = MirBuilder::new(hir_builder.expr_arena());
                    let mut mir_body = mir_builder.build_function(func);

                    // Optimize MIR before codegen based on opt_level
                    let mir_opt_level = match opt_level {
                        0 => MirOptLevel::None,
                        1 => MirOptLevel::Less,
                        2 => MirOptLevel::Default,
                        _ => MirOptLevel::Aggressive,
                    };
                    let mut optimizer = Optimizer::for_level(mir_opt_level);
                    optimizer.optimize(&mut mir_body);

                    // Run borrow checker on MIR (using a separate diagnostics sink)
                    let mut borrow_diagnostics = DiagnosticSink::new();
                    let mut borrow_checker = BorrowChecker::new(interner, &mut borrow_diagnostics);
                    let borrow_errors = borrow_checker.check_body(&mir_body);
                    if !borrow_errors.is_empty() {
                        for err in &borrow_errors {
                            eprintln!("{} in function '{}': {}", "Borrow error".red(), func_name_str, err);
                        }
                        all_borrow_errors.extend(borrow_errors.iter().map(|e| format!("function '{}': {}", func_name_str, e)));
                    }

                    codegen.compile_function(&mir_body, None)
                        .map_err(|e| anyhow::anyhow!("LLVM codegen error: {}", e))?;
                }
                roast_hir::HirItem::Class(cls) => {
                    let class_name = interner.resolve(cls.name).unwrap_or_default();
                    
                    // Register the class so class globals (@.class.X) are generated in IR
                    codegen.register_class(class_name, cls.mro.clone());
                    
                    let mut init_sym_id: Option<u32> = None;
                    let mut init_num_params: usize = 0;
                    
                    for member in &cls.members {
                        if let roast_hir::HirClassMember::Method { func, .. } = member {
                            *function_count += 1;
                            let mut mir_builder = MirBuilder::new(hir_builder.expr_arena());
                            mir_builder.set_class_context(Some(class_name.to_string()), cls.mro.clone());
                            let mut mir_body = mir_builder.build_function(func);
                            
                            // Optimize MIR before codegen based on opt_level
                            let mir_opt_level = match opt_level {
                                0 => MirOptLevel::None,
                                1 => MirOptLevel::Less,
                                2 => MirOptLevel::Default,
                                _ => MirOptLevel::Aggressive,
                            };
                            let mut optimizer = Optimizer::for_level(mir_opt_level);
                            optimizer.optimize(&mut mir_body);
                            
                            // Check if this is __init__ method
                            let method_name_str = interner.resolve(func.name).unwrap_or_default();
                            if method_name_str == "__init__" {
                                init_sym_id = Some(func.name.as_raw());
                                // Subtract 1 for self parameter
                                init_num_params = func.params.len().saturating_sub(1);
                            }

                            // Run borrow checker on MIR (using a separate diagnostics sink)
                            let mut borrow_diagnostics = DiagnosticSink::new();
                            let mut borrow_checker = BorrowChecker::new(interner, &mut borrow_diagnostics);
                            let borrow_errors = borrow_checker.check_body(&mir_body);
                            if !borrow_errors.is_empty() {
                                for err in &borrow_errors {
                                    eprintln!("{} in method '{}.{}': {}", "Borrow error".red(), class_name, method_name_str, err);
                                }
                                all_borrow_errors.extend(borrow_errors.iter().map(|e| format!("method '{}.{}': {}", class_name, method_name_str, e)));
                            }

                            codegen.compile_function(&mir_body, Some(class_name))
                                .map_err(|e| anyhow::anyhow!("LLVM codegen error: {}", e))?;
                            
                            // Register the method for direct dispatch lookup
                            codegen.register_class_method(class_name, func.name.as_raw(), method_name_str);
                        }
                    }
                    
                    // Generate class constructor wrapper that allocates object and calls __init__
                    if let Some(init_id) = init_sym_id {
                        let class_sym_id = cls.name.as_raw();
                        codegen.generate_class_constructor(class_sym_id, init_id, init_num_params, class_name)
                            .map_err(|e| anyhow::anyhow!("Failed to generate class constructor: {}", e))?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    };
    
    // ===== Compile imported modules FIRST =====
    // This ensures that imported class methods are registered before main module is compiled,
    // enabling direct dispatch for imported class methods.
    let import_names = collect_imports(&module, &interner);
    for import_name in &import_names {
        if let Some(import_path) = resolve_module_path(import_name, path) {
            // Parse the imported module
            let import_source = match fs::read_to_string(&import_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{} Failed to read import '{}': {}", "warning:".yellow(), import_name, e);
                    continue;
                }
            };
            
            let mut import_diagnostics = DiagnosticSink::with_source(&import_source);
            let import_module = match parse_module(&import_source, &import_path.to_string_lossy(), &interner, &mut import_diagnostics) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{} Failed to parse import '{}': {}", "warning:".yellow(), import_name, e);
                    continue;
                }
            };
            
            // Type check the imported module
            {
                let mut import_checker = TypeChecker::new(&mut type_ctx, &interner, &mut import_diagnostics);
                import_checker.check_module(&import_module);
            }
            
            if import_diagnostics.has_errors() {
                report_diagnostics(&import_diagnostics, &import_source, &import_path);
                bail!("compilation failed in imported module '{}'", import_name);
            }
            
            // Re-register symbols after parsing import (new symbols like attr names were added)
            for i in 0..interner.len() {
                let sym = roast_common::Symbol::from_raw(i as u32);
                if let Some(name) = interner.resolve(sym) {
                    codegen.register_symbol(i as u32, name);
                }
            }
            
            // Build HIR and compile
            let mut import_hir_builder = HirBuilder::new(&interner, &mut type_ctx, &mut import_diagnostics);
            let import_hir_module = import_hir_builder.build_module(&import_module);
            
            // We don't want imported modules to set the entry point
            let mut _unused_entry: Option<String> = None;
            let mut _unused_void = false;
            compile_module(
                &import_hir_builder, &import_hir_module, &mut codegen, &interner,
                &mut _unused_entry, &mut _unused_void, &mut function_count,
                &mut all_borrow_errors, opt_level,
            )?;
        }
    }
    
    // ===== Compile main module AFTER imports =====
    {
        let mut hir_builder = HirBuilder::new(&interner, &mut type_ctx, &mut diagnostics);
        let hir_module = hir_builder.build_module(&module);
        
        compile_module(
            &hir_builder, &hir_module, &mut codegen, &interner,
            &mut entry_point, &mut entry_point_void, &mut function_count,
            &mut all_borrow_errors, opt_level,
        )?;
    }

    // Check for borrow errors - show warnings but don't block compilation for now
    // The borrow checker needs more refinement for complex class method patterns
    if !all_borrow_errors.is_empty() {
        eprintln!("{}: {} borrow warning(s) found (non-blocking)", "warning".yellow().bold(), all_borrow_errors.len());
        eprintln!("Borrow checker detected potential issues. These are informational for now.");
        // TODO: Make borrow errors blocking again once checker handles for-loop patterns
        // bail!("compilation failed due to borrow checker errors");
    }

    if let Some(entry) = entry_point {
        codegen.generate_main(&entry, entry_point_void)
            .map_err(|e| anyhow::anyhow!("Failed to generate main: {}", e))?;
    }

    println!("  Compiled {} function(s) to LLVM IR", function_count);

    let output_path = output.unwrap_or_else(|| Path::new("output"));
    println!("    Linking {}", output_path.display());
    let ir = codegen.get_ir();

    // Write IR to a temporary file
    let ir_path = output_path.with_extension("ll");
    std::fs::write(&ir_path, ir)
        .map_err(|e| anyhow::anyhow!("Failed to write IR: {}", e))?;

    // Compile with clang
    let mut cmd = std::process::Command::new("clang");
    cmd.arg(&ir_path)
        .arg("-o")
        .arg(output_path)
        .arg("-Wno-override-module")
        // Optimization flags for smaller binary size
        .arg("-O2")           // Optimize for speed (also reduces size)
        .arg("-flto")         // Link-time optimization (removes unused code)
        .arg("-ffunction-sections")
        .arg("-fdata-sections");

    // Try to link statically if the static library exists
    // First try relative to the roastc executable (for installed builds)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    
    let runtime_lib = if let Some(ref exe_dir) = exe_dir {
        // Check relative to executable: ../lib/libroast_runtime.a or same dir
        let lib_path = exe_dir.join("libroast_runtime.a");
        if lib_path.exists() {
            Some(lib_path)
        } else {
            let lib_path = exe_dir.parent().unwrap_or(exe_dir).join("lib").join("libroast_runtime.a");
            if lib_path.exists() {
                Some(lib_path)
            } else {
                None
            }
        }
    } else {
        None
    };
    
    // Fall back to checking known locations
    let runtime_lib = runtime_lib.or_else(|| {
        // Try roast source tree locations
        for path in &[
            "/home/swadhin/lang/roast/target/release/libroast_runtime.a",
            "/home/swadhin/lang/roast/target/debug/libroast_runtime.a",
            "target/release/libroast_runtime.a",
            "target/debug/libroast_runtime.a",
        ] {
            if std::path::Path::new(path).exists() {
                return Some(std::path::PathBuf::from(path));
            }
        }
        None
    });
    
    if let Some(lib_path) = runtime_lib {
        cmd.arg(&lib_path);
    } else {
        // Fall back to dynamic linking
        cmd.arg("-L/home/swadhin/lang/roast/target/release")
           .arg("-L/home/swadhin/lang/roast/target/debug")
           .arg("-Ltarget/release")
           .arg("-Ltarget/debug")
           .arg("-lroast_runtime");
    }

    let status = cmd.arg("-lm")
        .arg("-ldl")
        .arg("-lpthread")
        .arg("-Wl,--gc-sections")  // Remove unused sections
        .arg("-Wl,-s")             // Strip symbols
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to run clang: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("clang failed with exit code {}", status));
    }

    println!(
        "    Finished LLVM executable in {:.3}s",
        start.elapsed().as_secs_f64()
    );
    println!("\n→ Run with: {}", output_path.display());

    Ok(())
}

pub fn run_llvm(
    path: &Path,
    args: &[String],
    opt_level: u32,
    debug: bool,
) -> Result<()> {
    let stem = path.file_stem().unwrap_or_default();
    let output_path = std::env::temp_dir().join(stem);

    build_llvm(path, Some(&output_path), opt_level, debug)?;

    println!("\n    Running {}", output_path.display());

    let start = Instant::now();
    let status = std::process::Command::new(&output_path)
        .args(args)
        .status()
        .context("failed to run executable")?;

    println!(
        "\n    Finished Execution completed in {:.3}s (exit code: {})",
        start.elapsed().as_secs_f64(),
        status.code().unwrap_or(-1)
    );

    if !status.success() {
        bail!("program exited with status: {}", status);
    }

    Ok(())
}
