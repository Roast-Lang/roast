//! Main borrow checker implementation.

use crate::dataflow::{self, ControlFlowGraph, LivenessAnalysis, Statement, StatementRhs};
use crate::diagnostics::BorrowError;
use crate::facts::{AllFacts, FactGenerator, OutputFacts};
use crate::lifetime::{LifetimeContext, LifetimeId};
use crate::loans::{Loan, LoanId, LoanKind, LoanSet, Location};
use crate::moves::{InitStatus, MoveData, MoveError};
use crate::places::{Place, PlaceElem, PlaceId, PlaceSet};
use crate::regions::{ConstraintKind, ConstraintOrigin, RegionId, RegionInference};
use roast_common::{DiagnosticSink, Interner, Span, Symbol};
use roast_mir::{MirBody, MirStmt, MirStmtKind, MirTerminator};
use roast_typer::Type;
use rustc_hash::FxHashMap;

/// The main borrow checker.
pub struct BorrowChecker<'a> {
    /// Lifetime context.
    lifetimes: LifetimeContext,
    /// Region inference.
    regions: RegionInference,
    /// Move data.
    move_data: MoveData,
    /// Active loans.
    loans: LoanSet,
    /// Fact generator.
    facts: FactGenerator,
    /// Errors found.
    errors: Vec<BorrowError>,
    /// String interner.
    interner: &'a Interner,
    /// Diagnostic sink.
    diagnostics: &'a mut DiagnosticSink,
    /// Next loan ID.
    next_loan_id: u32,
    /// Map from place to whether it's Copy.
    copy_types: FxHashMap<PlaceId, bool>,
    /// Map from local to type.
    local_types: FxHashMap<PlaceId, Type>,
    /// Current function being checked.
    current_function: Option<Symbol>,
}

impl<'a> BorrowChecker<'a> {
    /// Creates a new borrow checker.
    pub fn new(interner: &'a Interner, diagnostics: &'a mut DiagnosticSink) -> Self {
        Self {
            lifetimes: LifetimeContext::new(),
            regions: RegionInference::new(),
            move_data: MoveData::new(),
            loans: LoanSet::new(),
            facts: FactGenerator::new(),
            errors: Vec::new(),
            interner,
            diagnostics,
            next_loan_id: 0,
            copy_types: FxHashMap::default(),
            local_types: FxHashMap::default(),
            current_function: None,
        }
    }

    /// Checks a MIR body for borrow violations.
    pub fn check_body(&mut self, body: &MirBody) -> Vec<BorrowError> {
        self.current_function = Some(body.name);
        self.errors.clear();
        self.loans = LoanSet::new();
        self.move_data = MoveData::new();

        // Initialize local types
        for local in &body.locals {
            let place_id = PlaceId(local.id);
            self.local_types.insert(place_id, local.ty.clone());
            self.copy_types.insert(place_id, self.is_copy_type(&local.ty));
        }

        // Initialize parameters as initialized
        for param in &body.params {
            let place = Place::local(PlaceId(param.local.id));
            self.move_data.record_init(&place, Location::start());
        }

        // Check each block
        for block in &body.blocks {
            self.check_block(block, body);
        }

        // Solve region constraints
        if let Err(e) = self.regions.solve() {
            self.errors.push(BorrowError::Region(e));
        }

        // Report all errors to diagnostics
        for error in &self.errors {
            self.diagnostics.report(error.to_diagnostic());
        }

        std::mem::take(&mut self.errors)
    }

    /// Checks a basic block.
    fn check_block(&mut self, block: &roast_mir::MirBlock, body: &MirBody) {
        let block_id = block.id;

        for (stmt_idx, stmt) in block.stmts.iter().enumerate() {
            let location = Location::new(block_id, stmt_idx as u32);
            self.check_statement(stmt, location);
        }

        // Check terminator - use last statement span or body span as fallback
        let term_location = Location::new(block_id, block.stmts.len() as u32);
        let fallback_span = block.stmts.last().map(|s| s.span).unwrap_or(body.span);
        self.check_terminator(&block.terminator, term_location, body, fallback_span);
    }

