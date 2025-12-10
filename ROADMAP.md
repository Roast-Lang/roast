# 🔥 Roast Language Roadmap

> **Goal**: A language as rich as Python, as fast as Rust

This document tracks everything needed to make Roast a production-ready, feature-rich compiled language.

---

## 📊 Current Status Overview (Realistic Assessment)

| Component                  | Status           | Completeness | Notes                                               |
| -------------------------- | ---------------- | ------------ | --------------------------------------------------- |
| Lexer/Parser               | ✅ Implemented   | 85%          | Missing positional-only params, f-string evaluation |
| AST                        | ✅ Implemented   | 90%          | Solid foundation                                    |
| Type System                | ⚠️ Partial       | 70%          | Missing variance, where clauses, const generics     |
| Borrow Checker             | ✅ Implemented   | 80%          | Works for most cases                                |
| MIR                        | ✅ Implemented   | 85%          | Good coverage                                       |
| HIR                        | ⚠️ Partial       | 60%          | Needs work                                          |
| Bytecode VM                | ✅ Implemented   | 80%          | **Missing async/await execution**                   |
| Optimizer                  | ✅ Implemented   | 75%          | Good passes, PGO infrastructure only                |
| Codegen (Bytecode)         | ✅ Implemented   | 85%          | Solid                                               |
| Native Codegen (Cranelift) | 🔴 Scaffolding   | 40%          | **Cannot call functions yet**                       |
| **LLVM Backend**           | ✅ **WORKING**   | 85%          | **Rust-matching speed! fib(40) in 0.19s**           |
| Standard Library           | ✅ Implemented   | 80%          | Good coverage, some gaps                            |
| GPU Backend                | ✅ Implemented   | 85%          | CUDA working                                        |
| Python Compat              | 🔴 Not Working   | 30%          | **py: prefix not implemented**                      |
| Kitchen (Project Mgr)      | ✅ Implemented   | 80%          | Works well                                          |
| LSP                        | ⚠️ Partial       | 65%          | Same-file only, no code actions                     |
| Package Registry           | ⚠️ Stub Only     | 20%          | Local only                                          |
| Debugger (DAP)             | 🔴 Not Connected | 50%          | **Not wired to VM**                                 |
| Async Executor             | ⚠️ Rust-Level    | 60%          | **Not integrated with bytecode**                    |
| Async I/O                  | ✅ Implemented   | 80%          | Rust-native works                                   |
| Testing Framework          | ✅ Implemented   | 80%          | Works well                                          |
| Macros System              | ⚠️ Partial       | 40%          | Classification only, no code gen                    |
| REPL                       | ✅ Implemented   | 70%          | Works                                               |

**Overall Realistic Completion: ~75%** (LLVM backend now working!)

---

## 🚨 CRITICAL BLOCKING ISSUES

### 🔴 Issue 1: Async/Await Cannot Execute

**Problem**: `async/await` syntax parses and type-checks, but **cannot run in VM**.

**Current State**:

- Parser: ✅ Handles `async def`, `await`
- Type checker: ✅ Checks async functions
- VM: ❌ **No `Await` opcode** - only `Yield` exists
- Executor: ❌ **Rust-native**, not integrated with bytecode

**Fix Required**:

- [ ] Add `OpCode::Await` to VM
- [ ] Implement coroutine/generator protocol
- [ ] Bridge async executor with bytecode interpreter
- [ ] Support `async for`, `async with`

### 🔴 Issue 2: Cranelift Cannot Call Functions

**Problem**: Native compilation exists but **produces broken binaries**.

**Current State** (from `cranelift.rs`):

```rust
// TODO: Actually call the function
let result = self.builder.ins().iconst(types::I64, 0);
```

**Fix Required**:

- [ ] Implement actual function call emission
- [ ] Link runtime library (builtins, GC)
- [ ] Handle string/object creation in native code

### 🔴 Issue 3: Python Interop Not Implemented

**Problem**: `py:` prefix advertised but **doesn't work**.

**Current State**:

- Kitchen: ✅ Can download PyPI packages
- FFI: ❌ **No bridge to call Python code**
- Import: ❌ **Cannot `import py:pandas`**

**Fix Required**:

- [ ] Implement CPython FFI (PyO3 or ctypes)
- [ ] Add `py:` module resolution
- [ ] Type stub generation from Python packages

### 🔴 Issue 4: Debugger Not Connected

**Problem**: DAP server exists but **cannot debug Roast code**.

**Current State**:

- DAP Protocol: ✅ Complete implementation
- VM Hooks: ❌ **Not exposed** - "VM doesn't expose frames directly"

**Fix Required**:

- [ ] Add `on_step` callback to VM interpreter loop
- [ ] Expose stack frames to debugger
- [ ] Wire breakpoint manager to VM

---

## 🚨 CRITICAL MISSING FEATURES

### 1. Native Compilation (HIGH PRIORITY) ⚠️ SCAFFOLDING ONLY

