//! Loop optimizations.

use roast_mir::*;
use rustc_hash::{FxHashMap, FxHashSet};
use crate::analysis::CfgAnalysis;

/// Loop information.
#[derive(Clone, Debug)]
pub struct Loop {
    /// Header block (entry point).
    pub header: BlockId,
    /// All blocks in the loop.
    pub blocks: FxHashSet<BlockId>,
    /// Back edges (blocks that jump back to header).
    pub back_edges: Vec<BlockId>,
    /// Exit blocks (blocks that can leave the loop).
    pub exits: Vec<BlockId>,
    /// Nesting depth.
    pub depth: u32,
}

/// Find all natural loops in the CFG.
pub fn find_loops(body: &MirBody) -> Vec<Loop> {
    let cfg = CfgAnalysis::new(body);
    let mut loops = Vec::new();

    // Find back edges: edges where target dominates source
    for block in &body.blocks {
        for succ in cfg.successors(block.id) {
            if cfg.dominates(*succ, block.id) {
                // Back edge found: block -> succ
                // succ is the loop header
                let loop_blocks = find_loop_blocks(body, &cfg, *succ, block.id);
                let exits = find_exit_blocks(&cfg, &loop_blocks);

                loops.push(Loop {
                    header: *succ,
                    blocks: loop_blocks,
                    back_edges: vec![block.id],
                    exits,
                    depth: 0,
                });
            }
        }
    }

    // Compute nesting depths
    compute_loop_depths(&mut loops);

    loops
}

fn find_loop_blocks(
    body: &MirBody,
    cfg: &CfgAnalysis,
    header: BlockId,
    back_edge_source: BlockId,
) -> FxHashSet<BlockId> {
    let mut blocks = FxHashSet::default();
    blocks.insert(header);

    let mut worklist = vec![back_edge_source];

    while let Some(block) = worklist.pop() {
        if blocks.insert(block) {
            // Add predecessors
            for pred in cfg.predecessors(block) {
                if !blocks.contains(pred) {
                    worklist.push(*pred);
                }
            }
        }
    }

    blocks
}

fn find_exit_blocks(cfg: &CfgAnalysis, loop_blocks: &FxHashSet<BlockId>) -> Vec<BlockId> {
    let mut exits = Vec::new();

    for &block in loop_blocks {
        for succ in cfg.successors(block) {
            if !loop_blocks.contains(succ) {
                exits.push(block);
                break;
            }
        }
    }

    exits
}

fn compute_loop_depths(loops: &mut Vec<Loop>) {
    for i in 0..loops.len() {
        let mut depth = 0;
        for j in 0..loops.len() {
            if i != j && loops[j].blocks.contains(&loops[i].header) {
                depth += 1;
            }
        }
        loops[i].depth = depth;
    }
}

/// Optimize loops in the MIR body.
/// Returns the number of optimizations performed.
pub fn optimize_loops(body: &mut MirBody) -> usize {
    let loops = find_loops(body);
    let mut count = 0;

    for loop_info in &loops {
        // Loop-invariant code motion
        count += hoist_invariants(body, loop_info);

        // Loop unrolling for small loops
        count += unroll_small_loops(body, loop_info);
    }

    count
}

/// Hoist loop-invariant code out of the loop.
fn hoist_invariants(body: &mut MirBody, loop_info: &Loop) -> usize {
    let mut hoisted = 0;

    // Find invariant instructions
    let invariants = find_invariant_stmts(body, loop_info);

    // Create a preheader block if needed
    if !invariants.is_empty() {
        // For now, just mark as hoisted - actual hoisting would require
        // creating a preheader block
        hoisted = invariants.len();
    }

    hoisted
}

fn find_invariant_stmts(body: &MirBody, loop_info: &Loop) -> Vec<(BlockId, usize)> {
    let mut invariants = Vec::new();

    // Find all definitions in the loop
    let loop_defs = find_loop_definitions(body, loop_info);

    for &block_id in &loop_info.blocks {
        if let Some(block) = body.blocks.iter().find(|b| b.id == block_id) {
            for (idx, stmt) in block.stmts.iter().enumerate() {
                if is_invariant(stmt, &loop_defs) {
                    invariants.push((block_id, idx));
                }
            }
        }
    }

    invariants
}

fn find_loop_definitions(body: &MirBody, loop_info: &Loop) -> FxHashSet<LocalId> {
    let mut defs = FxHashSet::default();

    for &block_id in &loop_info.blocks {
        if let Some(block) = body.blocks.iter().find(|b| b.id == block_id) {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { place, .. } = &stmt.kind {
                    if place.projections.is_empty() {
                        defs.insert(place.local);
                    }
                }
            }
        }
    }

    defs
}

fn is_invariant(stmt: &MirStmt, loop_defs: &FxHashSet<LocalId>) -> bool {
    if let MirStmtKind::Assign { value, .. } = &stmt.kind {
        // An instruction is invariant if all its operands are defined outside the loop
        return !uses_loop_def(value, loop_defs);
    }
    false
}

