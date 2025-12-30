# Known Issues - v1.0.0-rc1

## ✅ FIXED: Method Dispatch for 3+ Args

**Fixed in**: `crates/llvm_backend/src/lib.rs` line 5222

## ✅ FIXED: Multi-Module Entry Point

**Fixed in**: `crates/cli/src/commands.rs` line 2322 - `main()` now takes priority over `__module_init__`.

---

## ✅ FIXED: Variable Swap in Loops

**Fixed in**: `crates/optimizer/src/passes.rs` - `copy_propagation` now excludes parameters as copy destinations and requires source to be immutable.

---

## ✅ FIXED: list.remove() Not Working

**Fixed in**: `crates/llvm_backend/src/lib.rs` - Changed method mapping from `roast_list_remove` (by index) to `roast_list_remove_value` (by value).

---

## ✅ FIXED: String List Display

**Fixed in**:

- `crates/runtime/src/native_full.rs` line 6740 - `roast_str` now uses `value_to_display_string` for list elements
- `crates/llvm_backend/src/lib.rs` line 3448 - Cast function now calls `roast_str` for Any→Str conversions

---

## ✅ FIXED: Dataclass Object Display

**Fixed in**: `crates/runtime/src/native_full.rs` - `roast_print` and `value_to_display_string` now handle `TypeTag::Object` to show class name and attributes.

```python
@dataclass
class Point:
    x: int
    y: int

p = Point(10, 20)
print(p)  # Now shows: Point(10, 20)
```

---

## ✅ FIXED: Float Display

**Fixed in**: `crates/runtime/src/native_full.rs` - `roast_float_to_str` (line 360) and `roast_print_float` (line 4303) now append `.0` for whole number floats.