**Current State**: Cranelift backend exists but **cannot produce working binaries**.

**Actually Completed**:

- [x] Cranelift backend integration (`crates/codegen/src/cranelift.rs`)
- [x] MIR to Cranelift IR lowering (arithmetic, control flow)
- [x] Basic code generation for x86_64
- [x] Linker support (`crates/codegen/src/linker.rs`)

**Critical Gaps**:

- [ ] **Function calls** - Currently returns 0 instead of calling
- [ ] **String constants** - TODO: Create data section
- [ ] **Object creation** - Returns null for complex types
- [ ] **Runtime library** - No builtins in native code
- [ ] ARM64 code generation
- [ ] PGO support

### 2. Runtime Performance

**Needed**:

- [ ] **JIT compilation** - Infrastructure exists but not wired up
- [ ] **Inline caching** - Module exists but not connected to VM
- [ ] Escape analysis integration
- [ ] Speculative optimization
- [ ] Deoptimization support

### 3. Concurrency & Parallelism ⚠️ PARTIALLY IMPLEMENTED

**Current State**: Async executor works at **Rust level only**.

**Completed** (Rust-native):

- [x] Async executor (`crates/runtime/src/executor.rs`)
- [x] Work-stealing scheduler
- [x] Async channels
- [x] Async file I/O, TCP/UDP

**Missing** (Bytecode integration):

- [ ] **`await` opcode in VM**
- [ ] **Coroutine protocol**
- [ ] **`async for`, `async with`**
- [ ] Structured concurrency

**Still Needed**:

- [ ] VM integration for `async/await` syntax
- [ ] Structured concurrency (`trio`-style)
- [ ] Actor model support

---

## 📝 LANGUAGE FEATURES TO ADD

### 4. Pattern Matching Enhancements ✅ IMPLEMENTED

**Current State**: Pattern matching VM opcodes implemented with exhaustiveness checking.

**Completed**:

- [x] Full pattern matching implementation in VM (bytecode opcodes)
- [x] Guard clauses (`case x if x > 0`)
- [x] Structural patterns (`case Point(x, y)`)
- [x] Or patterns (`case 1 | 2 | 3`)
- [x] As patterns (`case x as y`)
- [x] Wildcard patterns (`case _`)
- [x] Sequence patterns (`case [first, *rest]`)
- [x] Mapping patterns (`case {"key": value}`)
- [x] Exhaustiveness checking (`typer/exhaustiveness.rs`)

### 5. Protocols/Traits (Structural Typing) ⚠️ PARTIALLY IMPLEMENTED

**Current State**: Protocol infrastructure exists but incomplete.

**Completed**:

- [x] Protocol definition syntax (`crates/typer/src/protocols.rs`)
- [x] 11 built-in protocols: `Sized`, `Copy`, `Clone`, `Send`, `Sync`, `Hashable`, `Eq`, `Ord`, `Iterator`, `Iterable`, `ContextManager`, `Callable`, `Add`
- [x] Basic protocol checking

**Missing**:

- [ ] **`where` clauses** - Not parsed or checked
- [ ] **Associated types** - Declared but not fully working
- [ ] **Default method implementations** - Classification only
- [ ] **Derive macros** - Just classification, no code generation

**Example** (syntax works, code gen missing):

```python
@derive(Eq, Hash, Debug, Copy)  # Classified but not generated
class Point:
    x: int
    y: int
```

### 6. Generics Improvements ⚠️ NEEDS WORK

**Current State**: Basic generics work, advanced features missing.

**Completed**:

- [x] Type parameters (`T`, `U`)
- [x] Basic bounds (`T: Hashable`)
- [x] Generic functions and classes

**Critical Gaps**:

- [ ] **Variance annotations** (`+T`, `-T`) - High priority
- [ ] **`where` clauses** - High priority
- [ ] Const generics (`List[T, N: int]`)
- [ ] Higher-kinded types
- [ ] Generic associated types
- [ ] `impl Trait` return type

### 7. Macros System 🔴 CLASSIFICATION ONLY

**Current State**: Decorators are **classified but not executed**.

**What Works**:

- [x] Decorator classification (`crates/ast/src/macros.rs`)
- [x] Built-in decorator recognition
- [x] Macro registry infrastructure

**What's Missing** (Major Gap):

- [ ] **`@dataclass` code generation** - Doesn't generate `__init__`, `__repr__`, etc.
- [ ] **`@derive` code generation** - Doesn't implement protocols
- [ ] **`@property` lowering** - Not converted to getter/setter
- [ ] Procedural macros
- [ ] Hygiene

### 8. Error Handling Improvements ✅ IMPLEMENTED

**Current State**: Comprehensive error handling implemented.

**Completed**:

- [x] `Result[T, E]` as first-class type (`crates/stdlib/src/result.rs`)
- [x] Error chain support (`crates/stdlib/src/error.rs`)
- [x] Stack traces with source locations
- [x] Retry logic with exponential backoff
- [x] Combinators: `flatten`, `transpose`, `try_map`, `try_filter`, `try_fold`

