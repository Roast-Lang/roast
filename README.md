# 🔥 Roast Programming Language

**Roast** is a compiled programming language that combines Python's elegant syntax with Rust-level performance. It features gradual static typing, optional ownership semantics, and compiles to native machine code.

```python
def main() -> None:
    print("Hello, Roast! 🔥")
```

## ✨ Features

- **Python-like Syntax**: Familiar, readable syntax that Python developers will feel at home with
- **Static Typing**: Compile-time type checking with full type inference
- **High Performance**: Native compilation with optimizations targeting Rust-level performance
- **Gradual Typing**: Optional type annotations for incremental adoption
- **Ownership System**: Rust-like ownership and borrowing for memory safety without GC
- **Python Compatibility**: Import and use Python modules seamlessly
- **Async/Await**: First-class async support with an efficient runtime
- **Modern Tooling**: REPL, LSP, package manager, and formatter included

## 🚀 Quick Start

```bash
# Build from source
cargo build --release

# Create a new project with Kitchen
kitchen new my_project
cd my_project

# Build and run
kitchen run

# Or use roastc directly
roastc run src/main.roast

# Start the REPL
roastc repl
```

## 🍳 Kitchen - Project Manager

**Kitchen** is the all-in-one project and environment manager for Roast (like Cargo + uv):

```bash
# Create a new project
kitchen new my_app              # Binary application
kitchen new my_lib --template library  # Library
kitchen new my_web --template web      # Web application
kitchen new my_gpu --template gpu      # GPU compute app

# Virtual environments
kitchen venv                    # Create .venv
source .venv/bin/activate       # Activate

# Dependencies
kitchen add requests           # Add dependency
kitchen add pytest --dev       # Dev dependency
kitchen install                # Install all

# Build & Run
kitchen build                  # Debug build
kitchen build --release        # Release build
kitchen run                    # Build and run
kitchen test                   # Run tests
kitchen bench                  # Benchmarks

# Publishing
kitchen login                  # Authenticate
kitchen publish                # Publish to registry

# GPU Support
kitchen gpu                    # Show GPU info
kitchen build --gpu            # Build with GPU
```

### roast.toml Configuration

```toml
[package]
name = "my_project"
version = "0.1.0"
edition = "2024"
entry = "src/main.roast"

[dependencies]
requests = "1.0"

[dev-dependencies]
pytest = "7.0"

[scripts]
test = "kitchen test"
lint = "roastc check src/"

[build.gpu]
enabled = true
cuda_archs = ["sm_80", "sm_90"]
```

## 📖 Examples

### Basic Types and Functions

```python
# Typed function
def sum_list(xs: list[int]) -> int:
    total: int = 0
    for x in xs:
        total += x
    return total

# Generic function
def first[T](items: list[T]) -> T | None:
    if items:
        return items[0]
    return None

# Lambda expressions
double = lambda x: x * 2
squares = [x ** 2 for x in range(10)]
```

### Classes and OOP

```python
class Point:
    def __init__(self, x: float, y: float) -> None:
        self.x = x
        self.y = y
    
    def distance(self, other: Point) -> float:
        dx = self.x - other.x
        dy = self.y - other.y
        return (dx ** 2 + dy ** 2) ** 0.5
    
    def __add__(self, other: Point) -> Point:
        return Point(self.x + other.x, self.y + other.y)
```

### Ownership and Borrowing

```python
# Owned value (moved on assignment)
def take_ownership(data: owned list[int]) -> int:
    return sum(data)

# Borrowed reference (read-only access)
def print_length(s: &str) -> None:
    print(f"Length: {len(s)}")

# Mutable borrow
def append_item(items: &mut list[int], value: int) -> None:
    items.append(value)
```

### Async/Await

```python
import asyncio

async def fetch_data(url: str) -> dict:
    response = await http.get(url)
    return response.json()

async def main() -> None:
    results = await asyncio.gather(
        fetch_data("https://api.example.com/users"),
        fetch_data("https://api.example.com/posts"),
    )
    print(results)

asyncio.run(main())
```

### GPU Compute

```python
from roast.gpu import Device, Tensor, kernel

# Auto-detect GPU (CUDA, OpenCL, Metal)
device = Device.default()
print(f"Using: {device.name}")  # e.g., "NVIDIA GeForce RTX 3060 Ti"

# Create tensors on GPU
a = Tensor.rand((1000, 1000), device=device)
b = Tensor.rand((1000, 1000), device=device)

# Matrix multiplication on GPU
c = a @ b

# Custom kernel
@kernel
def vector_add(a: Tensor[float], b: Tensor[float], c: Tensor[float]) -> None:
    idx = thread_idx()
    if idx < len(a):
        c[idx] = a[idx] + b[idx]

# Launch with [grid_size, block_size]
vector_add[n // 256, 256](a, b, c)

# Neural network ops
x = Tensor.randn((64, 784), device=device)
y = relu(x @ weights + bias)
probs = softmax(y, dim=-1)
```

