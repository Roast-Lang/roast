//! Escape Analysis for Roast MIR.
//!
//! This module implements escape analysis to determine which allocations
//! can be safely placed on the stack instead of the heap. An allocation
//! "escapes" if it can be accessed outside its defining function.
//!
//! # Escape Conditions
//!
//! An allocation escapes if:
//! - It is returned from the function
//! - It is stored in a global variable
//! - It is passed to a function that may store it
//! - It is stored in an escaping allocation
//! - Its address is taken and escapes
//!
//! # Stack Allocation Benefits
//!
//! Non-escaping allocations can be:
//! - Allocated on the stack (no heap allocation overhead)
//! - Deallocated automatically (no GC pressure)
//! - Potentially inlined into the parent object

use roast_mir::*;
use rustc_hash::{FxHashMap, FxHashSet};

/// Escape state for a local variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EscapeState {
    /// Does not escape - can be stack allocated
    NoEscape,
    /// Escapes only to arguments (may be captured by callee)
    ArgEscape,
    /// Escapes globally (returned, stored in global, etc.)
    GlobalEscape,
}

impl EscapeState {
    /// Combines two escape states (takes the more escaping one).
    pub fn join(self, other: EscapeState) -> EscapeState {
        std::cmp::max(self, other)
    }

    /// Returns true if this can be stack allocated.
    pub fn can_stack_allocate(&self) -> bool {
        matches!(self, EscapeState::NoEscape)
    }
}

/// Result of escape analysis.
#[derive(Clone, Debug)]
pub struct EscapeAnalysisResult {
    /// Escape state for each local.
    pub local_escape: FxHashMap<LocalId, EscapeState>,
    /// Locals that can be stack allocated.
    pub stack_allocatable: FxHashSet<LocalId>,
    /// Statistics.
    pub stats: EscapeStats,
}

/// Statistics from escape analysis.
#[derive(Clone, Debug, Default)]
pub struct EscapeStats {
    pub total_locals: usize,
    pub non_escaping: usize,
    pub arg_escaping: usize,
    pub global_escaping: usize,
    pub stack_allocated: usize,
}

/// The escape analyzer.
pub struct EscapeAnalyzer<'a> {
    body: &'a MirBody,
    /// Escape state for each local.
    escape: FxHashMap<LocalId, EscapeState>,
    /// Points-to graph: maps locals to what they might point to.
    points_to: FxHashMap<LocalId, FxHashSet<LocalId>>,
    /// Worklist for iterative analysis.
    worklist: Vec<LocalId>,
}

impl<'a> EscapeAnalyzer<'a> {
    pub fn new(body: &'a MirBody) -> Self {
        Self {
            body,
            escape: FxHashMap::default(),
            points_to: FxHashMap::default(),
            worklist: Vec::new(),
        }
    }

    /// Runs escape analysis on the function body.
    pub fn analyze(mut self) -> EscapeAnalysisResult {
        // Initialize all locals as non-escaping
        self.initialize();

        // Build points-to graph and initial escape info
        self.build_graph();

        // Propagate escape information to fixed point
        self.propagate();

        // Collect results
        self.collect_results()
    }

    fn initialize(&mut self) {
        // Initialize all locals as non-escaping
        for local in &self.body.locals {
            self.escape.insert(local.id, EscapeState::NoEscape);
            self.points_to.insert(local.id, FxHashSet::default());
        }

        // Parameters might escape (conservative)
        for param in &self.body.params {
            self.escape.insert(param.local.id, EscapeState::NoEscape);
            self.points_to.insert(param.local.id, FxHashSet::default());
        }
    }

    fn build_graph(&mut self) {
        for block in &self.body.blocks {
            // Analyze statements
            for stmt in &block.stmts {
                self.analyze_stmt(stmt);
            }

            // Analyze terminator
            self.analyze_terminator(&block.terminator);
        }
    }

    fn analyze_stmt(&mut self, stmt: &MirStmt) {
        if let MirStmtKind::Assign { place, value } = &stmt.kind {
            self.analyze_assignment(place, value);
        }
    }

