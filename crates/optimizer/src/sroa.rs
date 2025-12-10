//! Scalar Replacement of Aggregates (SROA)
//!
//! This optimization pass breaks down aggregate types (structs, tuples) into
//! their constituent scalar values. This enables:
//!
//! - Better register allocation (scalars can live in registers)
//! - Elimination of memory operations
//! - Better constant propagation and dead code elimination
//! - Improved code generation
//!
//! # Example
//!
//! Before SROA:
//! ```text
//! let point = Point { x: 1, y: 2 }
//! let z = point.x + point.y
//! ```
//!
//! After SROA:
//! ```text
//! let point_x = 1
//! let point_y = 2
//! let z = point_x + point_y
//! ```

use roast_mir::*;
use roast_typer::Type;
use rustc_hash::{FxHashMap, FxHashSet};

/// SROA configuration.
#[derive(Clone, Debug)]
pub struct SroaConfig {
    /// Maximum number of fields to split.
    pub max_fields: usize,
    /// Maximum aggregate nesting depth.
    pub max_depth: usize,
    /// Only split aggregates used in specific patterns.
    pub conservative: bool,
}

impl Default for SroaConfig {
    fn default() -> Self {
        Self {
            max_fields: 8,
            max_depth: 3,
            conservative: false,
        }
    }
}

/// Result of SROA analysis.
#[derive(Clone, Debug)]
pub struct SroaResult {
    /// Locals that were split.
    pub split_locals: FxHashSet<LocalId>,
    /// Mapping from (local, projection) -> new_local.
    pub field_map: FxHashMap<(LocalId, Vec<u32>), LocalId>,
    /// Number of new locals created.
    pub new_locals_count: usize,
    /// Statistics.
    pub stats: SroaStats,
}

/// SROA statistics.
#[derive(Clone, Debug, Default)]
pub struct SroaStats {
    pub aggregates_analyzed: usize,
    pub aggregates_split: usize,
    pub fields_promoted: usize,
    pub memory_ops_eliminated: usize,
}

/// SROA analyzer and transformer.
pub struct SroaPass<'a> {
    body: &'a mut MirBody,
    config: SroaConfig,
    /// Candidates for splitting.
    candidates: FxHashSet<LocalId>,
    /// Field accesses for each local.
    field_accesses: FxHashMap<LocalId, FxHashSet<Vec<u32>>>,
    /// Locals that have their address taken.
    address_taken: FxHashSet<LocalId>,
    /// Mapping from (local, field_path) to new scalar local.
    field_map: FxHashMap<(LocalId, Vec<u32>), LocalId>,
    /// Next available local ID.
    next_local_id: LocalId,
    /// Statistics.
    stats: SroaStats,
}

impl<'a> SroaPass<'a> {
    pub fn new(body: &'a mut MirBody, config: SroaConfig) -> Self {
        let next_local_id = body.locals.iter().map(|l| l.id).max().unwrap_or(0) + 1;

        Self {
            body,
            config,
            candidates: FxHashSet::default(),
            field_accesses: FxHashMap::default(),
            address_taken: FxHashSet::default(),
            field_map: FxHashMap::default(),
            next_local_id,
            stats: SroaStats::default(),
        }
    }

    /// Runs SROA on the function body.
    pub fn run(mut self) -> SroaStats {
        // Phase 1: Identify candidate aggregates
        self.identify_candidates();

        // Phase 2: Analyze field accesses
        self.analyze_accesses();

        // Phase 3: Filter candidates
        self.filter_candidates();

        // Phase 4: Create scalar locals
        self.create_scalar_locals();

        // Phase 5: Rewrite uses
        self.rewrite_uses();

        self.stats
    }

    fn identify_candidates(&mut self) {
        // Find all locals that are aggregates (tuples, classes)
        for local in &self.body.locals {
            self.stats.aggregates_analyzed += 1;

            if self.is_aggregate_type(&local.ty) {
                self.candidates.insert(local.id);
            }
        }
    }

