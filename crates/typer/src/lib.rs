//! Type system for the Roast language.
//!
//! This crate provides type checking, type inference, and the type representation
//! used throughout the compiler.

pub mod builtins;
pub mod context;
pub mod infer;
pub mod types;
pub mod checker;
pub mod constraints;
pub mod substitution;
pub mod ownership;
pub mod protocols;
pub mod exhaustiveness;
pub mod mro;
pub mod variance;
pub mod concurrency;
pub mod lints;

pub use types::*;
pub use mro::{compute_mro, get_next_in_mro};
pub use context::TypeContext;
pub use checker::TypeChecker;
pub use ownership::{OwnershipAnalyzer, OwnershipInfo, OwnershipMode, TypeTrait};
pub use protocols::{Protocol, ProtocolRegistry, ProtocolMethod, ProtocolImpl, DerivableProtocol};
pub use variance::{Variance, TypeParam, ConstGeneric, WhereClause, TypeBounds};
pub use concurrency::{ConcurrencyChecker, ConcurrencyError, ConcurrencyErrorKind};
pub use lints::{LintChecker, MutableDefaultKind};

use roast_common::prelude::*;

/// Prelude for the typer crate.
pub mod prelude {
    pub use crate::types::*;
    pub use crate::context::TypeContext;
    pub use crate::checker::TypeChecker;
    pub use crate::ownership::{OwnershipAnalyzer, OwnershipMode, TypeTrait};
    pub use crate::protocols::{Protocol, ProtocolRegistry, ProtocolMethod};
    pub use crate::variance::{Variance, TypeParam, ConstGeneric, WhereClause};
    pub use crate::concurrency::{ConcurrencyChecker, ConcurrencyError};
}
