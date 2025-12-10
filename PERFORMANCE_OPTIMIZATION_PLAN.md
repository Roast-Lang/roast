# Roast Performance Optimization Plan

## Current State: 50x+ Slower Than Python

**Benchmark:** `fib(30)` recursive

- **Python:** 0.05 seconds
- **Roast:** 13+ seconds (hangs/timeout)
- **Goal:** Match Python speed first, then work toward Rust speed

---

## ROOT CAUSE ANALYSIS

After analyzing the codebase, here are the **critical bottlenecks** causing the extreme slowness:

### 1. **Function Call Overhead (CRITICAL - 60% of slowdown)**

**Location:** `crates/vm/src/interpreter.rs:891-920` (OpCode::Call handler)

**Problems:**

```rust
// Every function call does ALL of this:
let args = self.stack.pop_n(argc)?;        // 1. Allocates Vec for args
let func = self.stack.pop()?;               // 2. Pop function value
let code = f.code.clone();                  // 3. Arc::clone (atomic refcount)
let mut frame = CallFrame::new(code, 0);    // 4. Allocates LocalsStorage
for (i, arg) in args.into_iter().enumerate() {  // 5. Iterates and moves
    frame.set_local(i, arg);
}
self.frames.push(frame);                    // 6. Vec push (may reallocate)
```

**Impact:** For `fib(30)`, there are **2.7 million recursive calls**. Each call:

- Allocates a `Vec<Value>` for arguments
- Clones an `Arc<Bytecode>` (atomic operation)
- Creates a new `CallFrame` with `LocalsStorage`
- Pushes to the frames `Vec`

**Python comparison:** Python uses a pre-allocated frame pool and doesn't allocate per-call.

### 2. **`Call` OpCode Not in Fast Path (CRITICAL - 20% of slowdown)**

**Location:** `crates/vm/src/interpreter.rs:316-430` (`run_fast` method)

The `run_fast()` method has inline handling for:

- LoadInt, LoadFast, StoreFast
- Add, Sub, Le
- JumpIfTrue, JumpIfFalse, Jump
- Return

**BUT NOT `Call`!** This means every function call falls through to `execute_instruction()`:

```rust
_ => {
    match self.execute_instruction(&instr)? {  // Expensive function call
        ExecResult::Continue => {}
        // ...
    }
}
```

This adds significant overhead because:

- Virtual function dispatch
- Match on all possible opcodes again
- Extra Result unwrapping

### 3. **`LoadGlobal` for Every Recursive Call (HIGH - 10% of slowdown)**

**Location:** `crates/vm/src/interpreter.rs:561-569`

```rust
OpCode::LoadGlobal | OpCode::LoadName => {
    if let Operand::U16(idx) = instr.operand {
        let name = self.get_name(idx as usize)?;  // String lookup
        let value = self.globals.get(&name)       // HashMap lookup
            .cloned()                              // Clone the Value
            .or_else(|| self.builtins.get(&name)...) // Check builtins
```

For recursive `fib(n)`, each call does:

1. `LoadGlobal "fib"` - HashMap lookup + clone
2. This happens **2.7 million times** for fib(30)

**Python comparison:** Python uses inline caching to avoid repeated lookups.

### 4. **Value Cloning Everywhere (MEDIUM - 10% of slowdown)**

**Location:** `crates/runtime/src/value.rs`

```rust
pub enum Value {
    Str(Arc<str>),                    // Arc clone on every use
    List(Arc<Mutex<Vec<Value>>>),     // Arc clone + Mutex
    Function(Arc<RoastFunction>),     // Arc clone for every call
    // ...
}
```

Every time a value is passed, returned, or used, it's cloned. For integers this is cheap, but for `Function` values:

- `Arc::clone` = atomic reference count increment
- Atomic ops are ~20-50x slower than regular ops

---

## OPTIMIZATION PLAN (Ordered by Impact)

### Phase 1: Quick Wins (Get to Python Speed) - 1-2 Days

#### 1.1 Add `Call` to Fast Path

**Expected improvement: 2-3x**

```rust
// In run_fast(), add before the fallback:
OpCode::Call => {
    if let Operand::U8(argc) = instr.operand {
        let argc = argc as usize;
        // Inline the hot path for simple function calls
        self.fast_call(argc)?;
    }
}
```

#### 1.2 Avoid Vec Allocation for Arguments

**Expected improvement: 1.5-2x**

```rust
// Instead of:
let args = self.stack.pop_n(argc)?;  // Allocates Vec

// Use a small-vec optimization:
fn fast_call(&mut self, argc: usize) -> VMResult<()> {
    // For small arg counts, use stack-allocated array
    let mut args: [Value; 8] = Default::default();
    for i in (0..argc).rev() {
        args[i] = self.stack.pop()?;
    }
    // ...
}
```