    /// Checks a statement.
    fn check_statement(&mut self, stmt: &MirStmt, location: Location) {
        match &stmt.kind {
            MirStmtKind::Assign { place, value } => {
                // Check for conflicts with the destination
                self.check_write_place(place, location, stmt.span);

                // Check the RValue
                self.check_rvalue(value, location, stmt.span);

                // Record initialization
                let roast_place = self.mir_place_to_place(place);
                self.move_data.record_init(&roast_place, location);

                // Kill loans to the destination
                self.loans.remove_for_place(&roast_place);
            }
            MirStmtKind::StorageLive(local) => {
                // Local comes into scope
                let place = Place::local(PlaceId(*local));
                self.move_data.record_init(&place, location);
            }
            MirStmtKind::StorageDead(local) => {
                // Local goes out of scope - check for active loans
                let place = Place::local(PlaceId(*local));
                let conflicts = self.loans.conflicts_with_move(&place);
                for loan in conflicts {
                    self.errors.push(BorrowError::BorrowTooShort {
                        place: place.clone(),
                        borrow_span: loan.span,
                        drop_span: stmt.span,
                    });
                }
                // Remove loans to this place
                self.loans.remove_for_place(&place);
            }
            MirStmtKind::Nop => {}
            MirStmtKind::TryEnd => {
                // Exception frame management - no borrow checking needed
            }
            MirStmtKind::ListAppend { list, value } => {
                // List comprehension append: check read of list and value
                self.check_read_place(&self.mir_place_to_place(list), location, stmt.span);
                self.check_operand(value, location, stmt.span);
            }
            MirStmtKind::SetAttr { object, attr: _, value } => {
                self.check_operand(object, location, stmt.span);
                self.check_operand(value, location, stmt.span);
            }
            MirStmtKind::SetAttrIndex { object, attr: _, index, value } => {
                // Indexed attribute assignment: check read of object, index, and value
                self.check_operand(object, location, stmt.span);
                self.check_operand(index, location, stmt.span);
                self.check_operand(value, location, stmt.span);
            }
        }
    }

    /// Checks a terminator.
    fn check_terminator(&mut self, term: &MirTerminator, location: Location, body: &MirBody, span: Span) {
        match term {
            MirTerminator::Return(operand) => {
                // Check that we're not returning references to locals
                // This would require knowing the return type and its regions
                if let Some(op) = operand {
                    self.check_operand(op, location, span);
                }
            }
            MirTerminator::Call {
                func,
                args,
                destination,
                ..
            } => {
                // Check function operand
                self.check_operand(func, location, span);

                // Check arguments
                for arg in args {
                    self.check_operand(arg, location, span);
                }

                // Destination is written to
                self.check_write_place(destination, location, span);
            }
            MirTerminator::Drop { place, .. } => {
                let roast_place = self.mir_place_to_place(place);

                // Check for loans
                let conflicts = self.loans.conflicts_with_move(&roast_place);
                for loan in conflicts {
                    self.errors.push(BorrowError::MoveWhileBorrowed {
                        place: roast_place.clone(),
                        borrow: crate::diagnostics::LoanInfo {
                            kind: loan.kind,
                            span: loan.span,
                        },
                        move_span: span,
                    });
                }
            }
            MirTerminator::Assert { cond, .. } => {
                self.check_operand(cond, location, span);
            }
            MirTerminator::SwitchInt { discr, .. } => {
                self.check_operand(discr, location, span);
            }
            MirTerminator::ForIter { iter: _, .. } => {
                // ForIter reads and writes to the iterator
                // TODO: proper borrow checking for for-loop iterators
            }
            MirTerminator::AsyncForIter { iter: _, .. } => {
                // AsyncForIter reads and writes to the async iterator
                // TODO: proper borrow checking for async for-loop iterators
            }
            MirTerminator::Goto(_) | MirTerminator::Unreachable => {}
            MirTerminator::TryBegin { .. } => {
                // TryBegin sets up exception handler - no borrow checking needed
            }
            MirTerminator::Raise { exc, .. } => {
                // Raise throws an exception - check operand if present
                if let Some(exc_op) = exc {
                    self.check_operand(exc_op, location, span);
                }
            }
            MirTerminator::MethodCall {
                receiver,
                args,
                destination,
                ..
            } => {
                // Check receiver and arguments
                self.check_operand(receiver, location, span);
                for arg in args {
                    self.check_operand(arg, location, span);
                }
                // Destination is written to
                self.check_write_place(destination, location, span);
            }
            MirTerminator::PythonCall {
                args,
                destination,
                ..
            } => {
                // Check arguments for Python FFI call
                for arg in args {
                    self.check_operand(arg, location, span);
                }
                // Destination is written to
                self.check_write_place(destination, location, span);
            }
        }
    }

