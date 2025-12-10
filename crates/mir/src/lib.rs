//! Mid-level Intermediate Representation (MIR) for Roast.
//!
//! The MIR is a control-flow graph representation suitable for
//! optimization and code generation.

pub mod build;
pub mod nodes;

pub use nodes::*;