#### 1.3 Cache Function Lookups

**Expected improvement: 1.5x**

```rust
// Add inline cache to CallFrame or instruction
struct InlineCache {
    cached_func: Option<Arc<RoastFunction>>,
    global_version: u64,  // Invalidate when globals change
}
```

### Phase 2: Major Optimizations (Beat Python) - 1 Week

#### 2.1 Frame Pool / Pre-allocation

**Expected improvement: 2-3x**

```rust
// Pre-allocate frames to avoid per-call allocation
struct FramePool {
    frames: Vec<CallFrame>,
    free_list: Vec<usize>,
}

impl VM {
    fn acquire_frame(&mut self) -> &mut CallFrame {
        // Reuse existing frame instead of allocating
    }
}
```

#### 2.2 Use `Rc` Instead of `Arc` for Single-Threaded Mode

**Expected improvement: 1.5-2x**

```rust
// Arc uses atomic operations (expensive)
// Rc uses simple increment (10x faster)
#[cfg(not(feature = "multi-threaded"))]
type SharedPtr<T> = Rc<T>;

#[cfg(feature = "multi-threaded")]
type SharedPtr<T> = Arc<T>;
```

#### 2.3 Specialized Integer Dispatch

**Expected improvement: 1.3x**

For numeric-heavy code, add type-specialized instructions:

```rust
OpCode::AddInt,     // Assumes both operands are Int
OpCode::SubInt,
OpCode::LeInt,
OpCode::CallDirect, // Direct function pointer, no lookup
```

### Phase 3: Advanced Optimizations (Approach Rust Speed) - 2+ Weeks

#### 3.1 Fix the JIT Compiler

The baseline JIT exists but doesn't actually work:

```rust
// In baseline.rs:emit_call_runtime()
fn emit_call_runtime(&mut self, _func: RuntimeFn) {
    // PLACEHOLDER - doesn't actually call anything!
    self.emit_bytes(&[0xff, 0x15, 0x00, 0x00, 0x00, 0x00]);
}
```

Need to:

1. Create actual runtime function pointers
2. Wire up the JIT to execute native code
3. Add OSR (On-Stack Replacement) for hot loops

#### 3.2 NaN-Boxing for Values

**Expected improvement: 3-5x for numeric code**

```rust
// Pack values into 64 bits using IEEE 754 NaN space
type PackedValue = u64;

// Float: direct IEEE 754
// Int: tagged (48-bit int + 16-bit tag)
// Pointer: 48-bit pointer + tag
```

#### 3.3 Threaded/Computed Goto Dispatch

**Expected improvement: 2x**

Instead of match-based dispatch:

```rust
// Current: ~15 cycles per instruction
match opcode {
    OpCode::Add => ...
    OpCode::Sub => ...
}

// Threaded code: ~2 cycles per instruction
static DISPATCH: [fn(&mut VM); 256] = [...];
DISPATCH[opcode as usize](self);
```

---

## IMMEDIATE ACTION ITEMS

### Step 1: Benchmark Infrastructure

Create proper benchmarks to measure progress:

```rust
// tests/bench_fib.rs
#[bench]
fn bench_fib_30() {
    // Measure execution time
}
```

### Step 2: Profile-Guided Development

```bash
# Run with profiling
cargo build --release
perf record ./target/release/roastc run fib.roast
perf report
```

### Step 3: Quick Fix Sequence

1. Add `Call` opcode to `run_fast()` inline handling
2. Add `LoadGlobal` to `run_fast()` with caching
3. Pre-allocate argument arrays for small functions
4. Pool CallFrames for reuse

---

## EXPECTED RESULTS

| Phase   | Target Time (fib30) | vs Python       |
| ------- | ------------------- | --------------- |
| Current | 13+ seconds         | 260x slower     |
| Phase 1 | 0.2-0.5s            | 4-10x slower    |
| Phase 2 | 0.03-0.05s          | ~Same as Python |
| Phase 3 | 0.001-0.01s         | 5-50x faster    |

---

## FILES TO MODIFY

### Critical Path (Phase 1)

1. `crates/vm/src/interpreter.rs` - Add Call to fast path, inline caching
2. `crates/vm/src/stack.rs` - Avoid allocations in pop_n
3. `crates/vm/src/frame.rs` - Frame pooling

### Phase 2

4. `crates/runtime/src/value.rs` - Rc vs Arc, consider NaN-boxing
5. `crates/codegen/src/bytecode.rs` - Add specialized opcodes

### Phase 3

6. `crates/jit/src/baseline.rs` - Fix runtime calls
7. `crates/vm/src/jit_integration.rs` - Wire up JIT execution