## 🏗️ Project Structure

```
roast/
├── crates/
│   ├── common/          # Shared utilities (diagnostics, spans, interner)
│   ├── ast/             # Abstract Syntax Tree definitions
│   ├── parser/          # Lexer and parser
│   ├── typer/           # Type system and type checker
│   ├── hir/             # High-level IR
│   ├── mir/             # Mid-level IR with ownership
│   ├── borrowck/        # Borrow checker (Polonius-inspired)
│   ├── optimizer/       # Optimization passes
│   ├── codegen/         # Bytecode generation
│   ├── vm/              # Virtual machine
│   ├── runtime/         # Runtime library
│   ├── pycompat/        # Python compatibility layer
│   ├── lsp/             # Language server protocol
│   ├── package_manager/ # Package manager (roastpkg)
│   ├── cli/             # Compiler CLI (roastc)
│   ├── stdlib/          # Standard library
│   ├── kitchen/         # Project manager (like Cargo/uv)
│   └── gpu/             # GPU compute backend
├── examples/            # Example programs
├── tests/               # Test suite
└── docs/                # Documentation
```

## 🔧 Building from Source

### Prerequisites

- Rust 1.70+ with Cargo
- Git

### Build

```bash
# Clone the repository
git clone https://github.com/roast-lang/roast
cd roast

# Build all crates
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path crates/cli
```

## 🛠️ CLI Reference

### Compiler (roastc)

```bash
# Compile a file
roastc build src/main.roast

# Build and run
roastc run src/main.roast

# Start interactive REPL
roastc repl

# Evaluate an expression
roastc eval "print(2 + 2)"

# Type-check without building
roastc check src/

# Format source files
roastc fmt src/

# Run tests
roastc test

# Generate documentation
roastc doc --open

# Create new project
roastc init my_project --git

# Show version
roastc version --verbose
```

### REPL Commands

```
:help     Show available commands
:quit     Exit the REPL
:clear    Clear the screen
:type     Show type of expression
:ast      Show AST of code
:load     Load and run a file
:reset    Reset state
:vars     Show defined variables
```

### Package Manager (roastpkg)

```bash
roastpkg init my_package       # Initialize new package
roastpkg add requests          # Add dependency
roastpkg install               # Install dependencies
roastpkg build                 # Build package
roastpkg publish               # Publish to registry
```

### Language Server (roast-lsp)

The Roast language server provides full IDE support:
- ✓ Autocomplete
- ✓ Real-time diagnostics
- ✓ Hover information
- ✓ Go to definition
- ✓ Find references
- ✓ Rename refactoring
- ✓ Format document
- ✓ Inline type hints

## 🐍 Python Compatibility

Roast provides comprehensive Python compatibility:

### Importing Python Modules
```python
import json
import math
import os
from collections import Counter, deque
from itertools import chain, permutations
```

### Supported Standard Library Modules
- `math` - Mathematical functions
- `json` - JSON encoding/decoding
- `os`, `os.path` - Operating system interface
- `sys` - System-specific parameters
- `collections` - Container datatypes
- `itertools` - Iterator functions
- `functools` - Higher-order functions
- `typing` - Type hints
- `datetime` - Date and time
- `pathlib` - Object-oriented paths
- `subprocess` - Process spawning
- `threading` - Thread-based parallelism
- `hashlib` - Secure hashes
- `base64` - Base64 encoding
- `dataclasses` - Data classes

### Migration Tool
```bash
# Migrate a Python file to Roast
roastc migrate script.py -o script.roast

# Migrate a directory
roastc migrate python_project/ -o roast_project/

# Dry run (preview changes)
roastc migrate script.py --dry-run

# Add ownership annotations
roastc migrate script.py --ownership
```

### Python Decorators
```python
@staticmethod
@classmethod
@property
@dataclass
@functools.lru_cache
@contextmanager
@deprecated("Use new_function instead")
```

## ⚡ Performance & Optimization

Roast includes a comprehensive optimization pipeline:

### Optimization Passes

