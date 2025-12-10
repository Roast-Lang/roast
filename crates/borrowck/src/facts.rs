//! Borrow checker facts in Polonius-style.
//!
//! This module provides the fact generation for dataflow-based borrow checking,
//! inspired by Polonius (the next-gen Rust borrow checker).

use crate::loans::{LoanId, Location};
use crate::places::PlaceId;
use crate::regions::RegionId;
use rustc_hash::FxHashSet;

/// All facts generated during borrow checking.
#[derive(Clone, Debug, Default)]
pub struct AllFacts {
    // === Loan facts ===
    /// (Loan, Point) - loan is issued at point
    pub loan_issued_at: FxHashSet<(LoanId, Location)>,
    /// (Loan, Point) - loan is killed at point (invalidated)
    pub loan_killed_at: FxHashSet<(LoanId, Location)>,
    /// (Loan, Point) - loan is live at point (used later)
    pub loan_live_at: FxHashSet<(LoanId, Location)>,

    // === Region facts ===
    /// (Region, Point) - region is live at point
    pub region_live_at: FxHashSet<(RegionId, Location)>,
    /// (Region1, Region2) - region1 outlives region2
    pub outlives: FxHashSet<(RegionId, RegionId)>,

    // === Path facts ===
    /// (Path, Point) - path is initialized at point
    pub path_initialized_at: FxHashSet<(PlaceId, Location)>,
    /// (Path, Point) - path is moved at point
    pub path_moved_at: FxHashSet<(PlaceId, Location)>,
    /// (Path, Point) - path is accessed at point
    pub path_accessed_at: FxHashSet<(PlaceId, Location)>,

    // === Control flow facts ===
    /// (Point, Point) - CFG edge from point1 to point2
    pub cfg_edge: FxHashSet<(Location, Location)>,
    /// Point is a function entry
    pub entry_point: Option<Location>,
    /// Points that are function exits
    pub exit_points: FxHashSet<Location>,

    // === Origin facts (for error reporting) ===
    /// (Loan, Region) - loan is associated with region
    pub loan_region: FxHashSet<(LoanId, RegionId)>,
    /// (Place, Region) - place has type with region
    pub place_region: FxHashSet<(PlaceId, RegionId)>,
}

impl AllFacts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that a loan was issued at a location.
    pub fn loan_issued(&mut self, loan: LoanId, location: Location) {
        self.loan_issued_at.insert((loan, location));
    }

    /// Records that a loan was killed at a location.
    pub fn loan_killed(&mut self, loan: LoanId, location: Location) {
        self.loan_killed_at.insert((loan, location));
    }

    /// Records that a region is live at a location.
    pub fn region_live(&mut self, region: RegionId, location: Location) {
        self.region_live_at.insert((region, location));
    }

    /// Records an outlives relationship.
    pub fn add_outlives(&mut self, longer: RegionId, shorter: RegionId) {
        self.outlives.insert((longer, shorter));
    }

    /// Records that a path was initialized.
    pub fn path_initialized(&mut self, path: PlaceId, location: Location) {
        self.path_initialized_at.insert((path, location));
    }

    /// Records that a path was moved.
    pub fn path_moved(&mut self, path: PlaceId, location: Location) {
        self.path_moved_at.insert((path, location));
    }

    /// Records that a path was accessed.
    pub fn path_accessed(&mut self, path: PlaceId, location: Location) {
        self.path_accessed_at.insert((path, location));
    }

    /// Records a CFG edge.
    pub fn add_cfg_edge(&mut self, from: Location, to: Location) {
        self.cfg_edge.insert((from, to));
    }

    /// Sets the entry point.
    pub fn set_entry(&mut self, location: Location) {
        self.entry_point = Some(location);
    }

    /// Adds an exit point.
    pub fn add_exit(&mut self, location: Location) {
        self.exit_points.insert(location);
    }

    /// Associates a loan with a region.
    pub fn associate_loan_region(&mut self, loan: LoanId, region: RegionId) {
        self.loan_region.insert((loan, region));
    }

    /// Associates a place with a region.
    pub fn associate_place_region(&mut self, place: PlaceId, region: RegionId) {
        self.place_region.insert((place, region));
    }
}

/// Output facts computed by the borrow checker.
#[derive(Clone, Debug, Default)]
pub struct OutputFacts {
    /// (Loan, Point) - loan is in scope at point
    pub loan_in_scope: FxHashSet<(LoanId, Location)>,
    /// (Point) - point has an error
    pub errors: FxHashSet<Location>,
    /// (Loan, Point) - loan conflicts at point
    pub loan_conflicts: FxHashSet<(LoanId, Location)>,
}

impl OutputFacts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks if a loan is in scope at a location.
    pub fn is_loan_in_scope(&self, loan: LoanId, location: Location) -> bool {
        self.loan_in_scope.contains(&(loan, location))
    }

    /// Checks if there's an error at a location.
    pub fn has_error(&self, location: Location) -> bool {
        self.errors.contains(&location)
    }

    /// Gets all loan conflicts at a location.
    pub fn conflicts_at(&self, location: Location) -> Vec<LoanId> {
        self.loan_conflicts
            .iter()
            .filter(|(_, loc)| *loc == location)
            .map(|(loan, _)| *loan)
            .collect()
    }
}

/// Fact generation from MIR.
pub struct FactGenerator {
    facts: AllFacts,
    next_loan: u32,
}

impl FactGenerator {
    pub fn new() -> Self {
        Self {
            facts: AllFacts::new(),
            next_loan: 0,
        }
    }

    /// Generates a new loan ID.
    pub fn new_loan(&mut self) -> LoanId {
        let id = LoanId(self.next_loan);
        self.next_loan += 1;
        id
    }

    /// Returns the generated facts.
    pub fn into_facts(self) -> AllFacts {
        self.facts
    }

    /// Returns a mutable reference to the facts.
    pub fn facts_mut(&mut self) -> &mut AllFacts {
        &mut self.facts
    }
}

impl Default for FactGenerator {
    fn default() -> Self {
        Self::new()
    }
}

