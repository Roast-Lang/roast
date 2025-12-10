//! Function inlining.

use roast_mir::*;
use crate::analysis::estimate_function_cost;
use rustc_hash::FxHashMap;

/// Inlining configuration.
#[derive(Clone, Debug)]
pub struct InlineConfig {
    /// Maximum cost of function to inline.
    pub cost_threshold: u32,
    /// Maximum depth of inlining.
    pub max_depth: u32,
    /// Maximum expansion factor.
    pub max_expansion: f32,
    /// Always inline functions marked with #[inline(always)].
    pub always_inline_marked: bool,
    /// Never inline functions marked with #[inline(never)].
    pub never_inline_marked: bool,
}

impl Default for InlineConfig {
    fn default() -> Self {
        Self {
            cost_threshold: 100,
            max_depth: 4,
            max_expansion: 2.0,
            always_inline_marked: true,
            never_inline_marked: true,
        }
    }
}

/// Inlining decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineDecision {
    /// Inline this call.
    Inline,
    /// Do not inline.
    NoInline,
    /// Cannot inline (recursive, external, etc.).
    CannotInline,
}

/// Function inlining pass.
pub struct Inliner {
    config: InlineConfig,
    /// Cache of function costs.
    costs: FxHashMap<String, u32>,
    /// Current inline depth.
    current_depth: u32,
}

impl Inliner {
    pub fn new(config: InlineConfig) -> Self {
        Self {
            config,
            costs: FxHashMap::default(),
            current_depth: 0,
        }
    }

    /// Decide whether to inline a call.
    pub fn should_inline(
        &self,
        callee: &MirBody,
        _caller_cost: u32,
        call_count: u32,
    ) -> InlineDecision {
        // Check depth limit
        if self.current_depth >= self.config.max_depth {
            return InlineDecision::NoInline;
        }

        // Estimate callee cost
        let callee_cost = estimate_function_cost(callee);

        // Check cost threshold
        if callee_cost > self.config.cost_threshold {
            return InlineDecision::NoInline;
        }

        // Check expansion factor
        let expansion = call_count as f32 * callee_cost as f32;
        if expansion > self.config.max_expansion * callee_cost as f32 {
            return InlineDecision::NoInline;
        }

        InlineDecision::Inline
    }

    /// Inline a function body at a call site.
    pub fn inline_call(
        &mut self,
        caller: &mut MirBody,
        call_block: BlockId,
        callee: &MirBody,
    ) -> bool {
        // Find the call in the block
        let block_idx = caller.blocks.iter().position(|b| b.id == call_block);
        let block_idx = match block_idx {
            Some(idx) => idx,
            None => return false,
        };

        // Get call info from terminator
        let (destination, args, target) = match &caller.blocks[block_idx].terminator {
            MirTerminator::Call { destination, args, target, .. } => {
                (destination.clone(), args.clone(), *target)
            }
            _ => return false,
        };

        // Generate fresh local IDs for callee
        let local_offset = caller.locals.len() as u32;
        let block_offset = caller.blocks.len() as u32;

        // Remap callee locals
        let mut remapped_locals: FxHashMap<LocalId, LocalId> = FxHashMap::default();

        // Map return value to destination
        remapped_locals.insert(0, destination.local);

        // Map parameters to arguments
        for (i, param) in callee.params.iter().enumerate() {
            if i < args.len() {
                if let MirOperand::Copy(place) | MirOperand::Move(place) = &args[i] {
                    if place.projections.is_empty() {
                        remapped_locals.insert(param.local.id, place.local);
                    }
                }
            }
        }

        // Add new locals for callee's locals (excluding params)
        for local in &callee.locals {
            if !remapped_locals.contains_key(&local.id) {
                let new_id = local_offset + remapped_locals.len() as u32;
                remapped_locals.insert(local.id, new_id);
                caller.locals.push(MirLocal {
                    id: new_id,
                    ty: local.ty.clone(),
                    name: local.name.clone(),
                    mutable: local.mutable,
                });
            }
        }

        // Clone and remap callee blocks
        let mut new_blocks: Vec<MirBlock> = Vec::new();

        for callee_block in &callee.blocks {
            let new_block_id = block_offset + callee_block.id;

            let mut new_stmts = Vec::new();
            for stmt in &callee_block.stmts {
                new_stmts.push(remap_stmt(stmt, &remapped_locals));
            }

            let new_terminator = remap_terminator(
                &callee_block.terminator,
                &remapped_locals,
                block_offset,
                target,
            );

            new_blocks.push(MirBlock {
                id: new_block_id,
                stmts: new_stmts,
                terminator: new_terminator,
            });
        }

        // Update caller block to jump to inlined code
        caller.blocks[block_idx].terminator = MirTerminator::Goto(block_offset);

        // Add inlined blocks
        caller.blocks.extend(new_blocks);

        true
    }