| Pass | Description |
|------|-------------|
| Constant Folding | Evaluate constant expressions at compile time |
| Copy Propagation | Replace copies with original values |
| Dead Code Elimination | Remove unused code and unreachable blocks |
| Common Subexpression Elimination | Reuse computed values |
| Strength Reduction | Replace expensive ops (mul → shift) |
| Loop Invariant Code Motion | Hoist invariant code out of loops |
| Tail Call Optimization | Convert tail calls to jumps |
| Function Inlining | Inline small functions at call sites |

### Optimization Levels

```bash
roastc build -O0 src/main.roast  # No optimization
roastc build -O1 src/main.roast  # Basic optimization
roastc build -O2 src/main.roast  # Standard optimization (default)
roastc build -O3 src/main.roast  # Aggressive optimization
roastc build -Os src/main.roast  # Optimize for size
```

### Benchmarking

```python
from roast.bench import Bencher

def main():
    bench = Bencher()
    
    bench.run("fibonacci", lambda: fib(30))
    bench.run("sorting", lambda: sorted(data))
    
    bench.print_report()
```

### Profiling

```python
from roast.profile import Profiler

profiler = Profiler()

profiler.time("parsing", lambda: parse_file("input.txt"))
profiler.time("processing", lambda: process(data))

profiler.print_summary()
```

## 🎮 GPU Computing

Roast includes a comprehensive GPU compute backend for high-performance parallel computing.

### Supported Backends

| Backend | Platforms | Status |
|---------|-----------|--------|
| **CUDA** | NVIDIA GPUs | ✅ Full support |
| **OpenCL** | AMD, Intel, NVIDIA | 🔄 Partial |
| **Metal** | macOS/iOS | 🔄 Partial |
| **Vulkan** | Cross-platform | 🔄 Planned |
| **CPU** | All | ✅ Fallback |

### Device Detection

```python
from roast.gpu import Device, list_devices

# List all GPUs
for dev in list_devices():
    print(f"{dev.name} ({dev.device_type})")
    print(f"  Memory: {dev.total_memory / 1e9:.1f} GB")
    print(f"  Compute: {dev.compute_capability}")

# Get default device
device = Device.default()
```

### Tensor Operations

```python
from roast.gpu import Tensor, Device

device = Device.default()

# Create tensors
a = Tensor.zeros((1000, 1000), dtype="float32", device=device)
b = Tensor.ones((1000, 1000), device=device)
c = Tensor.rand((1000, 1000), device=device)
d = Tensor.randn((1000, 1000), device=device)  # Normal distribution

# Arithmetic
result = a + b * c - d
result = a @ b  # Matrix multiplication

# Reductions
total = result.sum()
avg = result.mean()
maximum = result.max()

# Neural network ops
from roast.gpu.ops import relu, sigmoid, softmax, gelu
y = relu(x)
y = softmax(logits, dim=-1)
y = gelu(x)
```

### Custom Kernels

```python
from roast.gpu import kernel, Tensor, Device

@kernel
def saxpy(a: float, x: Tensor[float], y: Tensor[float], z: Tensor[float]) -> None:
    """SAXPY: z = a*x + y"""
    idx = thread_idx()
    if idx < len(x):
        z[idx] = a * x[idx] + y[idx]

# Launch configuration: [grid_size, block_size]
n = 1_000_000
saxpy[n // 256, 256](2.0, x, y, z)

# Or use automatic configuration
saxpy.launch(n)(2.0, x, y, z)
```

### Memory Management

```python
# Explicit memory control
ptr = device.alloc(1024 * 1024)  # 1 MB
device.free(ptr)

# Tensor memory
tensor = Tensor.zeros((1000,), device=device)
host_data = tensor.to_cpu()  # Copy to host
tensor2 = Tensor.from_slice(host_data, device=device)  # Copy to device

# Pinned memory for faster transfers
from roast.gpu.memory import PinnedMemory
pinned = PinnedMemory(size=1024*1024)
```

### Integration with Kitchen

```bash
# Build with GPU support
kitchen build --gpu

# GPU info
kitchen gpu

# GPU project template
kitchen new my_gpu_app --template gpu
```

### NVRTC Runtime Compilation

Roast includes full NVRTC (NVIDIA Runtime Compilation) integration for JIT-compiling CUDA kernels at runtime:

```python
from roast.gpu import JitCompiler, NvrtcCompileOptions

# Create JIT compiler (auto-detects GPU compute capability)
jit = JitCompiler.for_device(8, 6)  # RTX 3060 Ti = SM 8.6

# CUDA source
source = '''
extern "C" __global__ void vector_add(
    const float *a, const float *b, float *c, int n
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        c[idx] = a[idx] + b[idx];
    }
}
'''

# Compile and cache
kernel = jit.get_kernel(source, "vector_add")

# Launch
kernel.launch([n // 256, 1, 1], [256, 1, 1], [a_ptr, b_ptr, c_ptr, n])

# Pre-built kernel templates
from roast.gpu.cuda import templates
matmul_src = templates.matmul_tiled(16)  # 16x16 tiles
reduce_src = templates.reduce_sum(256)   # 256 threads/block
```

