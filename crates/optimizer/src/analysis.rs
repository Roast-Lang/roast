//! Program analysis for optimization.

use roast_mir::*;
use rustc_hash::{FxHashMap, FxHashSet};

/// Control flow graph analysis.
pub struct CfgAnalysis<'a> {
    body: &'a MirBody,
    predecessors: FxHashMap<BlockId, Vec<BlockId>>,
    successors: FxHashMap<BlockId, Vec<BlockId>>,
    dominators: FxHashMap<BlockId, BlockId>,
    post_dominators: FxHashMap<BlockId, BlockId>,
}

impl<'a> CfgAnalysis<'a> {
    pub fn new(body: &'a MirBody) -> Self {
        let mut analysis = Self {
            body,
            predecessors: FxHashMap::default(),
            successors: FxHashMap::default(),
            dominators: FxHashMap::default(),
            post_dominators: FxHashMap::default(),
        };

        analysis.compute_cfg();
        analysis.compute_dominators();

        analysis
    }

    fn compute_cfg(&mut self) {
        for block in &self.body.blocks {
            let succs = successor_blocks(&block.terminator);
            self.successors.insert(block.id, succs.clone());

            for succ in succs {
                self.predecessors
                    .entry(succ)
                    .or_default()
                    .push(block.id);
            }
        }
    }