    fn is_aggregate_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Tuple(_) | Type::Class(_))
    }

    fn get_field_count(&self, ty: &Type) -> usize {
        match ty {
            Type::Tuple(fields) => fields.len(),
            Type::Class(cls) => cls.members.len(),
            _ => 0,
        }
    }

    fn get_field_type(&self, ty: &Type, field_idx: u32) -> Option<Type> {
        match ty {
            Type::Tuple(fields) => fields.get(field_idx as usize).cloned(),
            Type::Class(cls) => cls.members.get(field_idx as usize).map(|(_, t)| t.clone()),
            _ => None,
        }
    }

    fn get_field_name(&self, ty: &Type, field_idx: u32) -> Option<String> {
        match ty {
            Type::Tuple(_) => Some(format!("{}", field_idx)),
            Type::Class(cls) => cls.members.get(field_idx as usize).map(|(n, _)| n.clone()),
            _ => None,
        }
    }

    fn analyze_accesses(&mut self) {
        // Clone blocks to avoid borrow issues
        let blocks: Vec<_> = self.body.blocks.iter().cloned().collect();

        for block in &blocks {
            for stmt in &block.stmts {
                Self::analyze_stmt_static(
                    stmt,
                    &mut self.field_accesses,
                    &mut self.address_taken,
                );
            }
            Self::analyze_terminator_static(&block.terminator, &mut self.field_accesses);
        }
    }

    fn analyze_stmt_static(
        stmt: &MirStmt,
        field_accesses: &mut FxHashMap<LocalId, FxHashSet<Vec<u32>>>,
        address_taken: &mut FxHashSet<LocalId>,
    ) {
        match &stmt.kind {
            MirStmtKind::Assign { place, value } => {
                // Track field accesses
                if !place.projections.is_empty() {
                    let fields = Self::extract_field_path_static(&place.projections);
                    if !fields.is_empty() {
                        field_accesses
                            .entry(place.local)
                            .or_default()
                            .insert(fields);
                    }
                }

                Self::analyze_rvalue_static(value, field_accesses, address_taken);
            }
            _ => {}
        }
    }

    fn analyze_rvalue_static(
        rvalue: &MirRvalue,
        field_accesses: &mut FxHashMap<LocalId, FxHashSet<Vec<u32>>>,
        address_taken: &mut FxHashSet<LocalId>,
    ) {
        match rvalue {
            MirRvalue::Use(op) => Self::analyze_operand_static(op, field_accesses),
            MirRvalue::Ref(place, _) => {
                // Address taken - can't split
                address_taken.insert(place.local);

                // Track field access
                if !place.projections.is_empty() {
                    let fields = Self::extract_field_path_static(&place.projections);
                    if !fields.is_empty() {
                        field_accesses
                            .entry(place.local)
                            .or_default()
                            .insert(fields);
                    }
                }
            }
            MirRvalue::BinaryOp(_, left, right) => {
                Self::analyze_operand_static(left, field_accesses);
                Self::analyze_operand_static(right, field_accesses);
            }
            MirRvalue::UnaryOp(_, op) => Self::analyze_operand_static(op, field_accesses),
            MirRvalue::Aggregate(_, ops) => {
                for op in ops {
                    Self::analyze_operand_static(op, field_accesses);
                }
            }
            MirRvalue::Cast(op, _) => Self::analyze_operand_static(op, field_accesses),
            MirRvalue::Len(place) => {
                if !place.projections.is_empty() {
                    let fields = Self::extract_field_path_static(&place.projections);
                    if !fields.is_empty() {
                        field_accesses
                            .entry(place.local)
                            .or_default()
                            .insert(fields);
                    }
                }
            }
            MirRvalue::Attr(op, _) => Self::analyze_operand_static(op, field_accesses),
            MirRvalue::Await(op) => Self::analyze_operand_static(op, field_accesses),
        }
    }

    fn analyze_operand_static(
        operand: &MirOperand,
        field_accesses: &mut FxHashMap<LocalId, FxHashSet<Vec<u32>>>,
    ) {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                if !place.projections.is_empty() {
                    let fields = Self::extract_field_path_static(&place.projections);
                    if !fields.is_empty() {
                        field_accesses
                            .entry(place.local)
                            .or_default()
                            .insert(fields);
                    }
                }
            }
            MirOperand::Constant(_) | MirOperand::Global(_) => {}
        }
    }

    fn analyze_terminator_static(
        term: &MirTerminator,
        field_accesses: &mut FxHashMap<LocalId, FxHashSet<Vec<u32>>>,
    ) {
        match term {
            MirTerminator::Call { func, args, .. } => {
                Self::analyze_operand_static(func, field_accesses);
                for arg in args {
                    Self::analyze_operand_static(arg, field_accesses);
                }
            }
            MirTerminator::SwitchInt { discr, .. } => {
                Self::analyze_operand_static(discr, field_accesses);
            }
            MirTerminator::Assert { cond, .. } => {
                Self::analyze_operand_static(cond, field_accesses);
            }
            _ => {}
        }
    }

    fn extract_field_path_static(projections: &[MirProjection]) -> Vec<u32> {
        projections.iter()
            .filter_map(|p| match p {
                MirProjection::Field(idx) => Some(*idx),
                _ => None,
            })
            .collect()
    }

    fn extract_field_path(&self, projections: &[MirProjection]) -> Vec<u32> {
        Self::extract_field_path_static(projections)
    }

    fn filter_candidates(&mut self) {
        // Remove candidates that:
        // 1. Have their address taken
        // 2. Have too many fields
        // 3. Are used as whole aggregates (not just field accesses)

        let to_remove: Vec<_> = self.candidates.iter()
            .filter(|&&local| {
                // Check address taken
                if self.address_taken.contains(&local) {
                    return true;
                }

                // Check field count
                if let Some(local_def) = self.body.locals.iter().find(|l| l.id == local) {
                    if self.get_field_count(&local_def.ty) > self.config.max_fields {
                        return true;
                    }
                }

                // Check if used as whole (no field accesses)
                if self.config.conservative {
                    if self.field_accesses.get(&local).map_or(true, |f| f.is_empty()) {
                        return true;
                    }
                }

                false
            })
            .copied()
            .collect();

        for local in to_remove {
            self.candidates.remove(&local);
        }
    }

    fn create_scalar_locals(&mut self) {
        // Clone to avoid borrow conflict
        let candidates: Vec<_> = self.candidates.iter().copied().collect();

        for local in candidates {
            if let Some(local_def) = self.body.locals.iter().find(|l| l.id == local).cloned() {
                let field_count = self.get_field_count(&local_def.ty);

                for field_idx in 0..field_count {
                    let field_ty = self.get_field_type(&local_def.ty, field_idx as u32)
                        .unwrap_or(Type::Any);

                    let new_id = self.alloc_local_id();
                    // Note: We don't have access to the interner here, so we use None
                    // The debug/display can be handled elsewhere
                    let new_name = None;

                    let new_local = MirLocal {
                        id: new_id,
                        name: new_name,
                        ty: field_ty,
                        mutable: local_def.mutable,
                    };

                    self.body.locals.push(new_local);
                    self.field_map.insert((local, vec![field_idx as u32]), new_id);
                    self.stats.fields_promoted += 1;
                }

                self.stats.aggregates_split += 1;
            }
        }
    }

    fn rewrite_uses(&mut self) {
        // Clone candidates and field_map for borrow checker
        let candidates = self.candidates.clone();
        let field_map = self.field_map.clone();
        let mut memory_ops_eliminated = 0;

        for block in &mut self.body.blocks {
            for stmt in &mut block.stmts {
                memory_ops_eliminated += Self::rewrite_stmt_static(stmt, &candidates, &field_map);
            }
            Self::rewrite_terminator_static(&mut block.terminator, &candidates, &field_map);
        }

        self.stats.memory_ops_eliminated = memory_ops_eliminated;
    }

    fn rewrite_stmt_static(
        stmt: &mut MirStmt,
        candidates: &FxHashSet<LocalId>,
        field_map: &FxHashMap<(LocalId, Vec<u32>), LocalId>,
    ) -> usize {
        let mut ops_eliminated = 0;

        if let MirStmtKind::Assign { place, value } = &mut stmt.kind {
            // Rewrite place if it's a field access of a split aggregate
            if candidates.contains(&place.local) && !place.projections.is_empty() {
                let fields = Self::extract_field_path_static(&place.projections);
                if let Some(&new_local) = field_map.get(&(place.local, fields)) {
                    place.local = new_local;
                    place.projections.clear();
                    ops_eliminated += 1;
                }
            }

            // Rewrite operands
            Self::rewrite_rvalue_static(value, candidates, field_map);
        }

        ops_eliminated
    }

    fn rewrite_rvalue_static(
        rvalue: &mut MirRvalue,
        candidates: &FxHashSet<LocalId>,
        field_map: &FxHashMap<(LocalId, Vec<u32>), LocalId>,
    ) {
        match rvalue {
            MirRvalue::Use(op) => Self::rewrite_operand_static(op, candidates, field_map),
            MirRvalue::BinaryOp(_, left, right) => {
                Self::rewrite_operand_static(left, candidates, field_map);
                Self::rewrite_operand_static(right, candidates, field_map);
            }
            MirRvalue::UnaryOp(_, op) => Self::rewrite_operand_static(op, candidates, field_map),
            MirRvalue::Cast(op, _) => Self::rewrite_operand_static(op, candidates, field_map),
            MirRvalue::Aggregate(_, ops) => {
                for op in ops {
                    Self::rewrite_operand_static(op, candidates, field_map);
                }
            }
            MirRvalue::Ref(place, _) | MirRvalue::Len(place) => {
                if candidates.contains(&place.local) && !place.projections.is_empty() {
                    let fields = Self::extract_field_path_static(&place.projections);
                    if let Some(&new_local) = field_map.get(&(place.local, fields)) {
                        place.local = new_local;
                        place.projections.clear();
                    }
                }
            }
            MirRvalue::Attr(op, _) => Self::rewrite_operand_static(op, candidates, field_map),
            MirRvalue::Await(op) => Self::rewrite_operand_static(op, candidates, field_map),
        }
    }

    fn rewrite_operand_static(
        operand: &mut MirOperand,
        candidates: &FxHashSet<LocalId>,
        field_map: &FxHashMap<(LocalId, Vec<u32>), LocalId>,
    ) {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                if candidates.contains(&place.local) && !place.projections.is_empty() {
                    let fields = Self::extract_field_path_static(&place.projections);
                    if let Some(&new_local) = field_map.get(&(place.local, fields)) {
                        place.local = new_local;
                        place.projections.clear();
                    }
                }
            }
            MirOperand::Constant(_) | MirOperand::Global(_) => {}
        }
    }

    fn rewrite_terminator_static(
        term: &mut MirTerminator,
        candidates: &FxHashSet<LocalId>,
        field_map: &FxHashMap<(LocalId, Vec<u32>), LocalId>,
    ) {
        match term {
            MirTerminator::Call { func, args, .. } => {
                Self::rewrite_operand_static(func, candidates, field_map);
                for arg in args {
                    Self::rewrite_operand_static(arg, candidates, field_map);
                }
            }
            MirTerminator::SwitchInt { discr, .. } => {
                Self::rewrite_operand_static(discr, candidates, field_map);
            }
            MirTerminator::Assert { cond, .. } => {
                Self::rewrite_operand_static(cond, candidates, field_map);
            }
            _ => {}
        }
    }

    fn alloc_local_id(&mut self) -> LocalId {
        let id = self.next_local_id;
        self.next_local_id += 1;
        id
    }
}

/// Runs SROA optimization with default configuration.
pub fn scalar_replacement(body: &mut MirBody) -> usize {
    scalar_replacement_with_config(body, SroaConfig::default())
}

/// Runs SROA optimization with custom configuration.
pub fn scalar_replacement_with_config(body: &mut MirBody, config: SroaConfig) -> usize {
    let stats = SroaPass::new(body, config).run();
    stats.aggregates_split
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would go here
}