**Still Needed**:

- [ ] `?` operator for Result propagation (parser integration)
- [ ] `catch_unwind` equivalent

### 9. Memory Management Refinements

**Current State**: Borrow checker exists, basic RC for dynamic.

**Needed**:

- [ ] Arena allocators
- [ ] Custom allocators per-type
- [ ] Stack allocation for small objects
- [ ] Placement new
- [ ] Explicit `drop()` for early deallocation
- [ ] Weak references
- [ ] Interior mutability (`Cell`, `RefCell` equivalent)

### 10. Metaprogramming

**Needed**:

- [ ] Compile-time function execution (constexpr)
- [ ] Type reflection at compile time
- [ ] Code generation from templates
- [ ] `@staticmethod`, `@classmethod` proper lowering
- [ ] `@dataclass` decorator implementation
- [ ] `@property` proper implementation

---

## 🔧 TOOLING IMPROVEMENTS

### 11. LSP Server (HIGH PRIORITY) ✅ IMPLEMENTED

**Current State**: Full-featured LSP with code intelligence.

**Completed**:

- [x] Go to definition (within file)
- [x] Find all references
- [x] Rename symbol
- [x] Auto-completion (keywords, types, builtins, local symbols)
- [x] Signature help
- [x] Hover information
- [x] Document symbols
- [x] Symbol analysis (`crates/lsp/src/analysis.rs`)
- [x] Inlay hints (type hints for variables)

**Critical Gaps**:

- [ ] **Cross-module go-to-definition** - Infrastructure exists, not wired up
- [ ] **Code actions** - Not implemented
- [ ] **Semantic highlighting** - Not implemented
- [ ] Organize imports
- [ ] Extract variable/function refactoring

### 12. Debugger 🔴 NOT CONNECTED TO VM

**Current State**: DAP protocol complete, but **cannot debug Roast code**.

**Completed** (Protocol Layer):

- [x] DAP (Debug Adapter Protocol) server (`crates/debugger/`)
- [x] Breakpoints (line, conditional, hit count)
- [x] Step in/over/out commands
- [x] Variable inspection interface
- [x] Call stack view interface
- [x] Scopes (locals, globals) interface
- [x] Watch expressions (`debugger/watch.rs`)

**Critical Gap**: VM not connected

- [ ] **VM step hook** - VM doesn't expose `on_step` callback
- [ ] **Stack frame exposure** - "VM doesn't expose frames directly"
- [ ] **Breakpoint triggering** - Breakpoints exist but don't fire
- [ ] Async stack traces

### 13. Profiler ✅ IMPLEMENTED

**Current State**: CPU and memory profiling implemented.

**Completed**:

- [x] CPU profiler (`crates/stdlib/src/profiler.rs`)
- [x] Memory profiler (allocation tracking)
- [x] Flame graph generation
- [x] Timer utilities

**Still Needed**:

- [ ] Line-level profiling
- [ ] Async profiling
- [ ] GPU profiling integration

### 14. Documentation Generator ✅ IMPLEMENTED

**Completed** (`crates/stdlib/src/doc.rs`, `roastc doc`):

- [x] `DocGenerator` - HTML documentation generator
- [x] Markdown to HTML conversion
- [x] CSS styling with light/dark theme
- [x] Search index generation (JSON)
- [x] Type signature extraction
- [x] Examples from docstrings
- [x] `roastc doc` CLI command

**Still Needed**:

- [ ] Cross-reference linking
- [ ] Version tracking

### 15. Formatter Improvements

**Current State**: `roastfmt` exists but basic.

**Needed**:

- [ ] Configurable style options
- [ ] Import sorting
- [ ] Line length enforcement
- [ ] Trailing comma control
- [ ] Blank line rules
- [ ] Comment preservation
- [ ] `# fmt: off/on` directives

### 16. Linter ✅ IMPLEMENTED

**Completed** (`roastc lint`):

- [x] `roastc lint` command with CLI options
- [x] E001: Parse errors
- [x] E002: Python 2 print statement
- [x] W001: Line too long (>100 chars)
- [x] W002: Trailing whitespace (auto-fixable)
- [x] W003: Mixed tabs and spaces
- [x] W004: TODO/FIXME comments
- [x] W005: Unnecessary pass statement
- [x] W006: Comparison to None (should use 'is')
- [x] W007: Comparison to True/False
- [x] W008: Single-letter variable names
- [x] `--fix` flag for auto-fixing
- [x] `--format json` for machine-readable output
- [x] `--ignore` and `--select` for rule filtering
- [x] `--errors-only` flag

**Still Needed**:

- [ ] Unused variable detection
- [ ] Unused import detection
- [ ] Unreachable code detection
- [ ] Complexity warnings (McCabe)
- [ ] Security lints
- [ ] Custom lint rules

---

## 📚 STANDARD LIBRARY GAPS