    /// Checks an RValue.
    fn check_rvalue(&mut self, rvalue: &roast_mir::MirRvalue, location: Location, span: Span) {
        use roast_mir::MirRvalue;

        match rvalue {
            MirRvalue::Use(operand) => {
                self.check_operand(operand, location, span);
            }
            MirRvalue::Ref(place, mutable) => {
                let roast_place = self.mir_place_to_place(place);
                let kind = if *mutable {
                    LoanKind::Mutable
                } else {
                    LoanKind::Shared
                };

                // Check for conflicts
                let conflicts = self.loans.conflicts_with(kind, &roast_place);
                for existing in conflicts {
                    self.errors.push(BorrowError::conflicting_borrow(
                        roast_place.clone(),
                        kind,
                        existing.kind,
                        existing.span,
                        span,
                    ));
                }

                // Create new loan
                let loan_id = self.new_loan_id();
                let lifetime = self.lifetimes.fresh(span);
                let loan = Loan::new(loan_id, kind, roast_place.clone(), lifetime, span, location);
                self.loans.add(loan);

                // Record in facts
                self.facts.facts_mut().loan_issued(loan_id, location);
            }
            MirRvalue::BinaryOp(_, left, right) => {
                self.check_operand(left, location, span);
                self.check_operand(right, location, span);
            }
            MirRvalue::UnaryOp(_, operand) => {
                self.check_operand(operand, location, span);
            }
            MirRvalue::Aggregate(_, operands) => {
                for op in operands {
                    self.check_operand(op, location, span);
                }
            }
            MirRvalue::Len(place) => {
                let roast_place = self.mir_place_to_place(place);
                self.check_read_place(&roast_place, location, span);
            }
            MirRvalue::Cast(operand, _) => {
                self.check_operand(operand, location, span);
            }
            MirRvalue::Attr(operand, _) => {
                self.check_operand(operand, location, span);
            }
            MirRvalue::Await(operand) => {
                self.check_operand(operand, location, span);
            }
        }
    }

    /// Checks an operand.
    fn check_operand(&mut self, operand: &roast_mir::MirOperand, location: Location, span: Span) {
        use roast_mir::MirOperand;

        match operand {
            MirOperand::Copy(place) => {
                let roast_place = self.mir_place_to_place(place);
                self.check_read_place(&roast_place, location, span);
            }
            MirOperand::Move(place) => {
                let roast_place = self.mir_place_to_place(place);
                self.check_move_place(&roast_place, location, span);
            }
            MirOperand::Constant(_) => {
                // Constants don't involve borrows
            }
            MirOperand::Global(_) => {
                // Global references don't involve borrows
            }
        }
    }

    /// Checks reading from a place.
    fn check_read_place(&mut self, place: &Place, location: Location, span: Span) {
        // Skip initialization check for field projections (like self.field)
        // Class field access uses borrowing semantics, not ownership transfer
        if place.is_local() {
            // Only check initialization for local variables, not field accesses
            let init_status = self.move_data.is_initialized(place);
            if matches!(init_status, InitStatus::Uninitialized | InitStatus::MaybeUninitialized) {
                // Find where the move happened
                let moves = self.move_data.moves_of(place);
                let moved_at = moves.last().map(|m| m.span).unwrap_or(span);

                self.errors.push(BorrowError::use_after_move(
                    place.clone(),
                    moved_at,
                    span,
                ));
            }
        }

        // Check for conflicting mutable borrows
        let conflicts: Vec<_> = self.loans.iter()
            .filter(|loan| loan.conflicts_with_read(place))
            .collect();

        for loan in conflicts {
            self.errors.push(BorrowError::use_while_borrowed(
                place.clone(),
                loan.kind,
                loan.span,
                span,
            ));
        }
    }

    /// Checks writing to a place.
    fn check_write_place(&mut self, mir_place: &roast_mir::MirPlace, location: Location, span: Span) {
        let place = self.mir_place_to_place(mir_place);

        // Check for conflicting borrows
        let conflicts: Vec<_> = self.loans.iter()
            .filter(|loan| loan.conflicts_with_write(&place))
            .collect();

        for loan in conflicts {
            self.errors.push(BorrowError::AssignWhileBorrowed {
                place: place.clone(),
                borrow: crate::diagnostics::LoanInfo {
                    kind: loan.kind,
                    span: loan.span,
                },
                assign_span: span,
            });
        }
    }

