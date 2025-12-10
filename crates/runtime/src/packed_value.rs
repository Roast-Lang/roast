//! High-performance packed value representation using NaN-boxing.
//!
//! NaN-boxing encodes values in 64 bits:
//! - Floats: IEEE 754 double (most values)
//! - Other types: Encoded in NaN payload bits
//!
//! This eliminates allocation for primitives and enables fast type checking.

use std::mem;
use std::ptr::NonNull;

/// A 64-bit packed value using NaN-boxing.
///
/// Layout:
/// - Float: Any valid IEEE 754 double except NaN with our tag bits
/// - Tagged values use the NaN space (exponent = 0x7FF, some mantissa bits set)
///
/// Tagged format: 0x7FF[T][PPPPPPPPPPPP]
/// - T: 4-bit tag (0-15)
/// - P: 48-bit payload (pointer or immediate value)
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PackedValue(u64);

// Tag bits (in the upper bits of NaN mantissa)
const QNAN: u64 = 0x7FF8_0000_0000_0000; // Quiet NaN base
const TAG_MASK: u64 = 0x0007_0000_0000_0000; // 3 bits for tag
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF; // 48 bits for payload

// Tags (3 bits = 8 types for inline values)
const TAG_NONE: u64 = 0x0000_0000_0000_0000;
const TAG_FALSE: u64 = 0x0001_0000_0000_0000;
const TAG_TRUE: u64 = 0x0002_0000_0000_0000;
const TAG_INT: u64 = 0x0003_0000_0000_0000;  // 48-bit signed int
const TAG_PTR: u64 = 0x0004_0000_0000_0000;  // Heap pointer (boxed value)

// Special values
const VAL_NONE: u64 = QNAN | TAG_NONE;
const VAL_FALSE: u64 = QNAN | TAG_FALSE;
const VAL_TRUE: u64 = QNAN | TAG_TRUE;

impl PackedValue {
    /// Create a None value.
    #[inline(always)]
    pub const fn none() -> Self {
        Self(VAL_NONE)
    }

    /// Create a boolean value.
    #[inline(always)]
    pub const fn from_bool(b: bool) -> Self {
        Self(if b { VAL_TRUE } else { VAL_FALSE })
    }

    /// Create an integer value.
    /// Small integers (48-bit) are stored inline.
    /// Large integers are boxed.
    #[inline(always)]
    pub fn from_int(n: i64) -> Self {
        // Check if fits in 48 bits (signed)
        if n >= -(1i64 << 47) && n < (1i64 << 47) {
            // Inline small int
            let payload = (n as u64) & PAYLOAD_MASK;
            Self(QNAN | TAG_INT | payload)
        } else {
            // Box large int
            Self::box_value(BoxedValue::BigInt(n))
        }
    }

    /// Create a float value.
    #[inline(always)]
    pub fn from_float(f: f64) -> Self {
        let bits = f.to_bits();
        // Check if it's a NaN that could collide with our tags
        if (bits & QNAN) == QNAN {
            // It's a NaN - canonicalize it
            Self(f64::NAN.to_bits())
        } else {
            Self(bits)
        }
    }

    /// Create a string value (always boxed).
    #[inline]
    pub fn from_str(s: &str) -> Self {
        Self::box_value(BoxedValue::Str(s.into()))
    }

    /// Create a string value from an owned String.
    #[inline]
    pub fn from_string(s: String) -> Self {
        Self::box_value(BoxedValue::Str(s.into_boxed_str()))
    }

    /// Create a list value.
    #[inline]
    pub fn from_list(items: Vec<PackedValue>) -> Self {
        Self::box_value(BoxedValue::List(items))
    }

    /// Box a complex value.
    fn box_value(value: BoxedValue) -> Self {
        let boxed = Box::new(value);
        let ptr = Box::into_raw(boxed) as u64;
        debug_assert!(ptr & !PAYLOAD_MASK == 0, "Pointer doesn't fit in 48 bits");
        Self(QNAN | TAG_PTR | (ptr & PAYLOAD_MASK))
    }

    /// Check if this is a float.
    #[inline(always)]
    pub fn is_float(&self) -> bool {
        (self.0 & QNAN) != QNAN
    }