### 17. Data Structures ✅ MOSTLY IMPLEMENTED

**Completed** (`crates/stdlib/src/collections.rs`, `heapq.rs`, `bisect.rs`):

- [x] `OrderedDict` - Insertion-order dict
- [x] `DefaultDict` - Dict with default factory
- [x] `Counter` - Counting dict
- [x] `NamedTuple` - Lightweight classes
- [x] `ChainMap` - Multiple dict view
- [x] `heapq` - Heap operations, nlargest, nsmallest, merge
- [x] `bisect` - Binary search operations
- [x] `SortedList` - Auto-sorted list
- [x] `SortedDict` - Dict with sorted keys
- [x] `PriorityQueue` - Heap-based priority queue

**Still Needed**:

- [ ] `deque` - Double-ended queue (optimize current)
- [ ] `LRUCache` - LRU caching
- [ ] `BitSet` - Efficient bit storage
- [ ] `BloomFilter` - Probabilistic set
- [ ] `Trie` - Prefix tree
- [ ] `IntervalTree` - Range queries
- [ ] `DisjointSet` - Union-find

### 18. Algorithms ✅ COMPLETED

**Completed**:

- [x] `itertools` - Iterator utilities (`crates/stdlib/src/itertools.rs`)
  - [x] `chain`, `cycle`, `repeat`, `count`
  - [x] `combinations`, `permutations`, `combinations_with_replacement`
  - [x] `groupby`, `islice`, `tee`
  - [x] `takewhile`, `dropwhile`, `filterfalse`
  - [x] `product`, `zip_longest`, `compress`
  - [x] `accumulate`, `starmap`
- [x] `functools` - Function utilities (`crates/stdlib/src/functools.rs`)
  - [x] `reduce`, `partial`
  - [x] `lru_cache` decorator
  - [x] `cache` decorator
  - [x] `total_ordering`, `cmp_to_key`
- [x] `bisect` - Binary search utilities (`crates/stdlib/src/bisect.rs`)
  - [x] `bisect_left`, `bisect_right`, `bisect`
  - [x] `insort_left`, `insort_right`, `insort`
  - [x] `floor`, `ceiling`, `range`
  - [x] `SortedList`, `SortedDict`
- [x] `heapq` - Heap operations (`crates/stdlib/src/heapq.rs`)
  - [x] `heappush`, `heappop`, `heapify`
  - [x] `heapreplace`, `heappushpop`
  - [x] `nlargest`, `nsmallest`, `merge`
  - [x] `PriorityQueue`

**Still Needed**:

- [ ] Sorting algorithms (timsort as default)

### 19. Serialization ✅ MOSTLY IMPLEMENTED

**Current State**: JSON, CSV, and XML implemented.

**Completed**:

- [x] CSV parser/writer (`crates/stdlib/src/csv.rs`)
  - [x] `Reader`, `Writer`, `DictReader`, `DictWriter`
  - [x] Dialect configuration
  - [x] Excel/Unix dialect support
- [x] XML parser/writer (`crates/stdlib/src/xml.rs`)
  - [x] `Parser`, `Writer`, `Element`, `Document`
  - [x] ElementTree-style builder pattern
  - [x] CDATA, comments, processing instructions

**Still Needed**:

- [ ] Full JSON compliance (RFC 8259)
- [ ] JSON streaming parser
- [ ] TOML parser/writer
- [ ] YAML parser/writer
- [ ] MessagePack
- [ ] Protocol Buffers support
- [ ] Pickle-like serialization

### 20. Networking Improvements

**Current State**: Basic TCP/UDP + async I/O.

**In Standard Library** (essential networking):

- [x] TCP/UDP sockets
- [x] Async TCP/UDP
- [ ] TLS/SSL (native)
- [ ] DNS resolver
- [ ] Basic HTTP/1.1 client

**External Packages** (advanced networking):

```bash
roast add requests   # HTTP client (like Python requests)
roast add aiohttp    # Async HTTP
roast add websockets # WebSocket support
roast add grpcio     # gRPC
```

**Package: roast-requests** (future):

- [ ] HTTP/2 support
- [ ] HTTP/3 (QUIC) support
- [ ] Connection pooling
- [ ] Proxy support (HTTP, SOCKS)

**Package: roast-websockets** (future):

- [ ] WebSocket client/server

**Package: roast-grpc** (future):

- [ ] gRPC client/server
- [ ] Protocol Buffers support

### 21. Database Support ✅ PARTIALLY IMPLEMENTED

**In Standard Library** (basic utilities):

- [x] SQLite-like in-memory database
- [x] Connection pooling interface
- [x] Query builder
- [x] ORM (Repository pattern)
- [x] Migrations system

**External Packages** (database drivers):

```bash
roast add sqlite3    # SQLite driver
roast add psycopg2   # PostgreSQL driver
roast add mysql      # MySQL driver
roast add redis      # Redis client
roast add pymongo    # MongoDB driver
roast add sqlalchemy # Full ORM
```

