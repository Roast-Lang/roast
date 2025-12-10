# Roast Performance Optimization Plan

## Current Problem

- **fib(35)**: Roast = 10.6s, Python = 0.6s (18x slower!)
- **Goal**: Match Rust speed (near 0.01s for fib(35))

## Root Causes Identified

### 1. **Cranelift Backend is BROKEN** (Critical)

**Location**: `crates/codegen/src/cranelift.rs:671`

```rust
// TODO: Actually call the function
let result = self.builder.ins().iconst(types::I64, 0);  // Returns 0!
```

Function calls return 0 instead of actually calling functions. This makes native compilation completely non-functional.

### 2. **Value Type is Heap-Heavy** (High Impact)

**Location**: `crates/runtime/src/value.rs`

```rust
pub enum Value {
    Str(Arc<str>),           // Every string = Arc allocation
    List(Arc<Mutex<Vec>>),   // Every list = Arc + Mutex
    Function(Arc<RoastFunction>),  // Every function ref = Arc clone
    ...
}
```

- Every function call clones `Arc<RoastFunction>` (atomic ref count = slow)
- Integer operations are fine, but any object = heap allocation

### 3. **Interpreter Dispatch Overhead** (High Impact)

**Location**: `crates/vm/src/interpreter.rs:run_fast()`

- Match-based dispatch (~15 cycles per instruction)
- Python uses computed goto (1-2 cycles per instruction)
- Every instruction = function call overhead

### 4. **Global Variable Lookups** (Medium Impact)

**Location**: `crates/vm/src/interpreter.rs:501`

```rust
let value = self.globals.get(&name).cloned()  // HashMap lookup + clone
```

- `LoadGlobal` on every recursive call to look up `fib`
- Should use inline caching or direct references

### 5. **Stack Operations Clone Values** (Medium Impact)

**Location**: `crates/vm/src/stack.rs:90`

```rust
pub fn pop_n(&mut self, n: usize) -> Result<Vec<Value>, StackError> {
    Ok(self.values.drain(start..).collect())  // Allocates Vec every call
}
```

---

## Optimization Strategy (Ordered by Impact)

### Phase 1: Fix Native Compilation (HIGHEST PRIORITY)

**Expected Speedup: 100-500x for numeric code**

1. **Implement Cranelift function calls properly**

   - File: `crates/codegen/src/cranelift.rs:663-679`
   - Need to:
     - Declare called function in module
     - Create proper `call` instruction
     - Pass arguments correctly
     - Store return value

2. **Add runtime support for native code**

   - Create C-compatible runtime functions for:
     - Memory allocation
     - String operations
     - Print/IO
   - Expose them to Cranelift via `extern` functions

3. **Wire up native compilation in CLI**
   - Add `--native` flag to `roastc run`
   - Compile hot functions to native code

### Phase 2: Optimize Value Representation

**Expected Speedup: 2-5x**

1. **Use NaN-boxing for values** (like LuaJIT)

   ```rust
   // Pack small values in 64 bits
   // Float: direct IEEE 754
   // Int: tagged pointer (48-bit int + 16-bit tag)
   // Pointer: 48-bit pointer + tag
   type PackedValue = u64;
   ```

   - Eliminates heap allocation for numbers
   - Fits in register, no indirection

2. **Use Rc instead of Arc for single-threaded code**

   - Most code is single-threaded
   - `Rc` = no atomic operations = 10x faster ref counting

3. **Intern common strings**
   - Small strings (< 24 bytes) inline in Value
   - Intern all identifier strings

### Phase 3: Faster Interpreter Dispatch

**Expected Speedup: 2-3x**

1. **Threaded code dispatch** (computed goto equivalent)

   ```rust
   // Use function pointers table
   static DISPATCH: [fn(&mut VM) -> ExecResult; 256] = [...];

   fn run_fast(&mut self) {
       loop {
           let opcode = self.fetch();
           DISPATCH[opcode as usize](self);
       }
   }
   ```

2. **Superinstructions for common patterns**

   - `LoadFast + LoadInt + Add + StoreFast` → single instruction
   - `LoadFast + LoadGlobal + Call` → `CallFastGlobal`

3. **Register-based VM** (longer term)
   - Current: stack-based (many push/pop)
   - Register: direct operand access
   - 2-3x fewer instructions

### Phase 4: Inline Caching

**Expected Speedup: 1.5-2x for method calls**

1. **Monomorphic inline cache for globals**

   ```rust
   // Cache the function pointer directly in bytecode
   struct CachedCall {
       cached_func: Option<Arc<RoastFunction>>,
       cache_version: u64,
   }
   ```

2. **Polymorphic inline cache for method calls**
   - Cache last N (class, method) pairs
   - Avoid HashMap lookup on hot paths

### Phase 5: JIT Compilation (Make it Work)

**Expected Speedup: 10-100x**

1. **Fix baseline JIT compiler**

   - File: `crates/jit/src/baseline.rs`
   - Generate simple native code for hot functions

2. **Implement proper OSR (On-Stack Replacement)**

   - Tier up while function is running
   - Don't wait for function to return

3. **Add type specialization**
   - If `fib` always receives `int`, compile int-only version
   - No type checks, no boxing

---

## Quick Wins (Apply Now)

### 1. Remove unnecessary clones

```rust
// Before
let value = frame.get_local(slot).cloned().unwrap_or(Value::None);

// After (for Copy types)
let value = match frame.get_local(slot) {
    Some(Value::Int(n)) => Value::Int(*n),
    Some(v) => v.clone(),
    None => Value::None,
};
```

### 2. Avoid HashMap for small global sets

```rust
// For modules with < 32 globals, use a Vec
enum Globals {
    Small(Vec<(String, Value)>),  // Linear scan, cache friendly
    Large(FxHashMap<String, Value>),
}
```

### 3. Pre-compute jump targets

```rust
// Store absolute addresses instead of relative offsets
// Avoid arithmetic on every jump
```

### 4. Use `#[inline(always)]` on hot paths

```rust
#[inline(always)]
fn stack_push(&mut self, value: Value) { ... }
```

---

## Implementation Order

1. **Week 1**: Fix Cranelift function calls
2. **Week 2**: Add native compilation CLI option
3. **Week 3**: Implement NaN-boxing for Value
4. **Week 4**: Threaded code dispatch
5. **Week 5**: Inline caching for globals
6. **Week 6**: Fix JIT baseline compiler

---

## Benchmark Targets

| Benchmark     | Current | Phase 1 | Phase 2 | Phase 3 | Goal    |
| ------------- | ------- | ------- | ------- | ------- | ------- |
| fib(35)       | 10.6s   | 0.1s    | 0.05s   | 0.02s   | 0.01s   |
| string concat | TBD     | -       | -       | -       | ~Python |
| list ops      | TBD     | -       | -       | -       | ~Python |

---

## Files to Modify

1. `crates/codegen/src/cranelift.rs` - Fix function calls
2. `crates/runtime/src/value.rs` - NaN-boxing
3. `crates/vm/src/interpreter.rs` - Dispatch optimization
4. `crates/vm/src/stack.rs` - Avoid allocations
5. `crates/jit/src/baseline.rs` - Fix baseline JIT
6. `crates/cli/src/main.rs` - Add --native flag
