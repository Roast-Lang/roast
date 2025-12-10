# Roast Architecture: Path to Rust-Level Performance

## Current Architecture Analysis

### What Roast Has (Good Foundation)

```
Source (.roast)
     │
     ▼
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
│ Parser  │───▶│   HIR   │───▶│   MIR   │───▶│Bytecode │
│  (AST)  │    │ (Typed) │    │  (CFG)  │    │ (Stack) │
└─────────┘    └─────────┘    └─────────┘    └─────────┘
                                                  │
                                                  ▼
                                            ┌──────────┐
                                            │    VM    │  ◀── SLOW (interpreter)
                                            │Interpreter│
                                            └──────────┘
```

**Good:**

- Full type system with inference
- MIR with CFG (like Rust's MIR)
- Ownership/borrow checking infrastructure
- Cranelift backend exists (but unused!)

**Bad:**

- Only generates bytecode, never native code
- VM interprets bytecode (50x+ slower than native)
- Cranelift backend is NOT wired up to CLI

### What Rust Does (Target Architecture)

```
Source (.rs)
     │
     ▼
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
│ Parser  │───▶│   HIR   │───▶│   MIR   │───▶│  LLVM   │
│  (AST)  │    │ (Typed) │    │  (CFG)  │    │   IR    │
└─────────┘    └─────────┘    └─────────┘    └─────────┘
                                                  │
                                                  ▼
                                            ┌──────────┐
                                            │  Native  │  ◀── FAST (machine code)
                                            │  Binary  │
                                            └──────────┘
```

---

## The Core Problem

\*\*Roast NEVER generates native code, even though it has:

- Cranelift backend (complete, in `crates/codegen/src/cranelift.rs`)
- JIT infrastructure (in `crates/jit/`)
- Native code gen module (in `crates/codegen/src/native.rs`)\*\*

The CLI **only** uses bytecode:

```rust
// crates/cli/src/commands.rs - line ~315
let mut bytecode_builder = roast_codegen::BytecodeBuilder::with_interner(&func_name, interner);
let bytecode = bytecode_builder.compile(&mir_body);
// Then runs via VM interpreter... ALWAYS
```

---

## Solution: Wire Up Native Compilation

### Phase 1: Enable AOT Native Compilation (1-2 weeks)

**Goal:** `roastc build --native program.roast` produces a native executable

#### 1.1 Add Native Build Path to CLI

```rust
// crates/cli/src/commands.rs

pub fn build_native(file: &Path, output: &Path) -> Result<()> {
    let interner = Interner::new();

    // Parse → HIR → MIR (same as before)
    let mir_bodies = compile_to_mir(file, &interner)?;

    // NEW: Use Cranelift instead of bytecode
    let mut cranelift = CraneliftBackend::new(None, OptLevel::Aggressive)?;

    for body in &mir_bodies {
        cranelift.compile_function(body)?;
    }

    // Generate object file
    let object = cranelift.finish()?;
    let object_bytes = write_object(object)?;

    // Link to executable
    link_executable(&[object_bytes], output.to_str().unwrap(), &[])?;

    Ok(())
}
```

#### 1.2 Add CLI Flag

```rust
// In Commands::Build
#[arg(long)]
native: bool,  // Compile to native executable
```

#### 1.3 Fix Missing Runtime Functions

The Cranelift backend needs runtime support for:

- String allocation/manipulation
- List/Dict operations
- Print and I/O
- Memory management

Create `crates/runtime/src/native_runtime.rs`:

```rust
#[no_mangle]
pub extern "C" fn roast_print(s: *const c_char) {
    // ...
}

#[no_mangle]
pub extern "C" fn roast_alloc_string(len: usize) -> *mut u8 {
    // ...
}
```

### Phase 2: Enable JIT Compilation (1-2 weeks)

**Goal:** Hot functions automatically compile to native while running

#### 2.1 Wire Up JIT to Actually Execute Native Code

Current state (`crates/vm/src/jit_integration.rs`):

```rust
// JIT compiles code but NEVER executes it!
pub fn compile_baseline(&mut self, func_id: u32, bytecode: &Bytecode) -> Result<(), String> {
    match self.engine.compile_baseline(func_id, bytecode) {
        Ok(_compiled) => {
            // compiled code is generated but thrown away!
            // Need to actually CALL it
        }
    }
}
```

Fix:

```rust
pub fn call_compiled(&mut self, func_id: u32, args: &[Value]) -> Option<Value> {
    if let Some(compiled) = self.engine.get_cached(func_id) {
        // Actually execute the native code!
        let entry: fn(*const Value, usize) -> Value =
            unsafe { std::mem::transmute(compiled.entry_point()) };
        return Some(entry(args.as_ptr(), args.len()));
    }
    None
}
```

#### 2.2 Modify Interpreter to Use JIT

```rust
// In run_fast(), for OpCode::Call:
fn call_function(&mut self, func: &RoastFunction, args: Vec<Value>) -> VMResult<Value> {
    let func_id = self.jit_manager.get_func_id(&func.code);

    // Try JIT first
    if let Some(result) = self.jit_manager.call_compiled(func_id, &args) {
        return Ok(result);
    }

    // Fallback to interpreter (and record for JIT)
    self.jit_manager.on_function_entry(func_id, &func.code);
    // ... existing interpreter code
}
```

### Phase 3: Optimize Value Representation (1 week)

**Goal:** Zero-cost values for primitive types

#### 3.1 NaN-Boxing

```rust
// crates/runtime/src/value.rs

/// A 64-bit value that can hold any Roast value
/// Uses NaN-boxing for efficient storage
#[repr(transparent)]
pub struct PackedValue(u64);

impl PackedValue {
    // Encoding:
    // Float:   IEEE 754 double (NaN values are special)
    // Int:     0x7FF8_XXXX_XXXX_XXXX (48-bit signed int)
    // Pointer: 0x7FFC_XXXX_XXXX_XXXX (48-bit pointer)
    // Bool:    0x7FFE_0000_0000_000X (X = 0 or 1)
    // None:    0x7FFE_0000_0000_0002

    const TAG_INT: u64    = 0x7FF8_0000_0000_0000;
    const TAG_PTR: u64    = 0x7FFC_0000_0000_0000;
    const TAG_BOOL: u64   = 0x7FFE_0000_0000_0000;
    const VAL_NONE: u64   = 0x7FFE_0000_0000_0002;
    const VAL_TRUE: u64   = 0x7FFE_0000_0000_0001;
    const VAL_FALSE: u64  = 0x7FFE_0000_0000_0000;

    #[inline(always)]
    pub fn from_int(n: i64) -> Self {
        // Fits in 48 bits? Use NaN-boxing
        if n >= -(1 << 47) && n < (1 << 47) {
            Self(Self::TAG_INT | (n as u64 & 0xFFFF_FFFF_FFFF))
        } else {
            // Box as BigInt
            Self::from_ptr(Box::into_raw(Box::new(n)))
        }
    }

    #[inline(always)]
    pub fn as_int(&self) -> Option<i64> {
        if self.0 & 0xFFFF_0000_0000_0000 == Self::TAG_INT {
            // Sign-extend from 48 bits
            let raw = self.0 & 0xFFFF_FFFF_FFFF;
            let signed = ((raw as i64) << 16) >> 16;
            Some(signed)
        } else {
            None
        }
    }
}
```

#### 3.2 Specialized Operations

```rust
// Fast path for integer operations (no boxing/unboxing)
#[inline(always)]
fn add_packed(a: PackedValue, b: PackedValue) -> PackedValue {
    match (a.as_int(), b.as_int()) {
        (Some(x), Some(y)) => PackedValue::from_int(x.wrapping_add(y)),
        _ => add_slow(a, b),
    }
}
```

### Phase 4: Static Compilation of Pure Functions (2+ weeks)

**Goal:** Functions with known types compile to zero-overhead native code

#### 4.1 Type Specialization

When types are known at compile time:

```python
def fib(n: int) -> int:  # Types are known!
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)
```

Generate specialized MIR:

```rust
// Instead of generic Value operations:
fn fib_specialized(n: i64) -> i64 {
    if n <= 1 { return n; }
    fib_specialized(n - 1) + fib_specialized(n - 2)
}
```

This compiles to native code that's as fast as C/Rust.

#### 4.2 Monomorphization

Like Rust generics - generate specialized code for each type combination:

```python
def sum[T](items: list[T]) -> T:
    ...

# Called with list[int] → generates sum_int()
# Called with list[float] → generates sum_float()
```

---

## Implementation Priority

### Week 1-2: Native AOT Compilation

1. ✅ Cranelift backend exists
2. 🔧 Wire up to CLI (`roastc build --native`)
3. 🔧 Create native runtime functions
4. 🔧 Implement linker integration

### Week 3-4: JIT Integration

1. ✅ JIT infrastructure exists
2. 🔧 Wire up JIT code execution
3. 🔧 Add tier-up in interpreter
4. 🔧 Implement OSR (on-stack replacement)

### Week 5-6: Value Optimization

1. 🔧 Implement NaN-boxing
2. 🔧 Remove Arc/Mutex for single-threaded
3. 🔧 Specialize integer operations

### Week 7+: Advanced Optimizations

1. 🔧 Type specialization
2. 🔧 Monomorphization
3. 🔧 Inline caching
4. 🔧 Escape analysis

---

## Expected Performance Gains

| Stage                 | fib(30) Time | vs Python     | vs Rust       |
| --------------------- | ------------ | ------------- | ------------- |
| Current (interpreter) | 13+ sec      | 260x slower   | 10000x slower |
| Phase 1 (AOT native)  | 0.01-0.05s   | Same/faster   | 5-10x slower  |
| Phase 2 (JIT)         | 0.01-0.02s   | 2-5x faster   | 2-5x slower   |
| Phase 3 (NaN-boxing)  | 0.005-0.01s  | 5-10x faster  | ~Same         |
| Phase 4 (specialized) | 0.001-0.005s | 10-50x faster | ~Same         |

---

## Files to Modify

### Critical (Phase 1)

1. `crates/cli/src/commands.rs` - Add `build_native()` function
2. `crates/cli/src/main.rs` - Add `--native` flag
3. `crates/codegen/src/cranelift.rs` - Fix runtime function calls
4. NEW: `crates/runtime/src/native_runtime.rs` - C-ABI runtime functions

### JIT (Phase 2)

5. `crates/vm/src/jit_integration.rs` - Execute compiled code
6. `crates/vm/src/interpreter.rs` - Call JIT code when available
7. `crates/jit/src/baseline.rs` - Fix `emit_call_runtime()`

### Value System (Phase 3)

8. `crates/runtime/src/value.rs` - NaN-boxing implementation
9. `crates/vm/src/stack.rs` - Use PackedValue
10. `crates/vm/src/interpreter.rs` - Specialized operations

---

## Quick Win: Fastest Path to Improvement

**Immediate action (1 day):** Wire up Cranelift for simple numeric functions

```bash
# Add to CLI
roastc build --native --emit-exe fib.roast -o fib

# This should:
# 1. Parse → HIR → MIR (existing)
# 2. MIR → Cranelift IR (existing, in cranelift.rs)
# 3. Cranelift IR → Native object (existing)
# 4. Link to executable (needs work)
```

The Cranelift backend already handles:

- Integer operations
- Control flow (if/else, loops)
- Function calls
- Comparisons

What's missing:

- CLI integration
- Runtime library linking
- String/list operations

---

## Summary: Why Roast is 50x Slower Than Python

| Component                | Python                    | Roast                             | Impact |
| ------------------------ | ------------------------- | --------------------------------- | ------ |
| **Dispatch**             | Computed goto (~3 cycles) | Match statement (~15 cycles)      | 5x     |
| **Function calls**       | Frame pool, no alloc      | Arc::clone + Vec alloc every call | 10x    |
| **Value representation** | Tagged pointers           | Arc<Mutex<...>> everywhere        | 5x     |
| **Global lookup**        | Inline cache              | HashMap lookup every time         | 2x     |
| **Native code**          | Never (CPython)           | Never (but could!)                | -      |

**Combined: 50-100x slower**

The fix is NOT incremental interpreter optimization.
The fix is **COMPILING TO NATIVE CODE** which Roast already has infrastructure for but doesn't use!