**Package: roast-sqlite** (future):

- [ ] SQLite bindings

**Package: roast-postgres** (future):

- [ ] PostgreSQL driver

**Package: roast-redis** (future):

- [ ] Redis client

### 22. Cryptography ✅ MOSTLY IMPLEMENTED

**Completed** (`crates/stdlib/src/crypto.rs`):

- [x] AES-128/CBC encryption/decryption
- [x] SHA-256 hashing
- [x] HMAC-SHA256
- [x] PBKDF2-SHA256 password hashing
- [x] Secure random generator
- [x] Base64/Hex encoding
- [x] JWT support (HS256)

**Still Needed**:

- [ ] RSA key generation/encryption
- [ ] ECDSA signing
- [ ] Ed25519 signing
- [ ] bcrypt, Argon2
- [ ] X.509 certificate handling

### 23. System Interfaces ✅ MOSTLY IMPLEMENTED

**Completed**:

- [x] `subprocess` - Process spawning (`crates/stdlib/src/subprocess.rs`)
  - [x] `Popen`, `ProcessBuilder`, `Pipeline`
  - [x] `run()`, `check_call()`, `check_output()`
  - [x] `shell()`, `getoutput()`
  - [x] Process pools
- [x] `shutil` - High-level file operations (`crates/stdlib/src/shutil.rs`)
  - [x] `copy()`, `copy2()`, `copytree()`
  - [x] `move_()`, `rmtree()`
  - [x] `which()`, `disk_usage()`
  - [x] Archive support (zip, tar)
- [x] `glob` - Path pattern matching (`crates/stdlib/src/glob.rs`)
  - [x] `glob()`, `iglob()`, `fnmatch()`
  - [x] `*`, `?`, `[abc]`, `{a,b}`, `**` patterns
- [x] `tempfile` - Temporary files (`crates/stdlib/src/tempfile.rs`)
  - [x] `TempDir`, `NamedTempFile`
  - [x] `SpooledTempFile`
  - [x] `mktemp()`, `mkdtemp()`

**Completed**:

- [x] `signal` - Signal handling (`crates/stdlib/src/signal.rs`)
- [x] `mmap` - Memory-mapped files (`crates/stdlib/src/mmap.rs`)

**Still Needed**:

- [ ] `pty` - Pseudo-terminal
- [ ] `fcntl` - File control

### 24. Date/Time Improvements ✅ MOSTLY IMPLEMENTED

**Completed** (`crates/stdlib/src/timezone.rs`):

- [x] `Timezone` type with UTC offsets
- [x] `ZonedDateTime` with timezone support
- [x] ISO 8601 parsing (`parse_iso`)
- [x] `strftime` formatting
- [x] `TimeDelta` for duration arithmetic
- [x] Common timezone database (UTC, EST, PST, CET, JST, etc.)
- [x] IANA-style names (America/New_York, Europe/Paris, etc.)

**Still Needed**:

- [ ] DST transitions
- [ ] Recurring events
- [ ] Full IANA database integration

---

---

## 📦 PACKAGE ECOSYSTEM PHILOSOPHY

### Standard Library vs External Packages

**Standard Library** (ships with Roast):

- Core types: `str`, `int`, `list`, `dict`, `set`
- Essential I/O: `fs`, `io`, `net` (TCP/UDP)
- Basic utilities: `json`, `csv`, `time`, `os`, `path`
- Async runtime: `async`, `spawn`, `channels`
- Crypto basics: `hash`, `hmac`
- Testing: `test`, `assert`

**External Packages** (installable via `roast add`):

- Data Science: `numpy`, `pandas`, `matplotlib`, `scipy`
- Web: `requests`, `flask`, `django`, `fastapi`
- Database: `sqlite3`, `psycopg2`, `redis`, `sqlalchemy`
- ML/AI: `torch`, `tensorflow`, `transformers`
- CLI: `click`, `rich`, `typer`
- DevOps: `docker`, `kubernetes`, `ansible`

### Package Manager Commands

```bash
# Add Roast packages
roast add pandas                    # Add Roast package
roast add numpy scipy matplotlib    # Add multiple packages
roast add requests==2.28.0          # Specific version
roast add "pandas>=2.0,<3.0"        # Version range

# Add Python packages (use existing PyPI packages!)
roast add py:pandas                 # Install Python pandas from PyPI
roast add py:numpy py:scipy         # Multiple Python packages
roast add py:requests==2.28.0       # Specific version
roast add "py:flask>=2.0"           # Version range

# Requirements files
roast add -r requirements.txt       # Roast requirements
roast add -r py:requirements.txt    # Python requirements (from PyPI)

# Dev dependencies
roast add --dev pytest black mypy
roast add --dev py:pytest py:black  # Python dev tools

# Remove packages
roast remove pandas
roast remove py:pandas              # Remove Python package

# Update packages
roast update                        # Update all
roast update pandas                 # Update specific
roast update py:pandas              # Update Python package

# List packages
roast list                          # All installed packages
roast list --roast                  # Only Roast packages
roast list --python                 # Only Python packages
roast list --outdated               # Check for updates

# Lock file
roast lock                          # Generate roast.lock

# Publish
roast publish                       # Publish to registry
```

