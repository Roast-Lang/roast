# Roast Language Bugs - FIXED

## ✅ ALL BUGS FIXED

### BUG-001: `move` is incorrectly a reserved keyword
- **Severity**: Medium | **Status**: ✅ FIXED
- **Files**: `crates/parser/src/token.rs`, `crates/parser/src/parser.rs`
- **Fix**: Removed `move` from reserved keywords list and parser ownership handling.

---

### BUG-002: Float comparison with int literal not supported
- **Severity**: High | **Status**: ✅ FIXED
- **File**: `crates/typer/src/checker.rs`
- **Fix**: Added numeric type check in `check_comparison_op()` - if both are numeric, comparisons allowed.

---

### BUG-003: Float default parameters generate invalid LLVM IR
- **Severity**: High | **Status**: ✅ FIXED
- **File**: `crates/llvm_backend/src/lib.rs`
- **Fix**: Modified `generate_operand_as_i64()` to bitcast double values to i64 when passing float operands to functions that expect i64 args.

---

### BUG-004: `in` operator fails on f-string result variables
- **Severity**: Medium | **Status**: ✅ FIXED
- **File**: `crates/typer/src/checker.rs`
- **Fix**: Added `ExprKind::JoinedStr` and `ExprKind::FormattedValue` cases to `check_expr_inner()` to return `Type::Str`.

---

### BUG-005: `kitchen build` didn't link executables
- **Severity**: High | **Status**: ✅ FIXED
- **File**: `crates/kitchen/src/build.rs`
- **Fix**: Changed `is_binary_target()` to match any `.roast` file, not just `main.roast`.

---

## Test Results
```
./target/release/roastc run examples/roast_system_test/system_test.roast

  ✅ ALL 38 TESTS PASSED!
```

## Summary

| Bug | Status | Fix Location |
| --- | --- | --- |
| BUG-001 | ✅ Fixed | `token.rs`, `parser.rs` |
| BUG-002 | ✅ Fixed | `checker.rs` |
| BUG-003 | ✅ Fixed | `lib.rs` (generate_operand_as_i64) |
| BUG-004 | ✅ Fixed | `checker.rs` (JoinedStr handling) |
| BUG-005 | ✅ Fixed | `build.rs` |
