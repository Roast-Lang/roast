//! Loan tracking for borrow checking.
//!
//! A loan represents an active borrow of a place.

use crate::lifetime::LifetimeId;
use crate::places::Place;
use roast_common::Span;
use std::fmt;

/// Unique identifier for a loan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LoanId(pub u32);

impl fmt::Display for LoanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "L{}", self.0)
    }
}

/// The kind of a loan (borrow).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoanKind {
    /// Shared (immutable) borrow: `&x`
    Shared,
    /// Mutable borrow: `&mut x`
    Mutable,
    /// Two-phase borrow (allows nested calls like `v.push(v.len())`)
    TwoPhase,
}

impl LoanKind {
    pub fn is_mutable(&self) -> bool {
        matches!(self, LoanKind::Mutable | LoanKind::TwoPhase)
    }

    pub fn is_shared(&self) -> bool {
        matches!(self, LoanKind::Shared)
    }
}

impl fmt::Display for LoanKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoanKind::Shared => write!(f, "shared"),
            LoanKind::Mutable => write!(f, "mutable"),
            LoanKind::TwoPhase => write!(f, "two-phase"),
        }
    }
}

/// A loan (active borrow) of a place.
#[derive(Clone, Debug)]
pub struct Loan {
    /// Unique identifier.
    pub id: LoanId,
    /// The kind of borrow.
    pub kind: LoanKind,
    /// The borrowed place.
    pub place: Place,
    /// The lifetime of the borrow.
    pub lifetime: LifetimeId,
    /// Where the borrow was created.
    pub span: Span,
    /// The location in the MIR where the loan was issued.
    pub issued_at: Location,
}

impl Loan {
    pub fn new(
        id: LoanId,
        kind: LoanKind,
        place: Place,
        lifetime: LifetimeId,
        span: Span,
        issued_at: Location,
    ) -> Self {
        Self {
            id,
            kind,
            place,
            lifetime,
            span,
            issued_at,
        }
    }

    /// Returns true if this loan conflicts with another operation.
    pub fn conflicts_with(&self, other_kind: LoanKind, other_place: &Place) -> bool {
        // Loans only conflict if the places overlap
        if !self.place.may_overlap(other_place) {
            return false;
        }

        // Two shared borrows don't conflict
        if self.kind.is_shared() && other_kind.is_shared() {
            return false;
        }

        // All other combinations conflict
        true
    }

    /// Returns true if this loan conflicts with a write to a place.
    pub fn conflicts_with_write(&self, place: &Place) -> bool {
        self.place.may_overlap(place)
    }

    /// Returns true if this loan conflicts with a read from a place.
    pub fn conflicts_with_read(&self, place: &Place) -> bool {
        // Only mutable borrows conflict with reads
        self.kind.is_mutable() && self.place.may_overlap(place)
    }

    /// Returns true if this loan conflicts with a move of a place.
    pub fn conflicts_with_move(&self, place: &Place) -> bool {
        self.place.may_overlap(place)
    }
}

impl fmt::Display for Loan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} borrow of {}", self.id, self.kind, self.place)
    }
}

/// A location in the MIR (block + statement index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    pub block: u32,
    pub statement: u32,
}

impl Location {
    pub fn new(block: u32, statement: u32) -> Self {
        Self { block, statement }
    }

    pub fn start() -> Self {
        Self {
            block: 0,
            statement: 0,
        }
    }

    /// Returns the next location in the same block.
    pub fn next(&self) -> Self {
        Self {
            block: self.block,
            statement: self.statement + 1,
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}[{}]", self.block, self.statement)
    }
}

/// A set of active loans.
#[derive(Clone, Debug, Default)]
pub struct LoanSet {
    loans: Vec<Loan>,
}

impl LoanSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a loan to the set.
    pub fn add(&mut self, loan: Loan) {
        self.loans.push(loan);
    }

    /// Removes loans that have expired at the given location.
    pub fn expire_at(&mut self, _location: Location, _lifetime_ctx: &crate::lifetime::LifetimeContext) {
        // In a full implementation, we'd check if the lifetime is still active
        // For now, we keep all loans until explicitly removed
    }

    /// Removes a specific loan.
    pub fn remove(&mut self, id: LoanId) {
        self.loans.retain(|l| l.id != id);
    }

    /// Removes all loans for a given place.
    pub fn remove_for_place(&mut self, place: &Place) {
        self.loans.retain(|l| !l.place.may_overlap(place));
    }

    /// Returns all active loans.
    pub fn iter(&self) -> impl Iterator<Item = &Loan> {
        self.loans.iter()
    }

    /// Returns all loans that conflict with a new borrow.
    pub fn conflicts_with(&self, kind: LoanKind, place: &Place) -> Vec<&Loan> {
        self.loans
            .iter()
            .filter(|l| l.conflicts_with(kind, place))
            .collect()
    }

    /// Returns all loans that conflict with a write.
    pub fn conflicts_with_write(&self, place: &Place) -> Vec<&Loan> {
        self.loans
            .iter()
            .filter(|l| l.conflicts_with_write(place))
            .collect()
    }

    /// Returns all loans that conflict with a move.
    pub fn conflicts_with_move(&self, place: &Place) -> Vec<&Loan> {
        self.loans
            .iter()
            .filter(|l| l.conflicts_with_move(place))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.loans.is_empty()
    }

    pub fn len(&self) -> usize {
        self.loans.len()
    }
}