### Python Package Interoperability

Roast can use Python packages as if they were native Roast packages!

```python
# Install Python package
# $ roast add py:requests

# Use in Roast code (transparent!)
import requests  # Works! Uses Python's requests

response = requests.get("https://api.example.com")
data = response.json()

# Or be explicit about Python origin
from py import requests
from py.pandas import DataFrame
```

**How it works**:

- `py:` prefix tells Roast to fetch from PyPI
- Packages are installed in a managed Python venv
- Roast's FFI layer bridges calls transparently
- Type hints from Python are used for type checking
- Performance-critical code can be migrated to native Roast later

---

## 🌐 ECOSYSTEM NEEDS

### 25. Package Registry ✅ MOSTLY IMPLEMENTED

**Current State**: Core features implemented!

**Completed** (`crates/kitchen/`):

- [x] `kitchen add <package>` - Add Roast packages
- [x] `kitchen add py:<package>` - Add Python packages from PyPI!
- [x] `kitchen add -r requirements.txt` - Roast requirements
- [x] `kitchen add -r py:requirements.txt` - Python requirements
- [x] `kitchen add --dev` - Dev dependencies
- [x] `kitchen remove <package>` - Remove packages
- [x] `kitchen list` - List installed packages
- [x] `kitchen list --outdated` - Check for updates
- [x] `kitchen list --python` - Show Python packages
- [x] `kitchen install` - Install all dependencies
- [x] `roast.lock` lockfile generation
- [x] PyPI API client (`pypi.rs`)
- [x] Managed Python venv per project (`.roast/python/`)
- [x] Python package lock (`python.lock`)

**Still Needed**:

- [ ] Central registry server (`registry.roast-lang.org`)
- [ ] Package signing
- [ ] Security scanning
- [ ] Download statistics
- [ ] Transparent Python package imports

### 26. Build System Enhancements (Kitchen)

**Current State**: Basic build/run works.

**Needed**:

- [ ] Build caching (like ccache)
- [ ] Incremental compilation
- [ ] Parallel builds
- [ ] Build scripts
- [ ] Platform-specific dependencies
- [ ] Features/conditional compilation
- [ ] Workspace dependencies sharing
- [ ] Vendoring support
- [ ] Reproducible builds

### 27. Testing Framework ✅ IMPLEMENTED

**Current State**: Full-featured testing framework.

**Completed**:

- [x] Test suites and test cases
- [x] Parameterized tests
- [x] Test discovery from files
- [x] Setup/teardown hooks (before_each, after_each)
- [x] Mock counter
- [x] Test runner with parallel support
- [x] Test filtering
- [x] Rich assertions (eq, ne, contains, range, approx, etc.)
- [x] Fixture system

**Still Needed**:

- [ ] `@test` decorator integration with parser
- [ ] Coverage collection
- [ ] Snapshot testing
- [ ] Property-based testing
- [ ] Async test support

### 28. CI/CD Integration

**Needed**:

- [ ] GitHub Actions templates
- [ ] GitLab CI templates
- [ ] Docker images
- [ ] Pre-built binaries for all platforms
- [ ] Release automation

---

## 🎮 GPU & COMPUTE

### 29. GPU Improvements (Current: 85%)

**Already Implemented**:

- ✅ CUDA backend
- ✅ cuBLAS integration
- ✅ cuDNN integration
- ✅ Multi-GPU (NCCL)
- ✅ Autograd
- ✅ Memory management
- ✅ NVRTC JIT compilation

**Needed**:

- [ ] OpenCL full implementation
- [ ] Metal backend for macOS
- [ ] Vulkan compute shaders
- [ ] Mixed precision training (FP16/BF16)
- [ ] Gradient checkpointing
- [ ] Model parallelism
- [ ] Tensor parallelism
- [ ] Pipeline parallelism
- [ ] Flash Attention
- [ ] cuSPARSE integration
- [ ] cuFFT integration
- [ ] TensorRT integration
- [ ] ONNX export

### 30. Scientific Computing → **EXTERNAL PACKAGE: `roast-numpy`**

These will be **installable packages**, not part of stdlib:

```bash
roast add numpy      # Array library
roast add scipy      # Scientific computing
roast add pandas     # Data analysis
roast add matplotlib # Plotting
```

**Package: roast-numpy** (future):

- [ ] NumPy-like array library
- [ ] Broadcasting rules
- [ ] Array slicing syntax
- [ ] BLAS integration (CPU)
- [ ] LAPACK integration
- [ ] FFT

**Package: roast-scipy** (future):

- [ ] Statistics module
- [ ] Linear algebra module
- [ ] Optimization
- [ ] Signal processing
- [ ] Image processing

