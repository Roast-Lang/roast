//! String interner for symbols.

use crate::Symbol;
use rustc_hash::FxHashMap;
use std::sync::RwLock;

/// A thread-safe string interner.
///
/// The interner stores strings and returns unique symbols that can be used
/// to efficiently compare and store identifiers.
pub struct Interner {
    inner: RwLock<InternerInner>,
}

struct InternerInner {
    strings: Vec<Box<str>>,
    map: FxHashMap<&'static str, Symbol>,
}

impl Interner {
    /// Creates a new interner with common keywords pre-interned.
    pub fn new() -> Self {
        let mut interner = Self {
            inner: RwLock::new(InternerInner {
                strings: Vec::new(),
                map: FxHashMap::default(),
            }),
        };

        // Pre-intern common keywords
        for kw in KEYWORDS {
            interner.intern(kw);
        }

        interner
    }

    /// Interns a string, returning its symbol.
    pub fn intern(&self, s: &str) -> Symbol {
        // Fast path: check if already interned
        {
            let inner = self.inner.read().unwrap();
            if let Some(&sym) = inner.map.get(s) {
                return sym;
            }
        }

        // Slow path: insert new string
        let mut inner = self.inner.write().unwrap();
        
        // Double-check after acquiring write lock
        if let Some(&sym) = inner.map.get(s) {
            return sym;
        }

        let idx = inner.strings.len() as u32;
        let sym = Symbol::from_raw(idx);
        let boxed: Box<str> = s.into();
        
        // SAFETY: The string lives as long as the interner, and we never remove strings.
        let static_ref: &'static str = unsafe { std::mem::transmute(boxed.as_ref()) };
        
        inner.strings.push(boxed);
        inner.map.insert(static_ref, sym);
        sym
    }

    /// Resolves a symbol back to its string.
    pub fn resolve(&self, sym: Symbol) -> Option<&str> {
        let inner = self.inner.read().unwrap();
        inner.strings.get(sym.as_raw() as usize).map(|s| {
            // SAFETY: We return a reference that lives as long as &self,
            // and strings are never removed from the interner.
            unsafe { std::mem::transmute::<&str, &str>(s.as_ref()) }
        })
    }

    /// Returns the number of interned strings.
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().strings.len()
    }

    /// Returns true if no strings have been interned.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Interner {
    fn default() -> Self {
        Self::new()
    }
}

/// Common keywords to pre-intern.
const KEYWORDS: &[&str] = &[
    // Python keywords
    "False", "None", "True", "and", "as", "assert", "async", "await",
    "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
    "while", "with", "yield",
    // Roast-specific keywords
    "mut", "imm", "borrow", "move", "ref", "own",
    // Type keywords  
    "int", "float", "str", "bool", "bytes", "list", "dict", "set", "tuple",
    "List", "Dict", "Set", "Tuple", "Optional", "Union", "Any", "Callable",
    // Built-in names
    "self", "cls", "__init__", "__new__", "__del__", "__repr__", "__str__",
    "__len__", "__iter__", "__next__", "__getitem__", "__setitem__",
    "__contains__", "__add__", "__sub__", "__mul__", "__div__", "__mod__",
    "__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__",
    "__hash__", "__bool__", "__call__", "__enter__", "__exit__",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_resolve() {
        let interner = Interner::new();
        let sym = interner.intern("hello");
        assert_eq!(interner.resolve(sym), Some("hello"));
    }

    #[test]
    fn test_intern_same_string() {
        let interner = Interner::new();
        let sym1 = interner.intern("world");
        let sym2 = interner.intern("world");
        assert_eq!(sym1, sym2);
    }

    #[test]
    fn test_different_strings() {
        let interner = Interner::new();
        let sym1 = interner.intern("foo");
        let sym2 = interner.intern("bar");
        assert_ne!(sym1, sym2);
    }
}