    /// Check if this is None.
    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0 == VAL_NONE
    }

    /// Check if this is a boolean.
    #[inline(always)]
    pub fn is_bool(&self) -> bool {
        self.0 == VAL_TRUE || self.0 == VAL_FALSE
    }

    /// Check if this is an integer.
    #[inline(always)]
    pub fn is_int(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == (QNAN | TAG_INT)
    }

    /// Check if this is a pointer to a boxed value.
    #[inline(always)]
    pub fn is_ptr(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == (QNAN | TAG_PTR)
    }

    /// Get as float.
    #[inline(always)]
    pub fn as_float(&self) -> Option<f64> {
        if self.is_float() {
            Some(f64::from_bits(self.0))
        } else {
            None
        }
    }

    /// Get as boolean.
    #[inline(always)]
    pub fn as_bool(&self) -> Option<bool> {
        match self.0 {
            VAL_TRUE => Some(true),
            VAL_FALSE => Some(false),
            _ => None,
        }
    }

    /// Get as integer.
    #[inline(always)]
    pub fn as_int(&self) -> Option<i64> {
        if self.is_int() {
            // Sign-extend from 48 bits
            let payload = self.0 & PAYLOAD_MASK;
            let shifted = (payload as i64) << 16;
            Some(shifted >> 16)
        } else if self.is_ptr() {
            // Check if it's a boxed BigInt
            if let Some(BoxedValue::BigInt(n)) = self.as_boxed() {
                Some(*n)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get the boxed value if this is a pointer.
    #[inline]
    pub fn as_boxed(&self) -> Option<&BoxedValue> {
        if self.is_ptr() {
            let ptr = (self.0 & PAYLOAD_MASK) as *const BoxedValue;
            unsafe { Some(&*ptr) }
        } else {
            None
        }
    }

    /// Get mutable boxed value.
    #[inline]
    pub fn as_boxed_mut(&mut self) -> Option<&mut BoxedValue> {
        if self.is_ptr() {
            let ptr = (self.0 & PAYLOAD_MASK) as *mut BoxedValue;
            unsafe { Some(&mut *ptr) }
        } else {
            None
        }
    }

    /// Convert to truthiness for conditionals.
    #[inline(always)]
    pub fn is_truthy(&self) -> bool {
        match self.0 {
            VAL_NONE | VAL_FALSE => false,
            VAL_TRUE => true,
            _ if self.is_int() => self.as_int().unwrap() != 0,
            _ if self.is_float() => {
                let f = self.as_float().unwrap();
                f != 0.0 && !f.is_nan()
            }
            _ => {
                // Check boxed types
                match self.as_boxed() {
                    Some(BoxedValue::Str(s)) => !s.is_empty(),
                    Some(BoxedValue::List(l)) => !l.is_empty(),
                    Some(BoxedValue::BigInt(n)) => *n != 0,
                    _ => true,
                }
            }
        }
    }

    /// Get raw bits (for debugging).
    #[inline(always)]
    pub fn bits(&self) -> u64 {
        self.0
    }
}

// Arithmetic operations - FAST paths for integers
impl PackedValue {
    /// Add two values.
    #[inline(always)]
    pub fn add(self, other: Self) -> Self {
        // Fast path: both are inline ints
        if self.is_int() && other.is_int() {
            let a = self.as_int().unwrap();
            let b = other.as_int().unwrap();
            return Self::from_int(a.wrapping_add(b));
        }
        // Fast path: both are floats
        if self.is_float() && other.is_float() {
            let a = self.as_float().unwrap();
            let b = other.as_float().unwrap();
            return Self::from_float(a + b);
        }
        // Slow path: mixed or boxed types
        self.add_slow(other)
    }

    #[cold]
    fn add_slow(self, other: Self) -> Self {
        // Handle int + float promotion
        if let (Some(a), Some(b)) = (self.as_int(), other.as_float()) {
            return Self::from_float(a as f64 + b);
        }
        if let (Some(a), Some(b)) = (self.as_float(), other.as_int()) {
            return Self::from_float(a + b as f64);
        }
        // String concatenation
        if let (Some(BoxedValue::Str(a)), Some(BoxedValue::Str(b))) = (self.as_boxed(), other.as_boxed()) {
            let mut result = String::with_capacity(a.len() + b.len());
            result.push_str(a);
            result.push_str(b);
            return Self::from_string(result);
        }
        // List concatenation
        if let (Some(BoxedValue::List(a)), Some(BoxedValue::List(b))) = (self.as_boxed(), other.as_boxed()) {
            let mut result = Vec::with_capacity(a.len() + b.len());
            result.extend_from_slice(a);
            result.extend_from_slice(b);
            return Self::from_list(result);
        }
        Self::none() // Error case
    }

    /// Subtract two values.
    #[inline(always)]
    pub fn sub(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            let a = self.as_int().unwrap();
            let b = other.as_int().unwrap();
            return Self::from_int(a.wrapping_sub(b));
        }
        if self.is_float() && other.is_float() {
            let a = self.as_float().unwrap();
            let b = other.as_float().unwrap();
            return Self::from_float(a - b);
        }
        self.sub_slow(other)
    }

    #[cold]
    fn sub_slow(self, other: Self) -> Self {
        if let (Some(a), Some(b)) = (self.as_int(), other.as_float()) {
            return Self::from_float(a as f64 - b);
        }
        if let (Some(a), Some(b)) = (self.as_float(), other.as_int()) {
            return Self::from_float(a - b as f64);
        }
        Self::none()
    }

    /// Multiply two values.
    #[inline(always)]
    pub fn mul(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            let a = self.as_int().unwrap();
            let b = other.as_int().unwrap();
            return Self::from_int(a.wrapping_mul(b));
        }
        if self.is_float() && other.is_float() {
            let a = self.as_float().unwrap();
            let b = other.as_float().unwrap();
            return Self::from_float(a * b);
        }
        self.mul_slow(other)
    }

    #[cold]
    fn mul_slow(self, other: Self) -> Self {
        if let (Some(a), Some(b)) = (self.as_int(), other.as_float()) {
            return Self::from_float(a as f64 * b);
        }
        if let (Some(a), Some(b)) = (self.as_float(), other.as_int()) {
            return Self::from_float(a * b as f64);
        }
        // String repetition
        if let (Some(BoxedValue::Str(s)), Some(n)) = (self.as_boxed(), other.as_int()) {
            if n > 0 {
                return Self::from_string(s.repeat(n as usize));
            }
            return Self::from_str("");
        }
        Self::none()
    }

    /// Divide two values.
    #[inline(always)]
    pub fn div(self, other: Self) -> Self {
        // Division always returns float in Python semantics
        let a = if self.is_int() {
            self.as_int().unwrap() as f64
        } else if self.is_float() {
            self.as_float().unwrap()
        } else {
            return Self::none();
        };

        let b = if other.is_int() {
            other.as_int().unwrap() as f64
        } else if other.is_float() {
            other.as_float().unwrap()
        } else {
            return Self::none();
        };

        Self::from_float(a / b)
    }

    /// Integer division.
    #[inline(always)]
    pub fn floor_div(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            let a = self.as_int().unwrap();
            let b = other.as_int().unwrap();
            if b == 0 {
                return Self::none(); // Division by zero
            }
            return Self::from_int(a.div_euclid(b));
        }
        self.floor_div_slow(other)
    }

    #[cold]
    fn floor_div_slow(self, other: Self) -> Self {
        let a = if self.is_int() {
            self.as_int().unwrap() as f64
        } else if self.is_float() {
            self.as_float().unwrap()
        } else {
            return Self::none();
        };

        let b = if other.is_int() {
            other.as_int().unwrap() as f64
        } else if other.is_float() {
            other.as_float().unwrap()
        } else {
            return Self::none();
        };

        Self::from_float((a / b).floor())
    }

    /// Modulo.
    #[inline(always)]
    pub fn rem(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            let a = self.as_int().unwrap();
            let b = other.as_int().unwrap();
            if b == 0 {
                return Self::none();
            }
            return Self::from_int(a.rem_euclid(b));
        }
        self.rem_slow(other)
    }

    #[cold]
    fn rem_slow(self, other: Self) -> Self {
        let a = if self.is_int() {
            self.as_int().unwrap() as f64
        } else if self.is_float() {
            self.as_float().unwrap()
        } else {
            return Self::none();
        };

        let b = if other.is_int() {
            other.as_int().unwrap() as f64
        } else if other.is_float() {
            other.as_float().unwrap()
        } else {
            return Self::none();
        };

        Self::from_float(a.rem_euclid(b))
    }

    /// Less than comparison.
    #[inline(always)]
    pub fn lt(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            return Self::from_bool(self.as_int().unwrap() < other.as_int().unwrap());
        }
        if self.is_float() && other.is_float() {
            return Self::from_bool(self.as_float().unwrap() < other.as_float().unwrap());
        }
        self.cmp_slow(other, |a, b| a < b)
    }

    /// Less than or equal comparison.
    #[inline(always)]
    pub fn le(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            return Self::from_bool(self.as_int().unwrap() <= other.as_int().unwrap());
        }
        if self.is_float() && other.is_float() {
            return Self::from_bool(self.as_float().unwrap() <= other.as_float().unwrap());
        }
        self.cmp_slow(other, |a, b| a <= b)
    }

    /// Greater than comparison.
    #[inline(always)]
    pub fn gt(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            return Self::from_bool(self.as_int().unwrap() > other.as_int().unwrap());
        }
        if self.is_float() && other.is_float() {
            return Self::from_bool(self.as_float().unwrap() > other.as_float().unwrap());
        }
        self.cmp_slow(other, |a, b| a > b)
    }

    /// Greater than or equal comparison.
    #[inline(always)]
    pub fn ge(self, other: Self) -> Self {
        if self.is_int() && other.is_int() {
            return Self::from_bool(self.as_int().unwrap() >= other.as_int().unwrap());
        }
        if self.is_float() && other.is_float() {
            return Self::from_bool(self.as_float().unwrap() >= other.as_float().unwrap());
        }
        self.cmp_slow(other, |a, b| a >= b)
    }

    /// Equality check.
    #[inline(always)]
    pub fn eq(self, other: Self) -> Self {
        // Fast path: identical bits
        if self.0 == other.0 {
            return Self::from_bool(true);
        }
        // Fast path: both ints
        if self.is_int() && other.is_int() {
            return Self::from_bool(self.as_int().unwrap() == other.as_int().unwrap());
        }
        // Fast path: both floats
        if self.is_float() && other.is_float() {
            return Self::from_bool(self.as_float().unwrap() == other.as_float().unwrap());
        }
        self.eq_slow(other)
    }

    #[cold]
    fn eq_slow(self, other: Self) -> Self {
        // Int/float comparison
        if let (Some(a), Some(b)) = (self.as_int(), other.as_float()) {
            return Self::from_bool(a as f64 == b);
        }
        if let (Some(a), Some(b)) = (self.as_float(), other.as_int()) {
            return Self::from_bool(a == b as f64);
        }
        // Boxed value comparison
        match (self.as_boxed(), other.as_boxed()) {
            (Some(BoxedValue::Str(a)), Some(BoxedValue::Str(b))) => Self::from_bool(a == b),
            (Some(BoxedValue::BigInt(a)), Some(BoxedValue::BigInt(b))) => Self::from_bool(a == b),
            _ => Self::from_bool(false),
        }
    }

    /// Not equal check.
    #[inline(always)]
    pub fn ne(self, other: Self) -> Self {
        let eq_result = self.eq(other);
        Self::from_bool(!eq_result.as_bool().unwrap_or(false))
    }

    #[cold]
    fn cmp_slow(self, other: Self, f: fn(f64, f64) -> bool) -> Self {
        let a = if self.is_int() {
            self.as_int().unwrap() as f64
        } else if self.is_float() {
            self.as_float().unwrap()
        } else {
            return Self::from_bool(false);
        };

        let b = if other.is_int() {
            other.as_int().unwrap() as f64
        } else if other.is_float() {
            other.as_float().unwrap()
        } else {
            return Self::from_bool(false);
        };

        Self::from_bool(f(a, b))
    }

    /// Negate.
    #[inline(always)]
    pub fn neg(self) -> Self {
        if self.is_int() {
            return Self::from_int(-self.as_int().unwrap());
        }
        if self.is_float() {
            return Self::from_float(-self.as_float().unwrap());
        }
        Self::none()
    }

    /// Logical not.
    #[inline(always)]
    pub fn not(self) -> Self {
        Self::from_bool(!self.is_truthy())
    }
}