    /// Cache a function's cost.
    pub fn cache_cost(&mut self, name: &str, body: &MirBody) {
        let cost = estimate_function_cost(body);
        self.costs.insert(name.to_string(), cost);
    }
}

fn remap_stmt(stmt: &MirStmt, locals: &FxHashMap<LocalId, LocalId>) -> MirStmt {
    MirStmt {
        span: stmt.span.clone(),
        kind: match &stmt.kind {
            MirStmtKind::Assign { place, value } => MirStmtKind::Assign {
                place: remap_place(place, locals),
                value: remap_rvalue(value, locals),
            },
            MirStmtKind::StorageLive(local) => {
                MirStmtKind::StorageLive(*locals.get(local).unwrap_or(local))
            }
            MirStmtKind::StorageDead(local) => {
                MirStmtKind::StorageDead(*locals.get(local).unwrap_or(local))
            }
            MirStmtKind::ListAppend { list, value } => MirStmtKind::ListAppend {
                list: remap_place(list, locals),
                value: remap_operand(value, locals),
            },
            MirStmtKind::SetAttr { object, attr, value } => MirStmtKind::SetAttr {
                object: remap_operand(object, locals),
                attr: *attr,
                value: remap_operand(value, locals),
            },
            MirStmtKind::SetAttrIndex { object, attr, index, value } => MirStmtKind::SetAttrIndex {
                object: remap_operand(object, locals),
                attr: *attr,
                index: remap_operand(index, locals),
                value: remap_operand(value, locals),
            },
            MirStmtKind::TryEnd => MirStmtKind::TryEnd,
            MirStmtKind::Nop => MirStmtKind::Nop,
        },
    }
}

fn remap_place(place: &MirPlace, locals: &FxHashMap<LocalId, LocalId>) -> MirPlace {
    MirPlace {
        local: *locals.get(&place.local).unwrap_or(&place.local),
        projections: place.projections.iter().map(|p| {
            match p {
                MirProjection::Index(local) => {
                    MirProjection::Index(*locals.get(local).unwrap_or(local))
                }
                other => other.clone(),
            }
        }).collect(),
    }
}

fn remap_operand(operand: &MirOperand, locals: &FxHashMap<LocalId, LocalId>) -> MirOperand {
    match operand {
        MirOperand::Copy(place) => MirOperand::Copy(remap_place(place, locals)),
        MirOperand::Move(place) => MirOperand::Move(remap_place(place, locals)),
        MirOperand::Constant(c) => MirOperand::Constant(c.clone()),
        MirOperand::Global(name) => MirOperand::Global(name.clone()),
    }
}

