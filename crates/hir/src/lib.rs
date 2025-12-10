//! High-level Intermediate Representation (HIR) for Roast.
//!
//! The HIR is a typed, desugared representation of the AST. It serves as
//! the input for later compilation phases.

pub mod build;
pub mod nodes;

pub use nodes::*;

use roast_common::prelude::*;

/// Prelude for the HIR crate.
pub mod prelude {
    pub use crate::nodes::*;
}

