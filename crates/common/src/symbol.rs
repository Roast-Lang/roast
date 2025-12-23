//! Interned symbol type for identifiers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::NonZeroU32;

/// An interned symbol representing an identifier or string.
///
/// Symbols are cheap to copy and compare, making them ideal for identifiers
/// that appear frequently throughout the compiler.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol(NonZeroU32);

impl Symbol {
    /// Creates a new symbol from a raw index.
    ///
    /// # Safety
    /// The index must be valid and obtained from the interner.
    #[inline]
    pub const fn from_raw(index: u32) -> Self {
        // SAFETY: we add 1 to ensure non-zero
        unsafe { Self(NonZeroU32::new_unchecked(index + 1)) }
    }

    /// Returns the raw index of this symbol.
    #[inline]
    pub const fn as_raw(self) -> u32 {
        self.0.get() - 1
    }

    /// Placeholder for unresolved symbols.
    pub const UNRESOLVED: Symbol = Symbol::from_raw(u32::MAX - 1);

    /// Intrinsic: __make_iter__ - creates an iterator from an iterable
    pub const MAKE_ITER: Symbol = Symbol::from_raw(u32::MAX - 2);
    
    /// Intrinsic: __make_aiter__ - creates an async iterator from an async iterable
    pub const MAKE_AITER: Symbol = Symbol::from_raw(u32::MAX - 3);
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Symbol({})", self.as_raw())
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<sym:{}>", self.as_raw())
    }
}