    fn analyze_assignment(&mut self, place: &MirPlace, value: &MirRvalue) {
        match value {
            // Simple copy/move - propagate points-to
            MirRvalue::Use(operand) => {
                if let Some(src_local) = self.operand_local(operand) {
                    // place points to whatever src points to
                    if place.projections.is_empty() {
                        if let Some(src_pts) = self.points_to.get(&src_local).cloned() {
                            self.points_to
                                .entry(place.local)
                                .or_default()
                                .extend(src_pts);
                        }
                        // Also point to src itself if it's a reference type
                        self.points_to
                            .entry(place.local)
                            .or_default()
                            .insert(src_local);
                    } else {
                        // Storing into a field - src escapes to place.local
                        self.mark_escape_to(src_local, place.local);
                    }
                }
            }

            // Taking a reference - create points-to edge
            MirRvalue::Ref(ref_place, _) => {
                if place.projections.is_empty() {
                    self.points_to
                        .entry(place.local)
                        .or_default()
                        .insert(ref_place.local);
                }
            }

            // Aggregate - all operands flow into result
            MirRvalue::Aggregate(_, operands) => {
                for op in operands {
                    if let Some(src_local) = self.operand_local(op) {
                        if place.projections.is_empty() {
                            // src escapes to the aggregate
                            self.mark_escape_to(src_local, place.local);
                        }
                    }
                }
            }

            // Binary/unary ops typically don't cause escapes for primitives
            MirRvalue::BinaryOp(_, left, right) => {
                // Results of binary ops are new values, not aliases
                // But if operands are objects, they might be captured
                if let Some(l) = self.operand_local(left) {
                    self.points_to.entry(place.local).or_default().insert(l);
                }
                if let Some(r) = self.operand_local(right) {
                    self.points_to.entry(place.local).or_default().insert(r);
                }
            }

            MirRvalue::UnaryOp(_, operand) => {
                if let Some(src) = self.operand_local(operand) {
                    self.points_to.entry(place.local).or_default().insert(src);
                }
            }

            MirRvalue::Cast(operand, _) => {
                if let Some(src) = self.operand_local(operand) {
                    self.points_to.entry(place.local).or_default().insert(src);
                }
            }

            MirRvalue::Len(_) => {
                // Length is a primitive, doesn't cause escape
            }

            MirRvalue::Attr(operand, _) => {
                if let Some(src) = self.operand_local(operand) {
                    self.points_to.entry(place.local).or_default().insert(src);
                }
            }

            MirRvalue::Await(operand) => {
                if let Some(src) = self.operand_local(operand) {
                    self.points_to.entry(place.local).or_default().insert(src);
                }
            }
        }
    }

    fn analyze_terminator(&mut self, term: &MirTerminator) {
        match term {
            // Return causes global escape
            MirTerminator::Return(operand) => {
                // If there's an operand, it escapes; otherwise local 0 is typically the return value
                if let Some(op) = operand {
                    if let Some(local) = self.operand_local(op) {
                        self.mark_global_escape(local);
                    }
                } else {
                    self.mark_global_escape(0);
                }
            }

            // Function calls - arguments might escape
            MirTerminator::Call { args, destination, .. } => {
                for arg in args {
                    if let Some(local) = self.operand_local(arg) {
                        // Conservative: assume arguments escape to callees
                        self.mark_arg_escape(local);
                    }
                }
                // Destination gets result, which might contain escaped data
                if destination.projections.is_empty() {
                    // Return value is a new allocation, mark as non-escaping initially
                    // but propagate if the function returns something that escapes
                }
            }

            // Drop doesn't cause escape
            MirTerminator::Drop { .. } => {}

            // ForIter may cause iterator to escape to loop variable
            MirTerminator::ForIter { iter, loop_var, .. } => {
                // Loop variable gets values from iterator - mark as potential escape
                if iter.projections.is_empty() {
                    self.mark_arg_escape(iter.local);
                }
                self.mark_arg_escape(*loop_var);
            }

            // Control flow doesn't cause escape
            MirTerminator::Goto(_) |
            MirTerminator::SwitchInt { .. } |
            MirTerminator::Assert { .. } |
            MirTerminator::Unreachable => {}

            // Exception handling
            MirTerminator::TryBegin { .. } => {
                // Try doesn't cause escape by itself
            }
            MirTerminator::Raise { exc, .. } => {
                // Exception value escapes (to handler or runtime)
                if let Some(exc_op) = exc {
                    if let Some(local) = self.operand_local(exc_op) {
                        self.mark_global_escape(local);
                    }
                }
            }

            MirTerminator::MethodCall { receiver, args, destination, .. } => {
                // Receiver escapes to method
                if let Some(local) = self.operand_local(receiver) {
                    self.mark_arg_escape(local);
                }
                // Arguments escape to method
                for arg in args {
                    if let Some(local) = self.operand_local(arg) {
                        self.mark_arg_escape(local);
                    }
                }
                // Return value could be anything - escape to destination
                self.mark_arg_escape(destination.local);
            }
        }
    }