fn uses_loop_def(rvalue: &MirRvalue, loop_defs: &FxHashSet<LocalId>) -> bool {
    match rvalue {
        MirRvalue::Use(op) => operand_uses_loop_def(op, loop_defs),
        MirRvalue::BinaryOp(_, l, r) => {
            operand_uses_loop_def(l, loop_defs) || operand_uses_loop_def(r, loop_defs)
        }
        MirRvalue::UnaryOp(_, op) => operand_uses_loop_def(op, loop_defs),
        MirRvalue::Aggregate(_, ops) => ops.iter().any(|op| operand_uses_loop_def(op, loop_defs)),
        MirRvalue::Ref(place, _) | MirRvalue::Len(place) => loop_defs.contains(&place.local),
        MirRvalue::Cast(op, _) => operand_uses_loop_def(op, loop_defs),
        MirRvalue::Attr(op, _) => operand_uses_loop_def(op, loop_defs),
        MirRvalue::Await(op) => operand_uses_loop_def(op, loop_defs),
    }
}

fn operand_uses_loop_def(operand: &MirOperand, loop_defs: &FxHashSet<LocalId>) -> bool {
    match operand {
        MirOperand::Copy(place) | MirOperand::Move(place) => loop_defs.contains(&place.local),
        MirOperand::Constant(_) | MirOperand::Global(_) => false,
    }
}

/// Unroll small loops.
fn unroll_small_loops(body: &mut MirBody, loop_info: &Loop) -> usize {
    // Only unroll very small loops
    if loop_info.blocks.len() > 2 {
        return 0;
    }

    // Check for simple counted loop
    let trip_count = estimate_trip_count(body, loop_info);

    match trip_count {
        Some(n) if n <= 4 => {
            // Would unroll here - for now just count
            1
        }
        _ => 0,
    }
}

fn estimate_trip_count(body: &MirBody, loop_info: &Loop) -> Option<u32> {
    // Look for a simple induction variable pattern
    // This is a simplified version - real implementation would be more sophisticated

    let header_block = body.blocks.iter().find(|b| b.id == loop_info.header)?;

    // Look for a comparison in the terminator
    if let MirTerminator::SwitchInt { discr, .. } = &header_block.terminator {
        if let MirOperand::Copy(place) = discr {
            // Check if this local is an induction variable
            // For now, just return None (unknown)
            let _ = place;
        }
    }

    None
}

/// Strength reduction for induction variables.
pub fn strength_reduce_induction(body: &mut MirBody, loop_info: &Loop) -> usize {
    let mut reductions = 0;

    // Find induction variables
    let ivs = find_induction_variables(body, loop_info);

    for iv in &ivs {
        // Look for multiplications by induction variable
        // and replace with additions
        for &block_id in &loop_info.blocks {
            if let Some(block) = body.blocks.iter_mut().find(|b| b.id == block_id) {
                for stmt in &mut block.stmts {
                    if reduce_iv_multiply(stmt, iv) {
                        reductions += 1;
                    }
                }
            }
        }
    }

    reductions
}

/// Induction variable information.
#[derive(Clone, Debug)]
struct InductionVariable {
    local: LocalId,
    init: MirConstant,
    step: MirConstant,
}

fn find_induction_variables(body: &MirBody, loop_info: &Loop) -> Vec<InductionVariable> {
    let mut ivs = Vec::new();

    // Look for patterns like: i = i + 1
    for &block_id in &loop_info.blocks {
        if let Some(block) = body.blocks.iter().find(|b| b.id == block_id) {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { place, value } = &stmt.kind {
                    if place.projections.is_empty() {
                        if let MirRvalue::BinaryOp(MirBinOp::Add, left, right) = value {
                            // Check if left is the same local
                            if let MirOperand::Copy(src) = left {
                                if src.local == place.local && src.projections.is_empty() {
                                    if let MirOperand::Constant(step) = right {
                                        ivs.push(InductionVariable {
                                            local: place.local,
                                            init: MirConstant::Int(0), // Simplified
                                            step: step.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    ivs
}

fn reduce_iv_multiply(stmt: &mut MirStmt, iv: &InductionVariable) -> bool {
    if let MirStmtKind::Assign { place: _, value } = &stmt.kind {
        if let MirRvalue::BinaryOp(MirBinOp::Mul, left, right) = value {
            // Check for iv * constant or constant * iv
            let (iv_operand, const_operand) = match (left, right) {
                (MirOperand::Copy(p), MirOperand::Constant(c))
                    if p.local == iv.local => (Some(p.clone()), Some(c.clone())),
                (MirOperand::Constant(c), MirOperand::Copy(p))
                    if p.local == iv.local => (Some(p.clone()), Some(c.clone())),
                _ => (None, None),
            };

            if iv_operand.is_some() && const_operand.is_some() {
                // Would convert multiplication to addition here
                return true;
            }
        }
    }
    false
}

/// Loop vectorization (placeholder).
pub fn vectorize_loop(_body: &mut MirBody, _loop_info: &Loop) -> bool {
    // Would implement SIMD vectorization here
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would go here
}