/// Heap-allocated values for types that don't fit in 64 bits.
#[derive(Clone, Debug)]
pub enum BoxedValue {
    /// Large integer (doesn't fit in 48 bits)
    BigInt(i64),
    /// String
    Str(Box<str>),
    /// List
    List(Vec<PackedValue>),
    /// Tuple (immutable)
    Tuple(Box<[PackedValue]>),
    /// Dictionary
    Dict(std::collections::HashMap<PackedValue, PackedValue>),
    /// Function pointer
    Function(FunctionPtr),
    /// Object instance
    Object(Box<ObjectInstance>),
}

/// Function pointer for callable values.
#[derive(Clone, Debug)]
pub struct FunctionPtr {
    pub name: Box<str>,
    pub arity: usize,
    /// Native function pointer (for compiled code)
    pub native_ptr: Option<*const ()>,
    /// Bytecode (for interpreted code)
    pub bytecode_id: Option<u32>,
}

/// Object instance.
#[derive(Clone, Debug)]
pub struct ObjectInstance {
    pub class_name: Box<str>,
    pub fields: std::collections::HashMap<Box<str>, PackedValue>,
}

impl Default for PackedValue {
    fn default() -> Self {
        Self::none()
    }
}

impl std::fmt::Debug for PackedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            write!(f, "None")
        } else if let Some(b) = self.as_bool() {
            write!(f, "{}", b)
        } else if let Some(i) = self.as_int() {
            write!(f, "{}", i)
        } else if let Some(fl) = self.as_float() {
            write!(f, "{}", fl)
        } else if let Some(boxed) = self.as_boxed() {
            write!(f, "{:?}", boxed)
        } else {
            write!(f, "PackedValue(0x{:016x})", self.0)
        }
    }
}