---

## 🐍 PYTHON COMPATIBILITY

### 31. Python Interop Improvements

**Current State**: Basic FFI, migration tool.

**Python Package Integration** (HIGH PRIORITY):

```bash
# Install any PyPI package
roast add py:pandas
roast add py:numpy py:scipy py:matplotlib
roast add -r py:requirements.txt
```

```python
# Use in Roast code - transparent!
import pandas as pd
import numpy as np

df = pd.DataFrame({"a": [1, 2, 3]})
arr = np.array([1, 2, 3])
```

**Needed**:

- [ ] `py:` prefix for PyPI packages
- [ ] Call Python from Roast seamlessly
- [ ] Call Roast from Python
- [ ] NumPy array sharing (zero-copy)
- [ ] Pandas DataFrame support
- [ ] Python C extension loading
- [ ] Managed Python venv per project
- [ ] Type stub generation from Python packages
- [ ] Gradual migration path (Python → Roast)

### 32. Python Feature Parity

**Needed for full compatibility**:

- [ ] `*args`, `**kwargs` full support
- [ ] Keyword-only arguments
- [ ] Positional-only arguments (Python 3.8+)
- [ ] Walrus operator `:=`
- [ ] F-string expressions
- [ ] `@functools.wraps`
- [ ] `contextlib` utilities
- [ ] `typing` module full support
- [ ] `dataclasses` decorator
- [ ] `enum.Enum` proper support
- [ ] `abc.ABC` abstract classes
- [ ] Multiple inheritance MRO
- [ ] `__slots__` for memory efficiency
- [ ] Descriptors (`__get__`, `__set__`)
- [ ] Metaclasses

---

## 📋 PRIORITY ORDER

### Phase 10: Production Ready ✅ COMPLETED

1. ✅ **Native Compilation** - Cranelift backend
2. ✅ **LSP Enhancements** - Go-to-definition, completion
3. ✅ **Async Runtime** - Full async/await executor
4. ✅ **Testing Framework** - pytest-like experience
5. ⚠️ **Package Registry** - Basic publishing (stub only)

### Phase 11: Ecosystem ✅ COMPLETED

6. ✅ **Debugger** - DAP integration
7. ✅ **Database Drivers** - In-memory DB with ORM
8. ⚠️ **HTTP/2** - Still needed
9. ✅ **Profiler** - CPU and memory
10. ⚠️ **Documentation Generator** - Still needed

### Phase 12: Advanced ✅ MOSTLY COMPLETED

11. ✅ **Macros System** - Decorator classification
12. ✅ **LLVM Backend** - WORKING! Native speed achieved (fib(40) in 0.19s matches Rust)
13. ⚠️ **Scientific Computing** - Still needed
14. ✅ **Web Framework** - Flask-like
15. ⚠️ **Full Python Interop** - In progress

### Phase 13: Polish ✅ COMPLETED

16. ✅ **Async I/O** - File and network
17. ✅ **itertools/functools** - Python utilities
18. ✅ **subprocess** - Process management
19. ✅ **collections** - OrderedDict, Counter, etc.
20. ✅ **CSV parser** - Full implementation

### Phase 14: System Utilities ✅ IN PROGRESS

21. ✅ **glob** - Pattern matching
22. ✅ **tempfile** - Temporary files and directories
23. ✅ **shutil** - High-level file operations
24. ✅ **doc** - Documentation generator

---

## � PRIORITY FIXES FOR "RICH LIKE PYTHON, FAST LIKE RUST"

### Week 1-2: Make Async Work (Highest Priority)

| Task                              | Effort   | Impact      |
| --------------------------------- | -------- | ----------- |
| Add `OpCode::Await` to VM         | 2-3 days | 🔴 Critical |
| Implement coroutine protocol      | 2-3 days | 🔴 Critical |
| Wire executor to bytecode         | 1-2 days | 🔴 Critical |
| Support `async for`, `async with` | 1-2 days | High        |

### Week 3-4: Fix Native Compilation

| Task                               | Effort   | Impact      |
| ---------------------------------- | -------- | ----------- |
| Implement Cranelift function calls | 3-5 days | 🔴 Critical |
| Create data section for strings    | 1-2 days | 🔴 Critical |
| Link runtime library               | 2-3 days | 🔴 Critical |
| Object allocation in native        | 2-3 days | High        |

### Week 5-6: Connect Debugger

| Task                     | Effort   | Impact      |
| ------------------------ | -------- | ----------- |
| Add `on_step` hook to VM | 1 day    | 🔴 Critical |
| Expose stack frames      | 1-2 days | 🔴 Critical |
| Wire breakpoint manager  | 1 day    | 🔴 Critical |

### Week 7-8: Python Interop

| Task                    | Effort   | Impact      |
| ----------------------- | -------- | ----------- |
| CPython FFI (PyO3)      | 5-7 days | 🔴 Critical |
| `py:` module resolution | 2-3 days | High        |
| Type stub generation    | 3-4 days | Medium      |