fn remap_rvalue(rvalue: &MirRvalue, locals: &FxHashMap<LocalId, LocalId>) -> MirRvalue {
    match rvalue {
        MirRvalue::Use(op) => MirRvalue::Use(remap_operand(op, locals)),
        MirRvalue::BinaryOp(op, l, r) => MirRvalue::BinaryOp(
            *op,
            remap_operand(l, locals),
            remap_operand(r, locals),
        ),
        MirRvalue::UnaryOp(op, operand) => MirRvalue::UnaryOp(*op, remap_operand(operand, locals)),
        MirRvalue::Aggregate(kind, ops) => MirRvalue::Aggregate(
            kind.clone(),
            ops.iter().map(|op| remap_operand(op, locals)).collect(),
        ),
        MirRvalue::Ref(place, kind) => MirRvalue::Ref(remap_place(place, locals), *kind),
        MirRvalue::Len(place) => MirRvalue::Len(remap_place(place, locals)),
        MirRvalue::Cast(op, ty) => MirRvalue::Cast(remap_operand(op, locals), ty.clone()),
        MirRvalue::Attr(op, name) => MirRvalue::Attr(remap_operand(op, locals), *name),
        MirRvalue::Await(op) => MirRvalue::Await(remap_operand(op, locals)),
    }
}

fn remap_terminator(
    term: &MirTerminator,
    locals: &FxHashMap<LocalId, LocalId>,
    block_offset: u32,
    return_target: Option<BlockId>,
) -> MirTerminator {
    match term {
        MirTerminator::Goto(target) => MirTerminator::Goto(block_offset + target),
        MirTerminator::SwitchInt { discr, targets, otherwise } => MirTerminator::SwitchInt {
            discr: remap_operand(discr, locals),
            targets: targets.iter().map(|(v, t)| (v.clone(), block_offset + t)).collect(),
            otherwise: block_offset + otherwise,
        },
        MirTerminator::Call { func, args, destination, target, unwind } => MirTerminator::Call {
            func: remap_operand(func, locals),
            args: args.iter().map(|a| remap_operand(a, locals)).collect(),
            destination: remap_place(destination, locals),
            target: target.map(|t| block_offset + t),
            unwind: unwind.map(|u| block_offset + u),
        },
        MirTerminator::Drop { place, target, unwind } => MirTerminator::Drop {
            place: remap_place(place, locals),
            target: block_offset + target,
            unwind: unwind.map(|u| block_offset + u),
        },
        MirTerminator::Assert { cond, expected, target, msg } => MirTerminator::Assert {
            cond: remap_operand(cond, locals),
            expected: *expected,
            target: block_offset + target,
            msg: msg.clone(),
        },
        MirTerminator::Return(operand) => {
            // Convert return to goto return_target
            if let Some(target) = return_target {
                MirTerminator::Goto(target)
            } else {
                MirTerminator::Return(operand.as_ref().map(|op| remap_operand(op, locals)))
            }
        }
        MirTerminator::ForIter { iter, loop_var, body, exit } => MirTerminator::ForIter {
            iter: remap_place(iter, locals),
            loop_var: *locals.get(loop_var).unwrap_or(loop_var),
            body: block_offset + body,
            exit: block_offset + exit,
        },
        MirTerminator::Unreachable => MirTerminator::Unreachable,
        MirTerminator::TryBegin { body: try_body, handlers, finally, exit } => {
            let remapped_handlers = handlers.iter().map(|h| MirExceptHandler {
                exc_type: h.exc_type,
                exc_var: h.exc_var.map(|n| *locals.get(&n).unwrap_or(&n)),
                body: block_offset + h.body,
            }).collect();
            MirTerminator::TryBegin {
                body: block_offset + try_body,
                handlers: remapped_handlers,
                finally: finally.map(|f| block_offset + f),
                exit: block_offset + exit,
            }
        }
        MirTerminator::Raise { exc } => MirTerminator::Raise {
            exc: exc.as_ref().map(|e| remap_operand(e, locals)),
        },
        MirTerminator::MethodCall { receiver, receiver_class, receiver_type, method, args, destination, target } => MirTerminator::MethodCall {
            receiver: remap_operand(receiver, locals),
            receiver_class: receiver_class.clone(),
            receiver_type: receiver_type.clone(),
            method: *method,
            args: args.iter().map(|a| remap_operand(a, locals)).collect(),
            destination: remap_place(destination, locals),
            target: target.map(|t| block_offset + t),
        },
    }
}