#### Compilation Options

```python
options = NvrtcCompileOptions(
    arch="sm_86",        # Target architecture
    opt_level=3,         # Optimization level (0-3)
    fast_math=True,      # Enable fast math
    debug=False,         # Debug info
    line_info=True,      # Line info for profiling
    max_registers=64,    # Max registers per thread
)

kernel = jit.compile(source, "kernel_name", options)
```

#### Built-in Kernel Templates

| Template | Description |
|----------|-------------|
| `vector_add(dtype)` | Element-wise addition |
| `vector_mul(dtype)` | Element-wise multiplication |
| `scalar_mul(dtype)` | Scalar multiplication |
| `saxpy(dtype)` | SAXPY: z = αx + y |
| `relu(dtype)` | ReLU activation |
| `sigmoid()` | Sigmoid activation |
| `matmul(dtype)` | Matrix multiplication (naive) |
| `matmul_tiled(tile_size)` | Tiled matrix multiplication |
| `reduce_sum(block_size)` | Parallel reduction sum |

### cuBLAS Integration

GPU-accelerated BLAS operations using NVIDIA cuBLAS:

```python
from roast.gpu import BlasOps, Tensor

blas = BlasOps()  # Auto-enables Tensor Cores

# Matrix multiplication (uses cuBLAS SGEMM)
c = blas.matmul(a, b)

# Batched matrix multiplication
c = blas.bmm(a, b)  # 3D tensors

# Vector operations
dot = blas.dot(x, y)      # Dot product
norm = blas.norm(x)       # L2 norm
blas.scale(2.0, x)        # x = 2 * x
blas.axpy(alpha, x, y)    # y = alpha*x + y
```

### cuDNN Integration

Deep learning primitives using NVIDIA cuDNN:

```python
from roast.gpu import DnnOps

dnn = DnnOps()

# Activation functions (forward pass)
dnn.relu(x, y)
dnn.sigmoid(x, y)
dnn.tanh(x, y)
dnn.elu(x, y, alpha=1.0)
dnn.swish(x, y)

# Softmax
dnn.softmax(x, y, dim=1)
dnn.log_softmax(x, y, dim=1)

# Pooling
dnn.max_pool2d(x, y, kernel_size=(2, 2), stride=(2, 2), padding=(0, 0))
dnn.avg_pool2d(x, y, kernel_size=(2, 2), stride=(2, 2), padding=(0, 0))
```

### Multi-GPU Support

Data parallelism and distributed training:

```python
from roast.gpu import MultiGpu, DataParallel

# Initialize multi-GPU
mgpu = MultiGpu()
print(f"Found {mgpu.device_count()} GPUs")

# NCCL for collective operations
if mgpu.has_nccl():
    mgpu.init_nccl([0, 1, 2, 3])  # Use GPUs 0-3
    
# Data parallel training
dp = DataParallel([0, 1])  # Use 2 GPUs
scattered = dp.scatter(batch)
# ... run on each GPU ...
result = dp.gather(outputs)
dp.reduce_gradients(grads)  # AllReduce via NCCL
```

### Automatic Differentiation

PyTorch-style autograd for gradient computation:

```python
from roast.gpu.autograd import Variable, SGD, Adam, no_grad

# Create trainable parameters
x = Variable.requires_grad(Tensor.rand((100, 100)))
y = Variable.requires_grad(Tensor.rand((100, 100)))

# Forward pass (builds computation graph)
z = autograd.matmul(x, y)
loss = autograd.sum(autograd.pow(z, 2))

# Backward pass
loss.backward()

# Access gradients
print(x.grad())  # dL/dx
print(y.grad())  # dL/dy

# Optimizers
params = [x, y]
optimizer = Adam(params, lr=0.001).betas(0.9, 0.999)
optimizer.zero_grad()
# ... forward + backward ...
optimizer.step()

# Disable gradient tracking
with no_grad():
    result = expensive_inference(model, data)
```

### Full Memory Management

Efficient GPU memory with caching allocator:

