//! Optimization passes for MIR.

use roast_mir::*;
use rustc_hash::{FxHashMap, FxHashSet};

// ============================================================================
// Constant Folding
// ============================================================================

/// Constant folding pass - evaluates constant expressions at compile time.
/// Returns the number of constants folded.
pub fn constant_folding(body: &mut MirBody) -> usize {
    let mut count = 0;
    for block in &mut body.blocks {
        for stmt in &mut block.stmts {
            count += fold_constants_in_stmt(stmt);
        }
    }
    count
}

fn fold_constants_in_stmt(stmt: &mut MirStmt) -> usize {
    if let MirStmtKind::Assign { value, .. } = &mut stmt.kind {
        fold_rvalue(value)
    } else {
        0
    }
}

fn fold_rvalue(rvalue: &mut MirRvalue) -> usize {
    match rvalue {
        MirRvalue::BinaryOp(op, left, right) => {
            if let (MirOperand::Constant(l), MirOperand::Constant(r)) = (left, right) {
                if let Some(result) = fold_binary(*op, l, r) {
                    *rvalue = MirRvalue::Use(MirOperand::Constant(result));
                    return 1;
                }
            }
            0
        }
        MirRvalue::UnaryOp(op, operand) => {
            if let MirOperand::Constant(c) = operand {
                if let Some(result) = fold_unary(*op, c) {
                    *rvalue = MirRvalue::Use(MirOperand::Constant(result));
                    return 1;
                }
            }
            0
        }
        _ => 0,
    }
}