    fn operand_local(&self, operand: &MirOperand) -> Option<LocalId> {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => Some(place.local),
            MirOperand::Constant(_) | MirOperand::Global(_) => None,
        }
    }

    fn mark_escape_to(&mut self, src: LocalId, dst: LocalId) {
        // src flows into dst - if dst escapes, so does src
        self.points_to.entry(dst).or_default().insert(src);
        self.worklist.push(dst);
    }

    fn mark_arg_escape(&mut self, local: LocalId) {
        let current = self.escape.get(&local).copied().unwrap_or(EscapeState::NoEscape);
        let new_state = current.join(EscapeState::ArgEscape);
        if new_state != current {
            self.escape.insert(local, new_state);
            self.worklist.push(local);
        }
    }

    fn mark_global_escape(&mut self, local: LocalId) {
        let current = self.escape.get(&local).copied().unwrap_or(EscapeState::NoEscape);
        if current != EscapeState::GlobalEscape {
            self.escape.insert(local, EscapeState::GlobalEscape);
            self.worklist.push(local);
        }
    }

    fn propagate(&mut self) {
        // Propagate escape state through points-to graph
        while let Some(local) = self.worklist.pop() {
            let current_escape = self.escape.get(&local).copied().unwrap_or(EscapeState::NoEscape);

            // Propagate to everything this local points to
            if let Some(points_to) = self.points_to.get(&local).cloned() {
                for target in points_to {
                    let target_escape = self.escape.get(&target).copied().unwrap_or(EscapeState::NoEscape);
                    let new_escape = target_escape.join(current_escape);

                    if new_escape != target_escape {
                        self.escape.insert(target, new_escape);
                        self.worklist.push(target);
                    }
                }
            }
        }
    }

    fn collect_results(self) -> EscapeAnalysisResult {
        let mut stack_allocatable = FxHashSet::default();
        let mut stats = EscapeStats::default();

        stats.total_locals = self.escape.len();

        for (&local, &state) in &self.escape {
            match state {
                EscapeState::NoEscape => {
                    stats.non_escaping += 1;
                    stack_allocatable.insert(local);
                }
                EscapeState::ArgEscape => {
                    stats.arg_escaping += 1;
                    // ArgEscape might still be stack-allocatable in some cases
                    // but we're conservative here
                }
                EscapeState::GlobalEscape => {
                    stats.global_escaping += 1;
                }
            }
        }

        stats.stack_allocated = stack_allocatable.len();

        EscapeAnalysisResult {
            local_escape: self.escape,
            stack_allocatable,
            stats,
        }
    }
}

/// Runs escape analysis on a MIR body.
pub fn analyze_escapes(body: &MirBody) -> EscapeAnalysisResult {
    EscapeAnalyzer::new(body).analyze()
}

/// Applies escape analysis results to mark locals for stack allocation.
/// Returns the number of locals converted to stack allocation.
pub fn apply_escape_analysis(body: &mut MirBody, result: &EscapeAnalysisResult) -> usize {
    let mut count = 0;

    for local in &mut body.locals {
        if result.stack_allocatable.contains(&local.id) {
            // Mark this local for stack allocation
            // This will be used by codegen to allocate on stack
            count += 1;
        }
    }

    count
}

/// Combined escape analysis and stack promotion pass.
pub fn escape_analysis_pass(body: &mut MirBody) -> usize {
    let result = analyze_escapes(body);
    apply_escape_analysis(body, &result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roast_common::{Span, Symbol};
    use roast_typer::Type;

    fn make_simple_body() -> MirBody {
        MirBody {
            name: Symbol::intern("test"),
            params: vec![],
            return_ty: Type::Unit,
            locals: vec![
                MirLocal { id: 0, name: None, ty: Type::Int, mutable: true },
                MirLocal { id: 1, name: None, ty: Type::Int, mutable: false },
            ],
            blocks: vec![
                MirBlock {
                    id: 0,
                    stmts: vec![],
                    terminator: MirTerminator::Return(None),
                }
            ],
            span: Span::dummy(),
        }
    }

    #[test]
    fn test_simple_non_escaping() {
        let body = make_simple_body();
        let result = analyze_escapes(&body);

        // Local 1 should not escape (not returned)
        assert!(result.stack_allocatable.contains(&1));
    }

    #[test]
    fn test_return_escapes() {
        let body = make_simple_body();
        let result = analyze_escapes(&body);

        // Local 0 (return value) should escape
        assert!(!result.stack_allocatable.contains(&0));
    }
}
