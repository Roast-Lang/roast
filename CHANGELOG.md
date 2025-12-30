# Changelog

All notable changes to the Roast programming language will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0-rc1] - 2024-12-29

### 🎉 First Release Candidate

This is the first release candidate for Roast v1.0, marking the language as production-ready.

### Added

#### Core Language
- **Static Type System**: Full type inference with gradual typing support
- **Ownership System**: Rust-inspired ownership and borrowing for memory safety
- **Pattern Matching**: Comprehensive pattern matching with exhaustiveness checking
- **Async/Await**: First-class async support with efficient runtime
- **Generics**: Generic functions and classes with type bounds
- **Dataclasses**: `@dataclass` decorator for automatic method generation
- **F-strings**: Formatted string literals with expression interpolation

#### Compilation
- **Native Code Generation**: Compiles to efficient native machine code
- **LLVM Backend**: Optional LLVM 17+ backend for optimized codegen
- **Cranelift Backend**: Fast compilation with Cranelift (default)
- **Cross-Platform**: Builds on Linux, Windows, and macOS
- **Optimization Passes**: Constant folding, DCE, inlining, and more

#### Standard Library
- **File System** (`fs`): read, write, mkdir, walk, copy, remove
- **Path** (`path`): join, basename, dirname, normalize, exists
- **Networking** (`net`): TCP/UDP sockets, connect, bind, listen
- **HTTP** (`http`): HTTP client with get, post, put, delete
- **JSON** (`json`): parse and stringify
- **Concurrency** (`sync`, `thread`, `channel`): Mutex, RwLock, spawn, MPSC
- **Collections** (`heap`, `queue`, `graph`): data structures and algorithms
- **Time** (`time`, `duration`): DateTime, Duration, sleep
- **Testing** (`testing`): test framework with assertions

#### Tooling
- **roastc CLI**: Compiler with build, run, check, fmt, test commands
- **REPL**: Interactive read-eval-print loop with history
- **Kitchen**: Project manager (like Cargo) with `new`, `build`, `run`, `test`
- **LSP Server**: Full IDE support with autocomplete, go-to-definition, hover
- **VS Code Extension**: Syntax highlighting and LSP integration
- **Package Manager**: `kitchen add`, `kitchen publish` for dependencies

#### GPU Computing
- **CUDA Support**: Native GPU kernel execution
- **Tensor Operations**: Matrix multiplication, reductions, activations
- **cuBLAS/cuDNN**: Integration with NVIDIA libraries
- **JIT Compilation**: Runtime kernel compilation with NVRTC

### Security
- **Bounds Checking**: Runtime array/slice bounds verification
- **Null Safety**: Optional types prevent null pointer errors
- **Package Signing**: Ed25519 signatures for published packages
- **Checksum Verification**: SHA256 integrity checks on downloads

### Performance
- **30-60x Python**: Native compilation delivers significant speedups
- **Zero-Cost Abstractions**: Collections and iterators optimized away
- **Incremental Compilation**: Fast rebuilds for large projects
- **Profile-Guided Optimization**: Support for PGO builds

### Known Limitations
- Borrow checker in advisory mode (enforcement in progress)
- Send/Sync traits not fully enforced for concurrency
- macOS GPU support limited to CPU fallback

### Migration from 0.1.0
- No breaking changes from 0.1.0 alpha
- Recommended to rebuild all projects with new compiler

---

## [0.1.0] - 2024-01-01

### Added
- Initial alpha release
- Core language features
- Basic standard library
- Experimental tooling

[1.0.0-rc1]: https://github.com/roast-lang/roast/releases/tag/v1.0.0-rc1
[0.1.0]: https://github.com/roast-lang/roast/releases/tag/v0.1.0