    /// Checks moving from a place.
    fn check_move_place(&mut self, place: &Place, location: Location, span: Span) {
        // Skip move tracking for field projections (like self.field)
        // Class field access uses borrowing semantics, not ownership transfer
        // This allows patterns like: for item in self.items: ...
        if !place.is_local() {
            // Just do a read check for field accesses
            self.check_read_place(place, location, span);
            return;
        }

        // Check if already moved
        let init_status = self.move_data.is_initialized(place);
        if matches!(init_status, InitStatus::Uninitialized | InitStatus::MaybeUninitialized) {
            // Find where the move happened
            let moves = self.move_data.moves_of(place);
            let moved_at = moves.last().map(|m| m.span).unwrap_or(span);

            self.errors.push(BorrowError::use_after_move(
                place.clone(),
                moved_at,
                span,
            ));
            return;
        }

        // Check for conflicting borrows
        let conflicts = self.loans.conflicts_with_move(place);
        for loan in conflicts {
            self.errors.push(BorrowError::move_while_borrowed(
                place.clone(),
                loan.kind,
                loan.span,
                span,
            ));
        }

        // Record the move
        let is_copy = self.is_place_copy(place);
        self.move_data.record_move(place, location, span, is_copy);
    }

    /// Converts a MIR place to our place representation.
    fn mir_place_to_place(&self, mir_place: &roast_mir::MirPlace) -> Place {
        let mut place = Place::local(PlaceId(mir_place.local));
        for proj in &mir_place.projections {
            match proj {
                roast_mir::MirProjection::Field(idx) => {
                    place = place.field(*idx, None);
                }
                roast_mir::MirProjection::Index(local) => {
                    place = place.index(PlaceId(*local));
                }
                roast_mir::MirProjection::Slice { lower, .. } => {
                    // Treat slice like an index for borrow checking purposes
                    place = place.index(PlaceId(*lower));
                }
                roast_mir::MirProjection::Deref => {
                    place = place.deref();
                }
            }
        }
        place
    }

    /// Generates a new loan ID.
    fn new_loan_id(&mut self) -> LoanId {
        let id = LoanId(self.next_loan_id);
        self.next_loan_id += 1;
        id
    }

    /// Checks if a type is Copy.
    fn is_copy_type(&self, ty: &Type) -> bool {
        match ty {
            // Primitive types are Copy
            Type::Bool | Type::Int | Type::Int8 | Type::Int16 | Type::Int32 |
            Type::Int64 | Type::Int128 | Type::UInt | Type::UInt8 | Type::UInt16 |
            Type::UInt32 | Type::UInt64 | Type::UInt128 | Type::Float |
            Type::Float32 | Type::Float64 => true,

            // References are Copy
            Type::Ref { .. } => true,

            // Tuples are Copy if all elements are Copy
            Type::Tuple(elems) => elems.iter().all(|e| self.is_copy_type(e)),

            // Everything else is not Copy by default
            _ => false,
        }
    }

    /// Checks if a place is Copy.
    fn is_place_copy(&self, place: &Place) -> bool {
        if let Some(&is_copy) = self.copy_types.get(&place.local) {
            is_copy
        } else {
            false
        }
    }

    /// Returns the errors found.
    pub fn errors(&self) -> &[BorrowError] {
        &self.errors
    }

    /// Returns whether any errors were found.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Configuration for the borrow checker.
#[derive(Clone, Debug, Default)]
pub struct BorrowCheckConfig {
    /// Whether to enable strict ownership checking.
    pub strict_ownership: bool,
    /// Whether to enable two-phase borrows.
    pub two_phase_borrows: bool,
    /// Whether to enable non-lexical lifetimes.
    pub nll: bool,
}

impl BorrowCheckConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn strict() -> Self {
        Self {
            strict_ownership: true,
            two_phase_borrows: true,
            nll: true,
        }
    }

    pub fn permissive() -> Self {
        Self {
            strict_ownership: false,
            two_phase_borrows: true,
            nll: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would go here
}
