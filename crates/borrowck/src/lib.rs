//! Borrow checker and ownership analysis for Roast.
//!
//! This crate implements Roast's optional ownership system, providing:
//! - Lifetime analysis and tracking
//! - Borrow checking (mutable and immutable borrows)
//! - Move/copy semantics
//! - Ownership transfer verification
//!
//! Unlike Rust, Roast's ownership system is opt-in. Code without ownership
//! annotations falls back to reference counting.

pub mod dataflow;
pub mod lifetime;
pub mod loans;
pub mod moves;
pub mod places;
pub mod regions;
pub mod checker;
pub mod diagnostics;
pub mod facts;

#[cfg(test)]
mod tests;

pub use checker::BorrowChecker;
pub use diagnostics::BorrowError;
pub use lifetime::{Lifetime, LifetimeId};
pub use loans::{Loan, LoanKind};
pub use moves::{MoveData, MovePath};
pub use places::{Place, PlaceElem};
pub use regions::{Region, RegionId};

use roast_common::prelude::*;

/// Prelude for the borrowck crate.
pub mod prelude {
    pub use crate::checker::BorrowChecker;
    pub use crate::diagnostics::BorrowError;
    pub use crate::lifetime::{Lifetime, LifetimeId};
    pub use crate::loans::{Loan, LoanKind};
    pub use crate::places::{Place, PlaceElem};
}