impl std::fmt::Display for PackedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            write!(f, "None")
        } else if let Some(b) = self.as_bool() {
            write!(f, "{}", if b { "True" } else { "False" })
        } else if let Some(i) = self.as_int() {
            write!(f, "{}", i)
        } else if let Some(fl) = self.as_float() {
            write!(f, "{}", fl)
        } else if let Some(boxed) = self.as_boxed() {
            match boxed {
                BoxedValue::Str(s) => write!(f, "{}", s),
                BoxedValue::BigInt(n) => write!(f, "{}", n),
                BoxedValue::List(l) => {
                    write!(f, "[")?;
                    for (i, v) in l.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", v)?;
                    }
                    write!(f, "]")
                }
                _ => write!(f, "{:?}", boxed),
            }
        } else {
            write!(f, "<value>")
        }
    }
}

// Hash implementation for use in dictionaries
impl std::hash::Hash for PackedValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for PackedValue {
    fn eq(&self, other: &Self) -> bool {
        self.eq(*other).as_bool().unwrap_or(false)
    }
}

impl Eq for PackedValue {}

// Drop implementation to free boxed values
impl Drop for PackedValue {
    fn drop(&mut self) {
        if self.is_ptr() {
            let ptr = (self.0 & PAYLOAD_MASK) as *mut BoxedValue;
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none() {
        let v = PackedValue::none();
        assert!(v.is_none());
        assert!(!v.is_truthy());
    }

    #[test]
    fn test_bool() {
        let t = PackedValue::from_bool(true);
        let f = PackedValue::from_bool(false);
        assert!(t.is_bool());
        assert!(f.is_bool());
        assert_eq!(t.as_bool(), Some(true));
        assert_eq!(f.as_bool(), Some(false));
        assert!(t.is_truthy());
        assert!(!f.is_truthy());
    }

    #[test]
    fn test_int() {
        let v = PackedValue::from_int(42);
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(42));

        let neg = PackedValue::from_int(-100);
        assert_eq!(neg.as_int(), Some(-100));

        // Test arithmetic
        let a = PackedValue::from_int(10);
        let b = PackedValue::from_int(3);
        assert_eq!(a.add(b).as_int(), Some(13));
        assert_eq!(a.sub(b).as_int(), Some(7));
        assert_eq!(a.mul(b).as_int(), Some(30));
    }

    #[test]
    fn test_float() {
        let v = PackedValue::from_float(3.14);
        assert!(v.is_float());
        assert!((v.as_float().unwrap() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn test_comparison() {
        let a = PackedValue::from_int(5);
        let b = PackedValue::from_int(10);
        assert!(a.lt(b).as_bool().unwrap());
        assert!(a.le(b).as_bool().unwrap());
        assert!(!a.gt(b).as_bool().unwrap());
        assert!(b.gt(a).as_bool().unwrap());
    }
}