```python
from roast.gpu import cuda_alloc, cuda_free, cuda_mem_info, cuda_empty_cache
from roast.gpu import DeviceMemory, PinnedHostMemory, UnifiedMemory

# Get memory info
free, total = cuda_mem_info()
print(f"GPU Memory: {free / 1e9:.1f} GB free / {total / 1e9:.1f} GB total")

# Device memory with RAII
mem = DeviceMemory.alloc(allocator, 1024 * 1024)  # 1 MB
mem.copy_from_host(data)
mem.copy_to_host(buffer)
mem.zero()  # Clears to 0
# Automatically freed when dropped

# Pinned host memory (faster transfers)
pinned = PinnedHostMemory.alloc(lib, 1024 * 1024)
pinned.as_mut_slice()[0] = 42

# Unified/managed memory (auto-migrating)
unified = UnifiedMemory.alloc(lib, 1024 * 1024)
unified.prefetch_to_device(0, stream)  # Move to GPU 0
unified.prefetch_to_host(stream)       # Move to CPU

# Cache management
cuda_empty_cache()  # Release cached memory
```

## 📚 Standard Library

Roast includes a comprehensive standard library:

### Core Modules
| Module | Description |
|--------|-------------|
| `fs` | File system operations (read, write, mkdir, walk) |
| `path` | Path manipulation (join, basename, dirname, normalize) |
| `net` | Networking (TCP, UDP sockets) |
| `http` | HTTP client and utilities |
| `io` | Input/output streams |

### Concurrency
| Module | Description |
|--------|-------------|
| `sync` | Synchronization primitives (Mutex, RwLock, Semaphore) |
| `thread` | Thread management and thread pools |
| `channel` | MPSC and MPMC channels |
| `async_utils` | Async/await utilities |

### Data Structures
| Module | Description |
|--------|-------------|
| `heap` | Binary heaps (min/max) |
| `queue` | Queues, deques, ring buffers |
| `graph` | Graph algorithms (BFS, DFS, Dijkstra) |

### Encoding
| Module | Description |
|--------|-------------|
| `json` | JSON parsing and serialization |
| `base64` | Base64 encoding/decoding |
| `hex` | Hexadecimal encoding/decoding |

### Utilities
| Module | Description |
|--------|-------------|
| `time` | Date/time handling |
| `duration` | Duration parsing and formatting |
| `hash` | Hash functions (FNV, CRC32, Adler32) |
| `random` | Random number generation |
| `regex` | Pattern matching |
| `fmt` | String formatting |
| `testing` | Testing framework |
| `error` | Error handling |
| `result` | Result utilities |

### Example Usage
```python
from roast.fs import read_text, write_text
from roast.json import parse, stringify
from roast.time import DateTime
from roast.thread import ThreadPool

# Read and parse JSON
data = parse(read_text("config.json"))

# Create a thread pool
pool = ThreadPool(4)
pool.execute(lambda: print("Hello from thread!"))

# Get current time
now = DateTime.now()
print(now.format("%Y-%m-%d %H:%M:%S"))
```

## ⚙️ Configuration

Project configuration in `roast.toml`:

```toml
[package]
name = "my_project"
version = "0.1.0"
edition = "2024"
authors = ["Your Name <you@example.com>"]
description = "A Roast project"

[dependencies]
requests = "1.0"

[dev-dependencies]
pytest = "7.0"
```

## 🗺️ Roadmap

- [x] **Phase 1**: Parser + AST + Lexer
- [x] **Phase 2**: Type system + Borrow checker + MIR
- [x] **Phase 3**: VM + Optimizations + Runtime
- [x] **Phase 4**: CLI + REPL + Tooling
- [x] **Phase 5**: Full Python compatibility
- [x] **Phase 6**: Complete standard library
- [x] **Phase 7**: Performance optimization + Native compilation
- [x] **Phase 8**: Kitchen - Project & Environment Manager
- [x] **Phase 9**: GPU Compute Backend - Complete!
  - Multi-backend: CUDA, OpenCL, Metal, Vulkan
  - Tensor operations with GPU acceleration
  - Kernel compilation from Roast DSL
  - Memory management (host ↔ device)
  - Neural network operations (ReLU, Softmax, GELU, etc.)
  - **NVRTC Integration**: Full runtime compilation
  - **cuBLAS Integration**: Optimized BLAS operations
  - **cuDNN Integration**: Deep learning primitives
  - **Multi-GPU Support**: NCCL, peer-to-peer, data parallelism
  - **Automatic Differentiation**: Full autograd system with optimizers
  - **Full Memory Management**: Caching allocator, pinned/unified memory
- [ ] **Phase 10**: Production release

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📜 License

Roast is dual-licensed under MIT and Apache 2.0.
