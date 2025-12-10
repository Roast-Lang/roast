//! Common utilities and types for the Roast compiler.
//!
//! This crate provides shared infrastructure used across all Roast compiler crates.

pub mod diagnostics;
pub mod interner;
pub mod span;
pub mod symbol;

pub use diagnostics::{Diagnostic, DiagnosticKind, DiagnosticSink};
pub use interner::Interner;
pub use span::{FileId, SourceFile, Span, Spanned};
pub use symbol::Symbol;

/// Roast compiler version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-export commonly used external types.
pub mod prelude {
    pub use crate::diagnostics::{Diagnostic, DiagnosticKind, DiagnosticSink};
    pub use crate::interner::Interner;
    pub use crate::span::{FileId, SourceFile, Span, Spanned};
    pub use crate::symbol::Symbol;
    pub use indexmap::{IndexMap, IndexSet};
    pub use rustc_hash::{FxHashMap, FxHashSet};
}

