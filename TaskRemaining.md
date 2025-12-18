# 🔥 Roast Language - Pre-Release Polish Tasks

> **Goal:** Prepare Roast for v1.0 public release with production-quality polish.
> **Current Status:** ~98% Core Complete | Polish Phase Required

---

## 📋 High Priority (P0) - Must Have for Release

### 1. Clean Output Formatting
- [ ] **Reduce verbosity in normal mode** - Hide LLVM details by default
- [ ] **Add `--quiet` / `-q` flag** - Minimal output (only program output)
- [ ] **Add `--verbose` / `-v` flag** - Show compilation details
- [ ] **Colored progress indicators** - ✓ symbols instead of text
- [ ] **Better timing format** - `0.26s` instead of `in 0.263s`

**Current:**
```
   Compiling tests/test_llvm_fib.roast (LLVM)
  Compiled 2 function(s) to LLVM IR
    Linking /tmp/test_llvm_fib
    Finished LLVM executable in 0.269s

→ Run with: /tmp/test_llvm_fib

    Running /tmp/test_llvm_fib
102334155

    Finished Execution completed in 0.182s (exit code: 0)
```

**Target (Default):**
```
102334155
```

**Target (--verbose):**
```
   Compiling test_llvm_fib.roast
      Built  2 functions → 0.27s
    Running  output:
102334155
   Finished  0.18s ✓
```

---

### 2. Error Message Enhancement
- [ ] **Rust-style caret underlines** - Point to exact error location
- [ ] **Colored error output** - Red for errors, yellow for warnings
- [ ] **Fix suggestions** - "Did you mean X?" for typos
- [ ] **Error code links** - `E0001: see 'roastc --explain E0001'`
- [ ] **Multi-line error spans** - Show context around errors
- [ ] **Help notes** - Common solutions for each error type

**Target Error Format:**
```
error[E0001]: unexpected token
 --> examples/bad.roast:5:12
  |
5 |     return x +
  |            ^^^^ expected expression after operator
  |
help: add the second operand
  |
5 |     return x + y
  |                +
```

---

### 3. Package Registry Server
- [ ] **Create registry backend** - Simple REST API (Rust/Axum)
- [ ] **Package upload endpoint** - POST /api/v1/packages
- [ ] **Package download endpoint** - GET /api/v1/packages/{name}/{version}
- [ ] **Search functionality** - GET /api/v1/search?q={query}
- [ ] **Authentication** - API key / OAuth
- [ ] **Package validation** - Verify roast.toml, checksums
- [ ] **Web UI** - Package browser (optional for v1.0)

---

### 4. Documentation
- [ ] **Language Reference** - Complete syntax documentation
- [ ] **Standard Library Docs** - Auto-generated from docstrings
- [ ] **Getting Started Guide** - Installation, first program
- [ ] **Migration Guide** - Python → Roast conversion tips
- [ ] **API Documentation** - roastc CLI flags and options
- [ ] **Examples Gallery** - Curated example programs

---

## 📋 Medium Priority (P1) - Important for Quality

### 5. CLI Polish
- [ ] **Consistent command structure** - `roastc <command> [options]`
- [ ] **Shell completions** - Bash, Zsh, Fish, PowerShell
- [ ] **Config file support** - `.roastrc` or `roast.toml`
- [ ] **Progress bars** - For long compilations
- [ ] **Interactive prompts** - Confirmation for destructive ops

### 6. Build Improvements
- [ ] **Incremental compilation** - Wire up `incremental.rs` to CLI
- [ ] **Parallel file compilation** - Already have rayon, ensure working
- [ ] **Build caching** - Hash-based cache invalidation
- [ ] **Watch mode** - `roastc watch` for auto-rebuild
- [ ] **Release builds** - `-O2` optimization flag

### 7. Testing Infrastructure
- [ ] **Test runner improvement** - Better failure reporting
- [ ] **Code coverage** - Integration with coverage tools
- [ ] **Snapshot testing** - For compiler output
- [ ] **Benchmark CI** - Track performance regressions
- [ ] **Integration test harness** - Run all .roast tests

### 8. LSP Improvements
- [ ] **Go to definition** - Jump to symbol source
- [ ] **Find references** - All usages of symbol
- [ ] **Rename symbol** - Project-wide rename
- [ ] **Code actions** - Quick fixes
- [ ] **Inlay hints** - Type annotations

---

## 📋 Lower Priority (P2) - Nice to Have

### 9. Installer & Distribution
- [ ] **Install script** - curl | sh installer
- [ ] **Homebrew formula** - macOS installation
- [ ] **APT/DNF packages** - Linux installation
- [ ] **Windows MSI** - Windows installation
- [ ] **Docker image** - Containerized roastc
- [ ] **GitHub releases** - Pre-built binaries

### 10. REPL Enhancement
- [ ] **Multi-line editing** - Better indentation handling
- [ ] **History search** - Ctrl+R reverse search
- [ ] **Inline help** - `?function` shows help
- [ ] **Magic commands** - %time, %debug, %load
- [ ] **Persistent variables** - Keep state between sessions

### 11. Debugger Polish
- [ ] **VS Code extension** - DAP integration package
- [ ] **Conditional breakpoints** - Break on condition
- [ ] **Watch expressions** - Live variable monitoring
- [ ] **Call stack navigation** - Step into/out/over
- [ ] **Memory inspector** - View memory layout

---

## 📋 Deferred (Post v1.0)

- [ ] GPU backend finalization
- [ ] Full Python interop (runtime loading)
- [ ] JIT compilation mode
- [ ] Self-hosting (roast compiler in roast)
- [ ] WebAssembly target
- [ ] Native ARM builds (Apple Silicon optimization)

---

## 🎯 Release Checklist

Before v1.0 release:

1. [ ] All P0 tasks complete
2. [ ] 80%+ of P1 tasks complete
3. [ ] Zero critical bugs
4. [ ] Documentation website live
5. [ ] Package registry operational
6. [ ] Install script tested on Linux/macOS
7. [ ] GitHub releases with binaries
8. [ ] Announcement blog post
9. [ ] Example projects ready
10. [ ] Community channels (Discord/GitHub Discussions)

---

**Estimated Effort:**
- P0 Tasks: ~2-3 weeks
- P1 Tasks: ~3-4 weeks
- P2 Tasks: ~4-6 weeks (ongoing)

**Suggested First Sprint:**
1. Clean output formatting (1-2 days)
2. Error message enhancement (2-3 days)
3. Getting Started documentation (1-2 days)
4. Registry MVP (3-5 days)