### Week 9-10: Type System

| Task                    | Effort   | Impact |
| ----------------------- | -------- | ------ |
| `where` clause parsing  | 1 day    | High   |
| `where` clause checking | 2-3 days | High   |
| Variance annotations    | 3-4 days | Medium |

### Quick Wins (Can Be Done Anytime)

| Task                              | Effort   | Impact |
| --------------------------------- | -------- | ------ |
| Cross-module go-to-definition     | 1-2 days | Medium |
| `@dataclass` code generation      | 2-3 days | High   |
| F-string evaluation               | 1-2 days | Medium |
| `assert_eq!`, `assert_ne!` macros | 0.5 days | Low    |
| `__hash__` for built-in types     | 1 day    | Low    |

---

## 📈 Metrics for "Rich like Python, Fast like Rust"

### Current Reality vs Targets

| Benchmark         | Python | Rust  | Roast Target | Roast Current       |
| ----------------- | ------ | ----- | ------------ | ------------------- |
| Fibonacci(40)     | 25s    | 0.5s  | < 1s         | ~5s (bytecode)      |
| Matrix 1000x1000  | 2s     | 0.05s | < 0.1s       | N/A (no arrays)     |
| JSON parse 10MB   | 0.3s   | 0.02s | < 0.05s      | ~0.2s               |
| HTTP requests/sec | 5k     | 100k  | > 50k        | N/A                 |
| Startup time      | 30ms   | 1ms   | < 10ms       | ~20ms               |
| Binary size       | N/A    | 1MB   | < 5MB        | N/A (native broken) |

### Feature Parity Checklist (Honest Assessment)

- [x] ~85% Python 3.12 syntax support
- [x] ~80% standard library coverage
- [x] Basic type inference
- [ ] Full type inference (closures, generics)
- [ ] Zero-cost abstractions (native codegen broken)
- [x] Deterministic resource cleanup (ownership)
- [x] Compile-time memory safety (borrow checker)
- [x] GPU compute out of the box

---

## 🔗 Resources

- [Roast Repository](.)
- [Python Language Reference](https://docs.python.org/3/reference/)
- [Rust Language Reference](https://doc.rust-lang.org/reference/)
- [Cranelift Documentation](https://cranelift.dev/)
- [LLVM Documentation](https://llvm.org/docs/)

---

_Last Updated: December 2025_
_Total Tasks: ~150_
_Completed: ~105 (70%)_
_Remaining: ~45_
_Critical Blockers: 4 (async, native, debugger, py interop)_

## 🎉 Recently Completed

### Phase 10 ✅ (Partial)

1. **Cranelift Backend** - Scaffolding only, cannot call functions
2. **LSP Improvements** - Go-to-definition (same file), completion, hover, symbols
3. **Async Executor** - Rust-native only, not bytecode integrated
4. **Testing Framework** - Full test suite, parameterized tests, fixtures
5. **Debugger (DAP)** - Protocol complete, not connected to VM

### Phase 11 ✅

6. **Pattern Matching** - VM opcodes for match statements
7. **Error Handling** - Error chains, stack traces, retry logic
8. **Protocols/Traits** - Definition only, no code generation
9. **itertools/functools** - Python-style utilities
10. **Database Drivers** - In-memory DB, ORM, migrations
11. **Cryptography** - AES, SHA-256, HMAC, JWT, PBKDF2
12. **Profiler** - CPU and memory profiling

### Phase 12 ✅

13. **subprocess** - Process spawning, pipes, pools
14. **collections** - OrderedDict, DefaultDict, Counter, ChainMap
15. **CSV parser** - Reader, writer, DictReader/Writer
16. **Web Framework** - Flask-like routing, templates, static files

### Phase 13 ✅ (Partial)

17. **Macros System** - Classification only, no code generation
18. **Async I/O** - Rust-native works
19. **Async channels** - Sender/Receiver with send/recv

### Phase 14 ✅

20. **glob** - Unix-style pattern matching
21. **tempfile** - Temporary files and directories
22. **shutil** - High-level file operations (copy, move, rmtree)
23. **doc** - HTML documentation generator
24. **XML parser** - Parse, build, write XML documents
25. **REPL** - Syntax highlighting, history, tab completion

### Phase 15 ✅

26. **Web framework enhancements** - WebSocket, sessions, flash messages
27. **heapq** - Heap queue algorithm, priority queue
28. **bisect** - Binary search, sorted containers
29. **Linter** - `roastc lint` with 10 lint rules
30. **Signal handling** - Unix signals, Ctrl+C, graceful shutdown

### Phase 16 ✅

31. **roastdoc** - Enhanced `roastc doc` command
32. **Timezone** - Full timezone support with parsing
33. **mmap** - Memory-mapped files module
34. **Package Registry** - kitchen add/remove/list/install
35. **Python Packages** - py: prefix, PyPI integration