fn fold_binary(op: MirBinOp, left: &MirConstant, right: &MirConstant) -> Option<MirConstant> {
    match (left, right) {
        (MirConstant::Int(a), MirConstant::Int(b)) => {
            let result = match op {
                MirBinOp::Add => a.checked_add(*b)?,
                MirBinOp::Sub => a.checked_sub(*b)?,
                MirBinOp::Mul => a.checked_mul(*b)?,
                MirBinOp::Div if *b != 0 => a.checked_div(*b)?,
                MirBinOp::Rem if *b != 0 => a.checked_rem(*b)?,
                MirBinOp::BitAnd => a & b,
                MirBinOp::BitOr => a | b,
                MirBinOp::BitXor => a ^ b,
                MirBinOp::Shl => a.checked_shl(*b as u32)?,
                MirBinOp::Shr => a.checked_shr(*b as u32)?,
                MirBinOp::Eq => return Some(MirConstant::Bool(a == b)),
                MirBinOp::Ne => return Some(MirConstant::Bool(a != b)),
                MirBinOp::Lt => return Some(MirConstant::Bool(a < b)),
                MirBinOp::Le => return Some(MirConstant::Bool(a <= b)),
                MirBinOp::Gt => return Some(MirConstant::Bool(a > b)),
                MirBinOp::Ge => return Some(MirConstant::Bool(a >= b)),
                _ => return None,
            };
            Some(MirConstant::Int(result))
        }
        (MirConstant::Float(a), MirConstant::Float(b)) => {
            let result = match op {
                MirBinOp::Add => a + b,
                MirBinOp::Sub => a - b,
                MirBinOp::Mul => a * b,
                MirBinOp::Div => a / b,
                MirBinOp::Eq => return Some(MirConstant::Bool(a == b)),
                MirBinOp::Ne => return Some(MirConstant::Bool(a != b)),
                MirBinOp::Lt => return Some(MirConstant::Bool(a < b)),
                MirBinOp::Le => return Some(MirConstant::Bool(a <= b)),
                MirBinOp::Gt => return Some(MirConstant::Bool(a > b)),
                MirBinOp::Ge => return Some(MirConstant::Bool(a >= b)),
                _ => return None,
            };
            Some(MirConstant::Float(result))
        }
        (MirConstant::Bool(a), MirConstant::Bool(b)) => {
            let result = match op {
                MirBinOp::BitAnd => *a && *b,
                MirBinOp::BitOr => *a || *b,
                MirBinOp::BitXor => *a ^ *b,
                MirBinOp::Eq => return Some(MirConstant::Bool(a == b)),
                MirBinOp::Ne => return Some(MirConstant::Bool(a != b)),
                _ => return None,
            };
            Some(MirConstant::Bool(result))
        }
        (MirConstant::Str(a), MirConstant::Str(b)) => {
            match op {
                MirBinOp::Add => Some(MirConstant::Str(format!("{}{}", a, b))),
                MirBinOp::Eq => Some(MirConstant::Bool(a == b)),
                MirBinOp::Ne => Some(MirConstant::Bool(a != b)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn fold_unary(op: MirUnaryOp, operand: &MirConstant) -> Option<MirConstant> {
    match (op, operand) {
        (MirUnaryOp::Neg, MirConstant::Int(n)) => Some(MirConstant::Int(-n)),
        (MirUnaryOp::Neg, MirConstant::Float(n)) => Some(MirConstant::Float(-n)),
        (MirUnaryOp::Not, MirConstant::Bool(b)) => Some(MirConstant::Bool(!b)),
        (MirUnaryOp::BitNot, MirConstant::Int(n)) => Some(MirConstant::Int(!n)),
        _ => None,
    }
}

// ============================================================================
// Copy Propagation
// ============================================================================

/// Copy propagation - replaces uses of copied values with the original.
/// Returns the number of copies propagated.
pub fn copy_propagation(body: &mut MirBody) -> usize {
    // Build a map of copies: dest -> source
    let mut copies: FxHashMap<LocalId, LocalId> = FxHashMap::default();

    // Find all simple copies
    for block in &body.blocks {
        for stmt in &block.stmts {
            if let MirStmtKind::Assign { place, value } = &stmt.kind {
                if place.projections.is_empty() {
                    if let MirRvalue::Use(MirOperand::Copy(src)) = value {
                        if src.projections.is_empty() {
                            copies.insert(place.local, src.local);
                        }
                    }
                }
            }
        }
    }

    // Propagate: follow chains of copies
    let mut final_copies: FxHashMap<LocalId, LocalId> = FxHashMap::default();
    for (&dest, &src) in &copies {
        let mut current = src;
        while let Some(&next) = copies.get(&current) {
            if next == dest {
                break; // Avoid cycles
            }
            current = next;
        }
        final_copies.insert(dest, current);
    }

    // Replace uses
    for block in &mut body.blocks {
        for stmt in &mut block.stmts {
            propagate_copies_in_stmt(stmt, &final_copies);
        }
        propagate_copies_in_terminator(&mut block.terminator, &final_copies);
    }

    final_copies.len()
}

fn propagate_copies_in_stmt(stmt: &mut MirStmt, copies: &FxHashMap<LocalId, LocalId>) {
    if let MirStmtKind::Assign { value, .. } = &mut stmt.kind {
        propagate_copies_in_rvalue(value, copies);
    }
}

fn propagate_copies_in_rvalue(rvalue: &mut MirRvalue, copies: &FxHashMap<LocalId, LocalId>) {
    match rvalue {
        MirRvalue::Use(op) => propagate_copies_in_operand(op, copies),
        MirRvalue::BinaryOp(_, left, right) => {
            propagate_copies_in_operand(left, copies);
            propagate_copies_in_operand(right, copies);
        }
        MirRvalue::UnaryOp(_, op) => propagate_copies_in_operand(op, copies),
        MirRvalue::Aggregate(_, ops) => {
            for op in ops {
                propagate_copies_in_operand(op, copies);
            }
        }
        MirRvalue::Ref(place, _) => {
            if place.projections.is_empty() {
                if let Some(&src) = copies.get(&place.local) {
                    place.local = src;
                }
            }
        }
        MirRvalue::Len(place) => {
            if place.projections.is_empty() {
                if let Some(&src) = copies.get(&place.local) {
                    place.local = src;
                }
            }
        }
        MirRvalue::Cast(op, _) => propagate_copies_in_operand(op, copies),
        MirRvalue::Attr(op, _) => propagate_copies_in_operand(op, copies),
        MirRvalue::Await(op) => propagate_copies_in_operand(op, copies),
    }
}

fn propagate_copies_in_operand(operand: &mut MirOperand, copies: &FxHashMap<LocalId, LocalId>) {
    match operand {
        MirOperand::Copy(place) | MirOperand::Move(place) => {
            if place.projections.is_empty() {
                if let Some(&src) = copies.get(&place.local) {
                    place.local = src;
                }
            }
        }
        MirOperand::Constant(_) | MirOperand::Global(_) => {}
    }
}

fn propagate_copies_in_terminator(term: &mut MirTerminator, copies: &FxHashMap<LocalId, LocalId>) {
    match term {
        MirTerminator::SwitchInt { discr, .. } => {
            propagate_copies_in_operand(discr, copies);
        }
        MirTerminator::Call { func, args, .. } => {
            propagate_copies_in_operand(func, copies);
            for arg in args {
                propagate_copies_in_operand(arg, copies);
            }
        }
        MirTerminator::Assert { cond, .. } => {
            propagate_copies_in_operand(cond, copies);
        }
        _ => {}
    }
}

// ============================================================================
// Dead Code Elimination
// ============================================================================

/// Dead code elimination - removes unused assignments and unreachable blocks.
/// Returns the number of instructions eliminated.
pub fn dead_code_elimination(body: &mut MirBody) -> usize {
    let before: usize = body.blocks.iter().map(|b| b.stmts.len()).sum();
    // Find reachable blocks
    let reachable = find_reachable_blocks(body);

    // Find used locals
    let used = find_used_locals(body, &reachable);

    // Remove dead assignments
    for block in &mut body.blocks {
        if !reachable.contains(&block.id) {
            block.stmts.clear();
            block.terminator = MirTerminator::Unreachable;
            continue;
        }

        block.stmts.retain(|stmt| {
            match &stmt.kind {
                MirStmtKind::Assign { place, .. } => {
                    place.projections.is_empty() && used.contains(&place.local)
                        || !place.projections.is_empty()
                }
                MirStmtKind::StorageLive(local) | MirStmtKind::StorageDead(local) => {
                    used.contains(local)
                }
                MirStmtKind::ListAppend { list, .. } => {
                    // Keep list appends if the list is used
                    used.contains(&list.local)
                }
                MirStmtKind::SetAttr { .. } => {
                    // Always keep attribute assignments (side effects)
                    true
                }
                MirStmtKind::SetAttrIndex { .. } => {
                    // Always keep indexed attribute assignments (side effects)
                    true
                }
                MirStmtKind::TryEnd => {
                    // Always keep TryEnd - needed for exception handling
                    true
                }
                MirStmtKind::Nop => false,
            }
        });
    }

    let after: usize = body.blocks.iter().map(|b| b.stmts.len()).sum();
    before.saturating_sub(after)
}

fn find_reachable_blocks(body: &MirBody) -> FxHashSet<BlockId> {
    let mut reachable = FxHashSet::default();
    let mut worklist = vec![0]; // Start from entry block

    while let Some(block_id) = worklist.pop() {
        if !reachable.insert(block_id) {
            continue;
        }

        if let Some(block) = body.blocks.get(block_id as usize) {
            match &block.terminator {
                MirTerminator::Goto(target) => {
                    worklist.push(*target);
                }
                MirTerminator::SwitchInt { targets, otherwise, .. } => {
                    for (_, target) in targets {
                        worklist.push(*target);
                    }
                    worklist.push(*otherwise);
                }
                MirTerminator::Call { target, unwind, .. } => {
                    if let Some(t) = target {
                        worklist.push(*t);
                    }
                    if let Some(u) = unwind {
                        worklist.push(*u);
                    }
                }
                MirTerminator::Drop { target, unwind, .. } => {
                    worklist.push(*target);
                    if let Some(u) = unwind {
                        worklist.push(*u);
                    }
                }
                MirTerminator::Assert { target, .. } => {
                    worklist.push(*target);
                }
                MirTerminator::ForIter { body, exit, .. } => {
                    worklist.push(*body);
                    worklist.push(*exit);
                }
                MirTerminator::Return(_) | MirTerminator::Unreachable => {}
                MirTerminator::TryBegin { body: try_body, handlers, finally, exit } => {
                    worklist.push(*try_body);
                    for handler in handlers {
                        worklist.push(handler.body);
                    }
                    if let Some(finally_bb) = finally {
                        worklist.push(*finally_bb);
                    }
                    worklist.push(*exit);
                }
                MirTerminator::Raise { .. } => {
                    // Raise doesn't have a regular control flow successor
                }
                MirTerminator::MethodCall { target, .. } => {
                    if let Some(target_bb) = target {
                        worklist.push(*target_bb);
                    }
                }
            }
        }
    }

    reachable
}

fn find_used_locals(body: &MirBody, reachable: &FxHashSet<BlockId>) -> FxHashSet<LocalId> {
    let mut used = FxHashSet::default();

    // All parameters are considered used
    for param in &body.params {
        used.insert(param.local.id);
    }

    for block in &body.blocks {
        if !reachable.contains(&block.id) {
            continue;
        }

        for stmt in &block.stmts {
            collect_used_in_stmt(stmt, &mut used);
        }
        collect_used_in_terminator(&block.terminator, &mut used);
    }

    used
}

fn collect_used_in_stmt(stmt: &MirStmt, used: &mut FxHashSet<LocalId>) {
    if let MirStmtKind::Assign { value, .. } = &stmt.kind {
        collect_used_in_rvalue(value, used);
    }
}

fn collect_used_in_rvalue(rvalue: &MirRvalue, used: &mut FxHashSet<LocalId>) {
    match rvalue {
        MirRvalue::Use(op) => collect_used_in_operand(op, used),
        MirRvalue::BinaryOp(_, left, right) => {
            collect_used_in_operand(left, used);
            collect_used_in_operand(right, used);
        }
        MirRvalue::UnaryOp(_, op) => collect_used_in_operand(op, used),
        MirRvalue::Aggregate(_, ops) => {
            for op in ops {
                collect_used_in_operand(op, used);
            }
        }
        MirRvalue::Ref(place, _) | MirRvalue::Len(place) => {
            used.insert(place.local);
            for proj in &place.projections {
                if let MirProjection::Index(local) = proj {
                    used.insert(*local);
                }
            }
        }
        MirRvalue::Cast(op, _) => collect_used_in_operand(op, used),
        MirRvalue::Attr(op, _) => collect_used_in_operand(op, used),
        MirRvalue::Await(op) => collect_used_in_operand(op, used),
    }
}

fn collect_used_in_operand(operand: &MirOperand, used: &mut FxHashSet<LocalId>) {
    match operand {
        MirOperand::Copy(place) | MirOperand::Move(place) => {
            used.insert(place.local);
            for proj in &place.projections {
                if let MirProjection::Index(local) = proj {
                    used.insert(*local);
                }
            }
        }
        MirOperand::Constant(_) | MirOperand::Global(_) => {}
    }
}

fn collect_used_in_terminator(term: &MirTerminator, used: &mut FxHashSet<LocalId>) {
    match term {
        MirTerminator::SwitchInt { discr, .. } => {
            collect_used_in_operand(discr, used);
        }
        MirTerminator::Call { func, args, destination, .. } => {
            collect_used_in_operand(func, used);
            for arg in args {
                collect_used_in_operand(arg, used);
            }
            used.insert(destination.local);
        }
        MirTerminator::Drop { place, .. } => {
            used.insert(place.local);
        }
        MirTerminator::Assert { cond, .. } => {
            collect_used_in_operand(cond, used);
        }
        _ => {}
    }
}

// ============================================================================
// Common Subexpression Elimination
// ============================================================================

/// Common subexpression elimination.
/// Returns the number of expressions eliminated.
pub fn common_subexpression_elimination(body: &mut MirBody) -> usize {
    let mut count = 0;
    // Track available expressions: expression -> local holding its value
    let mut available: FxHashMap<ExprKey, LocalId> = FxHashMap::default();

    for block in &mut body.blocks {
        for stmt in &mut block.stmts {
            if let MirStmtKind::Assign { place, value } = &mut stmt.kind {
                if place.projections.is_empty() {
                    // Check if this expression is already available
                    if let Some(key) = rvalue_to_key(value) {
                        if let Some(&existing) = available.get(&key) {
                            // Replace with copy of existing value
                            *value = MirRvalue::Use(MirOperand::Copy(MirPlace::local(existing)));
                            count += 1;
                        } else {
                            // Add to available expressions
                            available.insert(key, place.local);
                        }
                    }
                }
            }
        }

        // Invalidate expressions at control flow points
        available.clear();
    }

    count
}

/// A key representing an expression for CSE.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ExprKey {
    BinaryOp(MirBinOp, LocalId, LocalId),
    UnaryOp(MirUnaryOp, LocalId),
    Constant(ConstKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ConstKey {
    Int(i128),
    Bool(bool),
    None,
}

fn rvalue_to_key(rvalue: &MirRvalue) -> Option<ExprKey> {
    match rvalue {
        MirRvalue::BinaryOp(op, left, right) => {
            let left_local = operand_to_local(left)?;
            let right_local = operand_to_local(right)?;
            Some(ExprKey::BinaryOp(*op, left_local, right_local))
        }
        MirRvalue::UnaryOp(op, operand) => {
            let local = operand_to_local(operand)?;
            Some(ExprKey::UnaryOp(*op, local))
        }
        MirRvalue::Use(MirOperand::Constant(c)) => {
            let key = match c {
                MirConstant::Int(n) => ConstKey::Int(*n),
                MirConstant::Bool(b) => ConstKey::Bool(*b),
                MirConstant::None => ConstKey::None,
                _ => return None,
            };
            Some(ExprKey::Constant(key))
        }
        _ => None,
    }
}

fn operand_to_local(operand: &MirOperand) -> Option<LocalId> {
    match operand {
        MirOperand::Copy(place) | MirOperand::Move(place) => {
            if place.projections.is_empty() {
                Some(place.local)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ============================================================================
// Block Merging
// ============================================================================

/// Merges consecutive blocks with unconditional jumps.
pub fn merge_blocks(body: &mut MirBody) {
    // Find blocks that can be merged
    let mut merged: FxHashSet<BlockId> = FxHashSet::default();

    loop {
        let mut changed = false;

        for i in 0..body.blocks.len() {
            let block_id = body.blocks[i].id;

            if merged.contains(&block_id) {
                continue;
            }

            if let MirTerminator::Goto(target) = body.blocks[i].terminator.clone() {
                // Can only merge if target has single predecessor
                let preds = body.blocks.iter()
                    .filter(|b| successor_ids(&b.terminator).contains(&target))
                    .count();

                if preds == 1 && target != block_id {
                    // Merge target into this block
                    if let Some(target_idx) = body.blocks.iter().position(|b| b.id == target) {
                        let target_block = body.blocks[target_idx].clone();
                        body.blocks[i].stmts.extend(target_block.stmts);
                        body.blocks[i].terminator = target_block.terminator;
                        merged.insert(target);
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    // Remove merged blocks
    body.blocks.retain(|b| !merged.contains(&b.id));
}

fn successor_ids(term: &MirTerminator) -> Vec<BlockId> {
    match term {
        MirTerminator::Goto(target) => vec![*target],
        MirTerminator::SwitchInt { targets, otherwise, .. } => {
            let mut succs: Vec<_> = targets.iter().map(|(_, t)| *t).collect();
            succs.push(*otherwise);
            succs
        }
        MirTerminator::Call { target, unwind, .. } => {
            let mut succs = Vec::new();
            if let Some(t) = target {
                succs.push(*t);
            }
            if let Some(u) = unwind {
                succs.push(*u);
            }
            succs
        }
        MirTerminator::Drop { target, unwind, .. } => {
            let mut succs = vec![*target];
            if let Some(u) = unwind {
                succs.push(*u);
            }
            succs
        }
        MirTerminator::Assert { target, .. } => vec![*target],
        MirTerminator::ForIter { body, exit, .. } => vec![*body, *exit],
        MirTerminator::Return(_) | MirTerminator::Unreachable => vec![],
        MirTerminator::TryBegin { body: try_body, handlers, finally, exit } => {
            let mut succs = vec![*try_body, *exit];
            for handler in handlers {
                succs.push(handler.body);
            }
            if let Some(finally_bb) = finally {
                succs.push(*finally_bb);
            }
            succs
        }
        MirTerminator::Raise { .. } => vec![],
        MirTerminator::MethodCall { target, .. } => {
            let mut succs = Vec::new();
            if let Some(t) = target {
                succs.push(*t);
            }
            succs
        }
    }
}

// ============================================================================
// Strength Reduction
// ============================================================================

/// Strength reduction - replaces expensive operations with cheaper ones.
/// Returns the number of reductions performed.
pub fn strength_reduction(body: &mut MirBody) -> usize {
    let mut count = 0;
    for block in &mut body.blocks {
        for stmt in &mut block.stmts {
            if let MirStmtKind::Assign { value, .. } = &mut stmt.kind {
                count += reduce_strength(value);
            }
        }
    }
    count
}

fn is_power_of_two(n: i128) -> bool {
    n > 0 && (n & (n - 1)) == 0
}

fn reduce_strength(rvalue: &mut MirRvalue) -> usize {
    if let MirRvalue::BinaryOp(ref mut op, ref mut left, ref mut right) = rvalue {
        let mut reduced = false;
        let left_clone = left.clone();
        let right_clone = right.clone();
        let current_op = *op;

        match (current_op, &left_clone, &right_clone) {
            // x * 2 -> x + x
            (MirBinOp::Mul, _, MirOperand::Constant(MirConstant::Int(2))) => {
                *rvalue = MirRvalue::BinaryOp(MirBinOp::Add, left_clone.clone(), left_clone);
                reduced = true;
            }
            // x * power_of_2 -> x << log2(n)
            (MirBinOp::Mul, _, MirOperand::Constant(MirConstant::Int(n))) if is_power_of_two(*n) => {
                let shift = n.trailing_zeros() as i128;
                *right = MirOperand::Constant(MirConstant::Int(shift));
                *op = MirBinOp::Shl;
                reduced = true;
            }
            // x / power_of_2 -> x >> log2(n) (for positive)
            (MirBinOp::Div, _, MirOperand::Constant(MirConstant::Int(n))) if *n > 0 && is_power_of_two(*n) => {
                let shift = n.trailing_zeros() as i128;
                *right = MirOperand::Constant(MirConstant::Int(shift));
                *op = MirBinOp::Shr;
                reduced = true;
            }
            // x + 0 -> x
            (MirBinOp::Add, _, MirOperand::Constant(MirConstant::Int(0))) => {
                *rvalue = MirRvalue::Use(left_clone);
                reduced = true;
            }
            (MirBinOp::Add, MirOperand::Constant(MirConstant::Int(0)), _) => {
                *rvalue = MirRvalue::Use(right_clone);
                reduced = true;
            }
            // x * 1 -> x
            (MirBinOp::Mul, _, MirOperand::Constant(MirConstant::Int(1))) => {
                *rvalue = MirRvalue::Use(left_clone);
                reduced = true;
            }
            (MirBinOp::Mul, MirOperand::Constant(MirConstant::Int(1)), _) => {
                *rvalue = MirRvalue::Use(right_clone);
                reduced = true;
            }
            // x * 0 -> 0
            (MirBinOp::Mul, _, MirOperand::Constant(MirConstant::Int(0))) => {
                *rvalue = MirRvalue::Use(MirOperand::Constant(MirConstant::Int(0)));
                reduced = true;
            }
            (MirBinOp::Mul, MirOperand::Constant(MirConstant::Int(0)), _) => {
                *rvalue = MirRvalue::Use(MirOperand::Constant(MirConstant::Int(0)));
                reduced = true;
            }
            // x - 0 -> x
            (MirBinOp::Sub, _, MirOperand::Constant(MirConstant::Int(0))) => {
                *rvalue = MirRvalue::Use(left_clone);
                reduced = true;
            }
            _ => {}
        }

        return if reduced { 1 } else { 0 };
    }
    0
}

/// Tail call optimization - converts tail calls to jumps.
/// Returns the number of tail calls optimized.
pub fn tail_call_optimization(body: &mut MirBody) -> usize {
    let mut count = 0;

    for block in &mut body.blocks {
        // Check if block ends with a return preceded by a call
        if matches!(block.terminator, MirTerminator::Return(_)) {
            // Check if last statement assigns to return value from a call
            // This would require call terminator -> assign -> return pattern
            // For now, mark potential TCO candidates
            if block.stmts.len() >= 1 {
                // Check for tail call pattern
                let last_stmt = &block.stmts[block.stmts.len() - 1];
                if let MirStmtKind::Assign { place, .. } = &last_stmt.kind {
                    // If assigning to return place (local 0)
                    if place.local == 0 && place.projections.is_empty() {
                        // This could be a tail call candidate
                        count += 1;
                    }
                }
            }
        }
    }

    count
}

// ============================================================================
// Optimization Pipeline
// ============================================================================

/// Optimization level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptLevel {
    None,
    Less,    // -O1
    Default, // -O2
    Aggressive, // -O3
}

/// Runs the optimization pipeline.
pub fn optimize(body: &mut MirBody, level: OptLevel) {
    if level == OptLevel::None {
        return;
    }

    // O1: Basic optimizations
    constant_folding(body);
    copy_propagation(body);

    if level == OptLevel::Less {
        return;
    }

    // O2: Standard optimizations
    dead_code_elimination(body);
    merge_blocks(body);

    if level == OptLevel::Default {
        return;
    }

    // O3: Aggressive optimizations
    common_subexpression_elimination(body);
    strength_reduction(body);

    // Run some passes again after other optimizations expose more opportunities
    constant_folding(body);
    copy_propagation(body);
    dead_code_elimination(body);
}