    fn compute_dominators(&mut self) {
        // Simple dominator computation
        // Entry block dominates itself
        if !self.body.blocks.is_empty() {
            let entry = self.body.blocks[0].id;
            self.dominators.insert(entry, entry);

            // Iterative dataflow
            let mut changed = true;
            while changed {
                changed = false;

                for block in &self.body.blocks {
                    if block.id == entry {
                        continue;
                    }

                    let preds = self.predecessors.get(&block.id);
                    if let Some(preds) = preds {
                        if let Some(&first_pred) = preds.first() {
                            let mut idom = self.dominators.get(&first_pred).copied();

                            for &pred in preds.iter().skip(1) {
                                if let Some(pred_dom) = self.dominators.get(&pred) {
                                    idom = idom.map(|d| self.intersect(d, *pred_dom));
                                }
                            }

                            if let Some(idom) = idom {
                                if self.dominators.get(&block.id) != Some(&idom) {
                                    self.dominators.insert(block.id, idom);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn intersect(&self, mut a: BlockId, mut b: BlockId) -> BlockId {
        while a != b {
            while a > b {
                a = self.dominators.get(&a).copied().unwrap_or(a);
            }
            while b > a {
                b = self.dominators.get(&b).copied().unwrap_or(b);
            }
        }
        a
    }

    /// Get predecessors of a block.
    pub fn predecessors(&self, block: BlockId) -> &[BlockId] {
        self.predecessors.get(&block).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get successors of a block.
    pub fn successors(&self, block: BlockId) -> &[BlockId] {
        self.successors.get(&block).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Check if a block dominates another.
    pub fn dominates(&self, dominator: BlockId, dominated: BlockId) -> bool {
        if dominator == dominated {
            return true;
        }

        let mut current = dominated;
        while let Some(&dom) = self.dominators.get(&current) {
            if dom == dominator {
                return true;
            }
            if dom == current {
                break;
            }
            current = dom;
        }
        false
    }
}

fn successor_blocks(term: &MirTerminator) -> Vec<BlockId> {
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

/// Liveness analysis.
pub struct LivenessAnalysis<'a> {
    body: &'a MirBody,
    /// Variables live at entry of each block.
    live_in: FxHashMap<BlockId, FxHashSet<LocalId>>,
    /// Variables live at exit of each block.
    live_out: FxHashMap<BlockId, FxHashSet<LocalId>>,
}

impl<'a> LivenessAnalysis<'a> {
    pub fn new(body: &'a MirBody) -> Self {
        let mut analysis = Self {
            body,
            live_in: FxHashMap::default(),
            live_out: FxHashMap::default(),
        };

        analysis.compute();
        analysis
    }

    fn compute(&mut self) {
        let cfg = CfgAnalysis::new(self.body);

        // Initialize
        for block in &self.body.blocks {
            self.live_in.insert(block.id, FxHashSet::default());
            self.live_out.insert(block.id, FxHashSet::default());
        }

        // Iterate until fixed point
        let mut changed = true;
        while changed {
            changed = false;

            // Process blocks in reverse order
            for block in self.body.blocks.iter().rev() {
                // live_out = union of live_in of successors
                let mut new_live_out = FxHashSet::default();
                for succ in cfg.successors(block.id) {
                    if let Some(succ_in) = self.live_in.get(succ) {
                        new_live_out.extend(succ_in.iter().copied());
                    }
                }

                // live_in = use(block) ∪ (live_out - def(block))
                let (defs, uses) = self.compute_def_use(block);
                let mut new_live_in = new_live_out.clone();
                for def in &defs {
                    new_live_in.remove(def);
                }
                new_live_in.extend(uses);

                // Check for changes
                if new_live_in != *self.live_in.get(&block.id).unwrap() {
                    self.live_in.insert(block.id, new_live_in);
                    changed = true;
                }
                if new_live_out != *self.live_out.get(&block.id).unwrap() {
                    self.live_out.insert(block.id, new_live_out);
                    changed = true;
                }
            }
        }
    }

    fn compute_def_use(&self, block: &MirBlock) -> (FxHashSet<LocalId>, FxHashSet<LocalId>) {
        let mut defs = FxHashSet::default();
        let mut uses = FxHashSet::default();

        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { place, value } => {
                    // Uses first
                    self.collect_uses_in_rvalue(value, &mut uses);
                    // Then def
                    if place.projections.is_empty() {
                        defs.insert(place.local);
                    }
                }
                MirStmtKind::ListAppend { list, value } => {
                    // Uses list and value
                    uses.insert(list.local);
                    self.collect_uses_in_operand(value, &mut uses);
                }
                MirStmtKind::SetAttr { object, attr: _, value } => {
                    // Uses object and value
                    self.collect_uses_in_operand(object, &mut uses);
                    self.collect_uses_in_operand(value, &mut uses);
                }
                MirStmtKind::SetAttrIndex { object, attr: _, index, value } => {
                    // Uses object, index, and value
                    self.collect_uses_in_operand(object, &mut uses);
                    self.collect_uses_in_operand(index, &mut uses);
                    self.collect_uses_in_operand(value, &mut uses);
                }
                MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop | MirStmtKind::TryEnd => {}
            }
        }

        // Uses in terminator
        match &block.terminator {
            MirTerminator::SwitchInt { discr, .. } => {
                self.collect_uses_in_operand(discr, &mut uses);
            }
            MirTerminator::Call { func, args, .. } => {
                self.collect_uses_in_operand(func, &mut uses);
                for arg in args {
                    self.collect_uses_in_operand(arg, &mut uses);
                }
            }
            MirTerminator::Assert { cond, .. } => {
                self.collect_uses_in_operand(cond, &mut uses);
            }
            _ => {}
        }

        (defs, uses)
    }

    fn collect_uses_in_rvalue(&self, rvalue: &MirRvalue, uses: &mut FxHashSet<LocalId>) {
        match rvalue {
            MirRvalue::Use(op) => self.collect_uses_in_operand(op, uses),
            MirRvalue::BinaryOp(_, l, r) => {
                self.collect_uses_in_operand(l, uses);
                self.collect_uses_in_operand(r, uses);
            }
            MirRvalue::UnaryOp(_, op) => self.collect_uses_in_operand(op, uses),
            MirRvalue::Aggregate(_, ops) => {
                for op in ops {
                    self.collect_uses_in_operand(op, uses);
                }
            }
            MirRvalue::Ref(place, _) | MirRvalue::Len(place) => {
                uses.insert(place.local);
            }
            MirRvalue::Cast(op, _) => self.collect_uses_in_operand(op, uses),
            MirRvalue::Attr(op, _) => self.collect_uses_in_operand(op, uses),
            MirRvalue::Await(op) => self.collect_uses_in_operand(op, uses),
        }
    }

    fn collect_uses_in_operand(&self, operand: &MirOperand, uses: &mut FxHashSet<LocalId>) {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                uses.insert(place.local);
            }
            MirOperand::Constant(_) | MirOperand::Global(_) => {}
        }
    }

    /// Check if a variable is live at block entry.
    pub fn is_live_at_entry(&self, block: BlockId, local: LocalId) -> bool {
        self.live_in.get(&block).map_or(false, |s| s.contains(&local))
    }

    /// Check if a variable is live at block exit.
    pub fn is_live_at_exit(&self, block: BlockId, local: LocalId) -> bool {
        self.live_out.get(&block).map_or(false, |s| s.contains(&local))
    }
}

/// Function cost estimation.
pub fn estimate_function_cost(body: &MirBody) -> u32 {
    let mut cost = 0u32;

    for block in &body.blocks {
        for stmt in &block.stmts {
            cost += estimate_stmt_cost(stmt);
        }
        cost += estimate_terminator_cost(&block.terminator);
    }

    cost
}

fn estimate_stmt_cost(stmt: &MirStmt) -> u32 {
    match &stmt.kind {
        MirStmtKind::Assign { value, .. } => estimate_rvalue_cost(value),
        MirStmtKind::ListAppend { .. } => 5, // List append has moderate cost
        MirStmtKind::SetAttr { .. } => 5, // Attribute assignment has moderate cost
        MirStmtKind::SetAttrIndex { .. } => 6, // Indexed attribute assignment has slightly higher cost
        MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop | MirStmtKind::TryEnd => 0,
    }
}

fn estimate_rvalue_cost(rvalue: &MirRvalue) -> u32 {
    match rvalue {
        MirRvalue::Use(_) => 1,
        MirRvalue::BinaryOp(op, _, _) => match op {
            MirBinOp::Add | MirBinOp::Sub | MirBinOp::BitAnd |
            MirBinOp::BitOr | MirBinOp::BitXor => 1,
            MirBinOp::Mul => 3,
            MirBinOp::Div | MirBinOp::Rem => 10,
            MirBinOp::Shl | MirBinOp::Shr => 1,
            _ => 1,
        },
        MirRvalue::UnaryOp(_, _) => 1,
        MirRvalue::Aggregate(_, ops) => ops.len() as u32 + 2,
        MirRvalue::Ref(_, _) => 1,
        MirRvalue::Len(_) => 2,
        MirRvalue::Cast(_, _) => 2,
        MirRvalue::Attr(_, _) => 2,
        MirRvalue::Await(_) => 10, // Await has high cost due to suspension
    }
}

fn estimate_terminator_cost(term: &MirTerminator) -> u32 {
    match term {
        MirTerminator::Goto(_) => 1,
        MirTerminator::SwitchInt { targets, .. } => targets.len() as u32 + 2,
        MirTerminator::Call { args, .. } => 10 + args.len() as u32 * 2,
        MirTerminator::Drop { .. } => 5,
        MirTerminator::Assert { .. } => 2,
        MirTerminator::ForIter { .. } => 8,  // ForIter includes call-like overhead
        MirTerminator::Return(_) => 1,
        MirTerminator::Unreachable => 0,
        MirTerminator::TryBegin { handlers, .. } => 10 + handlers.len() as u32 * 5,
        MirTerminator::Raise { .. } => 5,
        MirTerminator::MethodCall { args, .. } => 12 + args.len() as u32 * 2, // Slightly higher than Call due to method lookup
    }
}
