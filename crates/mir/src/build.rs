//! MIR builder - converts HIR to MIR.

use crate::nodes::*;
use roast_common::{Span, Symbol};
use roast_hir::*;
use roast_typer::{Type, OwnershipAnalyzer, ClassType};
use id_arena::Arena;
use smallvec::smallvec;
use std::collections::HashMap;

/// Loop context for break/continue
struct LoopContext {
    header_block: BlockId,
    exit_block: BlockId,
}

/// MIR builder.
pub struct MirBuilder<'a> {
    locals: Vec<MirLocal>,
    blocks: Vec<MirBlock>,
    current_block: BlockId,
    next_local: LocalId,
    expr_arena: &'a Arena<HirExpr>,
    /// Maps symbol names to local IDs for variable lookup
    name_to_local: HashMap<Symbol, LocalId>,
    /// Maps symbol names to their types for method dispatch
    name_to_type: HashMap<Symbol, Type>,
    /// Set of symbols that are class names (for detecting constructor calls)
    class_symbols: std::collections::HashSet<Symbol>,
    /// Maps class symbols to their resolved class names
    class_names: HashMap<Symbol, String>,
    /// Stack of loop contexts for break/continue
    loop_stack: Vec<LoopContext>,
    /// Ownership analyzer for determining Copy vs Move
    ownership_analyzer: OwnershipAnalyzer,
    /// Current class name when building a method (for super() support)
    current_class: Option<String>,
    /// Method Resolution Order for the current class (for super() support)
    /// This is computed via C3 linearization and allows proper super() resolution
    /// in multiple inheritance scenarios.
    class_mro: Vec<String>,
    /// Symbol for "super" builtin (for detecting super() calls)
    super_symbol: Option<Symbol>,
}

impl<'a> MirBuilder<'a> {
    pub fn new(expr_arena: &'a Arena<HirExpr>) -> Self {
        Self {
            locals: Vec::new(),
            blocks: Vec::new(),
            current_block: 0,
            next_local: 0,
            expr_arena,
            name_to_local: HashMap::default(),
            name_to_type: HashMap::default(),
            class_symbols: std::collections::HashSet::new(),
            class_names: HashMap::new(),
            loop_stack: Vec::new(),
            ownership_analyzer: OwnershipAnalyzer::new(),
            current_class: None,
            class_mro: Vec::new(),
            super_symbol: None,
        }
    }

    /// Register a symbol as a class name (for tracking constructor calls)
    pub fn register_class(&mut self, sym: Symbol) {
        self.class_symbols.insert(sym);
    }

    /// Register a class with its resolved name (for tracking constructor calls)
    pub fn register_class_with_name(&mut self, sym: Symbol, name: &str) {
        self.class_symbols.insert(sym);
        self.class_names.insert(sym, name.to_string());
    }

    /// Set the class context for building methods (for super() support)
    /// `mro` is the Method Resolution Order computed via C3 linearization.
    pub fn set_class_context(&mut self, class_name: Option<String>, mro: Vec<String>) {
        self.current_class = class_name;
        self.class_mro = mro;
    }

    /// Set the super symbol for detecting super() calls
    pub fn set_super_symbol(&mut self, sym: Symbol) {
        self.super_symbol = Some(sym);
    }

    /// Get the next class in the MRO after the current class.
    /// This is used for super() resolution in multiple inheritance.
    fn get_next_in_mro(&self) -> Option<String> {
        if let Some(ref current) = self.current_class {
            roast_typer::get_next_in_mro(&self.class_mro, current)
        } else {
            // No current class, return first parent if available
            self.class_mro.get(1).cloned()
        }
    }

    /// Builds MIR for a function.
    pub fn build_function(&mut self, func: &HirFunction) -> MirBody {
        self.locals.clear();
        self.blocks.clear();
        self.next_local = 0;
        self.name_to_local.clear();
        self.name_to_type.clear();
        self.loop_stack.clear();

        // Create locals for parameters with their kinds
        let params: Vec<_> = func.params.iter().map(|p| {
            let local = self.new_local(Some(p.name), p.ty.clone(), false);
            self.name_to_local.insert(p.name, local.id);
            let kind = match p.kind {
                HirParamKind::Regular => MirParamKind::Regular,
                HirParamKind::VarPositional => MirParamKind::VarPositional,
                HirParamKind::VarKeyword => MirParamKind::VarKeyword,
            };
            MirParam { local, kind }
        }).collect();

        // Create entry block
        self.new_block();

        // Build body
        self.build_block(&func.body);

        // Add return if needed
        let term = self.current_terminator();
        if term.is_none() || matches!(term, Some(MirTerminator::Unreachable)) {
            self.set_terminator(MirTerminator::Return(None));
        }

        MirBody {
            name: func.name,
            params,
            return_ty: func.return_type.clone(),
            locals: std::mem::take(&mut self.locals),
            blocks: std::mem::take(&mut self.blocks),
            span: func.span,
        }
    }

    fn new_local(&mut self, name: Option<Symbol>, ty: Type, mutable: bool) -> MirLocal {
        let id = self.next_local;
        self.next_local += 1;
        let local = MirLocal { id, name, ty, mutable };
        self.locals.push(local.clone());
        // Register named locals for variable lookup
        if let Some(n) = name {
            self.name_to_local.insert(n, id);
        }
        local
    }

    fn new_block(&mut self) -> BlockId {
        let id = self.blocks.len() as BlockId;
        self.blocks.push(MirBlock {
            id,
            stmts: Vec::new(),
            terminator: MirTerminator::Unreachable,
        });
        self.current_block = id;
        id
    }

    fn current_terminator(&self) -> Option<&MirTerminator> {
        self.blocks.get(self.current_block as usize).map(|b| &b.terminator)
    }

    fn set_terminator(&mut self, term: MirTerminator) {
        if let Some(block) = self.blocks.get_mut(self.current_block as usize) {
            block.terminator = term;
        }
    }

    fn push_stmt(&mut self, stmt: MirStmt) {
        if let Some(block) = self.blocks.get_mut(self.current_block as usize) {
            block.stmts.push(stmt);
        }
    }

    fn build_block(&mut self, block: &HirBlock) {
        for stmt in &block.stmts {
            self.build_stmt(stmt);
        }
    }

    fn build_stmt(&mut self, stmt: &HirStmt) {
        match &stmt.kind {
            HirStmtKind::Let { name, ty, init, mutable } => {
                // Track type if initializer is a constructor call
                let mut actual_ty = ty.clone();
                if let Some(init_expr_id) = init {
                    let init_expr = self.expr_arena.get(*init_expr_id).expect("Init expr not found");
                    if let HirExprKind::Call { callee, .. } = &init_expr.kind {
                        let callee_expr = self.expr_arena.get(*callee).expect("Callee expr not found");
                        if let HirExprKind::Var(class_sym) = &callee_expr.kind {
                            if self.class_symbols.contains(class_sym) {
                                // Use resolved class name if available
                                let resolved_name = self.class_names.get(class_sym)
                                    .cloned()
                                    .unwrap_or_else(|| class_sym.as_raw().to_string());
                                let class_type = ClassType {
                                    name: resolved_name,
                                    module: None,
                                    type_params: vec![],
                                    bases: vec![],
                                    members: vec![],
                                    methods: vec![],
                                };
                                actual_ty = Type::Class(class_type.clone());
                                self.name_to_type.insert(*name, Type::Class(class_type));
                            }
                        }
                    }
                }

                // Check if this name already exists (e.g., parameter or outer variable)
                let local_id = if let Some(&existing_id) = self.name_to_local.get(name) {
                    // Reuse existing local (reassignment, not new declaration)
                    // Update the type if we have a more specific one
                    if let Type::Class(_) = &actual_ty {
                        if let Some(local) = self.locals.iter_mut().find(|l| l.id == existing_id) {
                            local.ty = actual_ty.clone();
                        }
                    }
                    existing_id
                } else {
                    // Create new local for first declaration with the actual type
                    let local = self.new_local(Some(*name), actual_ty, *mutable);
                    local.id
                };

                if let Some(init_expr) = init {
                    let init_op = self.build_expr(init_expr);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(local_id),
                            value: MirRvalue::Use(init_op),
                        },
                        span: stmt.span,
                    });
                }
            }
            HirStmtKind::Assign { target, value } => {
                // Check if the value is a constructor call and track the type
                let value_expr = self.expr_arena.get(*value).expect("Value expr not found");
                if let HirExprKind::Call { callee, .. } = &value_expr.kind {
                    let callee_expr = self.expr_arena.get(*callee).expect("Callee expr not found");
                    if let HirExprKind::Var(class_sym) = &callee_expr.kind {
                        // Check if callee is a class (constructor call)
                        if self.class_symbols.contains(class_sym) {
                            // Track that target variable has this class type
                            let target_expr = self.expr_arena.get(*target).expect("Target not found");
                            if let HirExprKind::Var(var_sym) = &target_expr.kind {
                                // Use resolved class name if available
                                let resolved_name = self.class_names.get(class_sym)
                                    .cloned()
                                    .unwrap_or_else(|| class_sym.as_raw().to_string());
                                let class_type = ClassType {
                                    name: resolved_name,
                                    module: None,
                                    type_params: vec![],
                                    bases: vec![],
                                    members: vec![],
                                    methods: vec![],
                                };
                                self.name_to_type.insert(*var_sym, Type::Class(class_type.clone()));
                                // Also update the local's type if it exists
                                if let Some(&local_id) = self.name_to_local.get(var_sym) {
                                    if let Some(local) = self.locals.iter_mut().find(|l| l.id == local_id) {
                                        local.ty = Type::Class(class_type);
                                    }
                                }
                            }
                        }
                    }
                }

                // Build the value expression
                let value_op = self.build_expr(value);

                // Check if target is an attribute access (Field)
                let target_expr = self.expr_arena.get(*target).expect("Expr not found");
                if let HirExprKind::Field { base, field } = &target_expr.kind {
                    // Attribute assignment: obj.attr = value
                    // We borrow the base object, not move it
                    let base_op = self.build_expr_as_copy(base);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::SetAttr {
                            object: base_op,
                            attr: *field,
                            value: value_op,
                        },
                        span: stmt.span,
                    });
                } else {
                    // Check for nested index assignment: obj.attr[idx] = value
                    if let HirExprKind::Index { base: index_base, index } = &target_expr.kind {
                        let index_base_expr = self.expr_arena.get(*index_base).expect("Index base not found");
                        if let HirExprKind::Field { base: field_base, field } = &index_base_expr.kind {
                            // This is obj.attr[idx] = value
                            let obj_op = self.build_expr_as_copy(field_base);
                            let idx_op = self.build_expr(index);
                            self.push_stmt(MirStmt {
                                kind: MirStmtKind::SetAttrIndex {
                                    object: obj_op,
                                    attr: *field,
                                    index: idx_op,
                                    value: value_op,
                                },
                                span: stmt.span,
                            });
                            return;
                        }
                    }
                    // Regular variable/index assignment
                    let target_place = self.build_place(target);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: target_place,
                            value: MirRvalue::Use(value_op),
                        },
                        span: stmt.span,
                    });
                }
            }
            HirStmtKind::Expr(expr_id) => {
                // Expression statement - evaluate for side effects
                let _result = self.build_expr(expr_id);
                // Result is discarded but side effects (like print) happen
            }
            HirStmtKind::Return(expr) => {
                let ret_op = if let Some(ret_expr) = expr {
                    // Build return value
                    Some(self.build_expr(ret_expr))
                } else {
                    None
                };
                self.set_terminator(MirTerminator::Return(ret_op));
            }
            HirStmtKind::If { cond, then_block, else_block } => {
                // Build the condition
                let cond_op = self.build_expr(cond);

                // Get the type of the condition expression - if it's a class type,
                // preserve it so the backend can call __bool__
                let cond_expr = self.expr_arena.get(*cond).expect("Cond expr not found");
                let cond_type = if matches!(cond_expr.ty, Type::Class(_)) {
                    cond_expr.ty.clone()
                } else {
                    Type::Bool
                };

                // Store condition in a temp for the switch
                let cond_temp = self.new_temp(cond_type);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(cond_temp),
                        value: MirRvalue::Use(cond_op),
                    },
                    span: stmt.span,
                });

                let cond_block = self.current_block;

                // Create blocks for then, else (if present), and merge
                let then_bb = self.new_block();

                if let Some(else_blk) = else_block {
                    let else_bb = self.new_block();
                    let merge_bb = self.new_block();

                    // Set up branch in condition block
                    self.blocks[cond_block as usize].terminator = MirTerminator::SwitchInt {
                        discr: MirOperand::Copy(MirPlace::local(cond_temp)),
                        targets: vec![(1, then_bb)], // true -> then
                        otherwise: else_bb,          // false -> else
                    };

                    // Build then block
                    self.current_block = then_bb;
                    self.build_block(then_block);
                    // Jump to merge unless already terminated
                    if matches!(self.blocks[self.current_block as usize].terminator, MirTerminator::Unreachable) {
                        self.set_terminator(MirTerminator::Goto(merge_bb));
                    }

                    // Build else block
                    self.current_block = else_bb;
                    self.build_block(else_blk);
                    if matches!(self.blocks[self.current_block as usize].terminator, MirTerminator::Unreachable) {
                        self.set_terminator(MirTerminator::Goto(merge_bb));
                    }

                    // Continue in merge block
                    self.current_block = merge_bb;
                } else {
                    let merge_bb = self.new_block();

                    // Set up branch in condition block
                    self.blocks[cond_block as usize].terminator = MirTerminator::SwitchInt {
                        discr: MirOperand::Copy(MirPlace::local(cond_temp)),
                        targets: vec![(1, then_bb)], // true -> then
                        otherwise: merge_bb,         // false -> skip to merge
                    };

                    // Build then block
                    self.current_block = then_bb;
                    self.build_block(then_block);
                    if matches!(self.blocks[self.current_block as usize].terminator, MirTerminator::Unreachable) {
                        self.set_terminator(MirTerminator::Goto(merge_bb));
                    }

                    // Continue in merge block
                    self.current_block = merge_bb;
                }
            }
            HirStmtKind::While { cond, body } => {
                // Save current block before creating new blocks
                let entry_block = self.current_block;

                // Create blocks: header (condition), body, exit
                let header_bb = self.new_block();
                let body_bb = self.new_block();
                let exit_bb = self.new_block();

                // Set up loop context for break/continue
                self.loop_stack.push(LoopContext {
                    header_block: header_bb,
                    exit_block: exit_bb,
                });

                // Jump to header from entry block
                self.blocks[entry_block as usize].terminator = MirTerminator::Goto(header_bb);

                // Build header: evaluate condition
                self.current_block = header_bb;
                let cond_op = self.build_expr(cond);

                // Get the type of the condition expression - if it's a class type,
                // preserve it so the backend can call __bool__
                let cond_expr = self.expr_arena.get(*cond).expect("Cond expr not found");
                let cond_type = if matches!(cond_expr.ty, Type::Class(_)) {
                    cond_expr.ty.clone()
                } else {
                    Type::Bool
                };

                let cond_temp = self.new_temp(cond_type);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(cond_temp),
                        value: MirRvalue::Use(cond_op),
                    },
                    span: stmt.span,
                });
                self.set_terminator(MirTerminator::SwitchInt {
                    discr: MirOperand::Copy(MirPlace::local(cond_temp)),
                    targets: vec![(1, body_bb)],
                    otherwise: exit_bb,
                });

                // Build body
                self.current_block = body_bb;
                self.build_block(body);
                if matches!(self.blocks[self.current_block as usize].terminator, MirTerminator::Unreachable) {
                    self.set_terminator(MirTerminator::Goto(header_bb));
                }

                self.loop_stack.pop();

                // Continue from exit
                self.current_block = exit_bb;
            }
            HirStmtKind::For { var, iter, body, .. } => {
                // Build iterator expression (e.g., range(0, 5) or a list)
                let iter_op = self.build_expr(iter);
                let source_local = self.new_temp(Type::Unknown);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(source_local),
                        value: MirRvalue::Use(iter_op),
                    },
                    span: stmt.span,
                });

                // Create iterator local (will hold the wrapped iterator)
                let iter_local = self.new_temp(Type::Unknown);

                // Create loop variable
                let loop_var = self.new_local(Some(*var), Type::Unknown, true);
                let loop_var_id = loop_var.id;

                // Save entry block before creating new ones
                let entry_block = self.current_block;

                // Create blocks: init block to wrap iterator, header, body, exit
                let init_bb = self.new_block();
                let header_bb = self.new_block();
                let body_bb = self.new_block();
                let exit_bb = self.new_block();

                // Set up loop context for break/continue
                self.loop_stack.push(LoopContext {
                    header_block: header_bb,
                    exit_block: exit_bb,
                });

                // Jump to init block from entry block
                self.blocks[entry_block as usize].terminator = MirTerminator::Goto(init_bb);

                // Init block: call __make_iter__ intrinsic to wrap the source into an iterator
                self.current_block = init_bb;
                self.set_terminator(MirTerminator::Call {
                    func: MirOperand::Global(Symbol::MAKE_ITER),
                    args: vec![MirOperand::Move(MirPlace::local(source_local))],
                    destination: MirPlace::local(iter_local),
                    target: Some(header_bb),
                    unwind: None,
                });

                // Header: ForIter handles getting next item and jumping
                self.current_block = header_bb;
                self.set_terminator(MirTerminator::ForIter {
                    iter: MirPlace::local(iter_local),
                    loop_var: loop_var_id,
                    body: body_bb,
                    exit: exit_bb,
                });

                // Body
                self.current_block = body_bb;
                self.build_block(body);
                // After body, jump back to header
                if matches!(self.blocks[self.current_block as usize].terminator, MirTerminator::Unreachable) {
                    self.set_terminator(MirTerminator::Goto(header_bb));
                }

                self.loop_stack.pop();
                // Continue from exit
                self.current_block = exit_bb;
            }
            HirStmtKind::Loop { body } => {
                // Save entry block before creating new ones
                let entry_block = self.current_block;

                // Create blocks for body and exit
                let body_bb = self.new_block();
                let exit_bb = self.new_block();

                // Set up loop context for break/continue
                self.loop_stack.push(LoopContext {
                    header_block: body_bb,
                    exit_block: exit_bb,
                });

                // Jump to body from entry block
                self.blocks[entry_block as usize].terminator = MirTerminator::Goto(body_bb);

                // Build body
                self.current_block = body_bb;
                self.build_block(body);
                // Loop back to start
                if matches!(self.blocks[self.current_block as usize].terminator, MirTerminator::Unreachable) {
                    self.set_terminator(MirTerminator::Goto(body_bb));
                }

                self.loop_stack.pop();

                // Continue from exit
                self.current_block = exit_bb;
            }
            HirStmtKind::Break => {
                // Jump to loop exit
                if let Some(loop_ctx) = self.loop_stack.last() {
                    self.set_terminator(MirTerminator::Goto(loop_ctx.exit_block));
                }
            }
            HirStmtKind::Continue => {
                // Jump to loop header
                if let Some(loop_ctx) = self.loop_stack.last() {
                    self.set_terminator(MirTerminator::Goto(loop_ctx.header_block));
                }
            }
            HirStmtKind::Assert { test, msg } => {
                let cond = self.build_expr(test);
                let msg_str = if let Some(msg_id) = msg {
                     // Try to extract string literal
                     if let Some(expr) = self.expr_arena.get(*msg_id) {
                         if let HirExprKind::Literal(HirLiteral::Str(s)) = &expr.kind {
                             s.clone()
                         } else {
                             "Assertion failed".to_string()
                         }
                     } else {
                         "Assertion failed".to_string()
                     }
                } else {
                    "Assertion failed".to_string()
                };

                let current_bb = self.current_block;
                let target_bb = self.new_block();

                // Set terminator of previous block
                if let Some(block) = self.blocks.get_mut(current_bb as usize) {
                    block.terminator = MirTerminator::Assert {
                        cond,
                        expected: true,
                        target: target_bb,
                        msg: msg_str,
                    };
                }
                // current_block is already target_bb
            }
            HirStmtKind::Match { subject, arms } => {
                self.build_match(subject, arms);
            }
            HirStmtKind::Try { body, handlers, orelse, finalbody } => {
                self.build_try(body, handlers, orelse.as_ref(), finalbody.as_ref());
            }
            HirStmtKind::Raise { exc, cause: _ } => {
                let exc_op = exc.map(|e| self.build_expr(&e));
                self.set_terminator(MirTerminator::Raise { exc: exc_op });
                // Code after raise is unreachable
                let next_bb = self.new_block();
                self.current_block = next_bb;
            }
            _ => {}
        }
    }

    /// Builds MIR for a try/except statement
    fn build_try(
        &mut self,
        body: &HirBlock,
        handlers: &[HirExceptHandler],
        orelse: Option<&HirBlock>,
        finalbody: Option<&HirBlock>,
    ) {
        let entry_bb = self.current_block;

        // Create blocks for try body, handlers, else, finally, and exit
        let try_bb = self.new_block();
        let exit_bb = self.new_block();
        let finally_bb = finalbody.map(|_| self.new_block());
        let else_bb = orelse.map(|_| self.new_block());

        // Create handler blocks
        let handler_blocks: Vec<BlockId> = handlers.iter().map(|_| self.new_block()).collect();

        // Build MIR exception handlers
        let mir_handlers: Vec<MirExceptHandler> = handlers.iter().zip(&handler_blocks).map(|(h, &block)| {
            // For exception type, we just store the name as a symbol if present
            let exc_type = h.exc_type.as_ref().and_then(|e| {
                if let Some(expr) = self.expr_arena.get(*e) {
                    if let HirExprKind::Var(sym) = &expr.kind {
                        Some(*sym)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let exc_var = h.name.map(|name| {
                let local = self.new_local(Some(name), Type::Any, true);
                local.id
            });

            MirExceptHandler {
                exc_type,
                exc_var,
                body: block,
            }
        }).collect();

        // Set entry terminator to start try block
        self.blocks[entry_bb as usize].terminator = MirTerminator::TryBegin {
            body: try_bb,
            handlers: mir_handlers,
            finally: finally_bb,
            exit: exit_bb,
        };

        // Build try body
        self.current_block = try_bb;
        self.build_block(body);

        // Pop exception frame after successful try body execution
        self.push_stmt(MirStmt {
            kind: MirStmtKind::TryEnd,
            span: Span::default(),
        });

        // After try body, jump to else block if present, otherwise to finally or exit
        let after_try = if let Some(else_block) = else_bb {
            else_block
        } else if let Some(finally_block) = finally_bb {
            finally_block
        } else {
            exit_bb
        };
        self.set_terminator(MirTerminator::Goto(after_try));

        // Build handler bodies
        for (handler, &block_id) in handlers.iter().zip(&handler_blocks) {
            self.current_block = block_id;
            self.build_block(&handler.body);
            // Pop exception frame after handler execution
            self.push_stmt(MirStmt {
                kind: MirStmtKind::TryEnd,
                span: Span::default(),
            });
            // After handler, jump to finally or exit
            let after_handler = finally_bb.unwrap_or(exit_bb);
            self.set_terminator(MirTerminator::Goto(after_handler));
        }

        // Build else block if present
        if let (Some(else_block), Some(else_body)) = (else_bb, orelse) {
            self.current_block = else_block;
            self.build_block(else_body);
            let after_else = finally_bb.unwrap_or(exit_bb);
            self.set_terminator(MirTerminator::Goto(after_else));
        }

        // Build finally block if present
        if let (Some(finally_block), Some(finally_body)) = (finally_bb, finalbody) {
            self.current_block = finally_block;
            self.build_block(finally_body);
            self.set_terminator(MirTerminator::Goto(exit_bb));
        }

        // Continue from exit block
        self.current_block = exit_bb;
    }

    /// Builds MIR for a match statement.
    /// This lowers pattern matching to a chain of conditional checks.
    fn build_match(&mut self, subject: &HirExprId, arms: &[HirMatchArm]) {
        let subject_op = self.build_expr(subject);

        // Create a local to hold the subject value
        let subject_local = self.new_temp(Type::Unknown);
        self.push_stmt(MirStmt {
            kind: MirStmtKind::Assign {
                place: MirPlace::local(subject_local),
                value: MirRvalue::Use(subject_op),
            },
            span: roast_common::Span::default(),
        });

        // Save the dispatch block (where we'll set up the jump to first check)
        let dispatch_bb = self.current_block;

        // Collect pattern info (without cloning)
        #[derive(Clone)]
        enum PatternDispatch {
            Literal(i128),         // Integer literal to compare
            BoolLiteral(bool),     // Boolean literal
            StringLiteral(String), // String literal
            Wildcard,              // Matches anything (_) or binding
        }

        let pattern_kinds: Vec<PatternDispatch> = arms.iter().map(|arm| {
            match &arm.pattern.kind {
                HirPatternKind::Literal(lit) => match lit {
                    HirLiteral::Int(v) => PatternDispatch::Literal(*v),
                    HirLiteral::Bool(b) => PatternDispatch::BoolLiteral(*b),
                    HirLiteral::Str(s) => PatternDispatch::StringLiteral(s.clone()),
                    _ => PatternDispatch::Wildcard,
                },
                HirPatternKind::Wildcard | HirPatternKind::Binding(_) => PatternDispatch::Wildcard,
                _ => PatternDispatch::Wildcard,
            }
        }).collect();

        // First pass: create all arm body blocks and build their bodies
        let mut arm_body_blocks: Vec<BlockId> = Vec::new(); // Entry blocks for each arm
        let mut arm_exit_blocks: Vec<BlockId> = Vec::new(); // Exit blocks for each arm (may differ due to control flow)

        for (i, arm) in arms.iter().enumerate() {
            let body_bb = self.new_block();
            arm_body_blocks.push(body_bb);

            // Bind pattern variables
            self.bind_pattern_vars(&arm.pattern, subject_local);

            // Build arm body
            self.build_block(&arm.body);

            // Track where the arm body ends (might be different from body_bb due to control flow)
            arm_exit_blocks.push(self.current_block);
        }

        // Create the exit block
        let exit_bb = self.new_block();

        // Terminate all arm exit blocks with goto exit (if still unreachable)
        for exit_block in &arm_exit_blocks {
            if let Some(block) = self.blocks.get_mut(*exit_block as usize) {
                if matches!(block.terminator, MirTerminator::Unreachable) {
                    block.terminator = MirTerminator::Goto(exit_bb);
                }
            }
        }

        // Find the first wildcard arm - it becomes the "otherwise" case
        let mut otherwise_bb = exit_bb;
        for (i, pk) in pattern_kinds.iter().enumerate() {
            if matches!(pk, PatternDispatch::Wildcard) {
                otherwise_bb = arm_body_blocks[i];
                break;
            }
        }

        // Build comparison blocks for literal patterns
        let mut check_blocks: Vec<(BlockId, usize)> = Vec::new(); // (check_block, arm_index)

        for (i, pk) in pattern_kinds.iter().enumerate() {
            match pk {
                PatternDispatch::Literal(_) | PatternDispatch::BoolLiteral(_) | PatternDispatch::StringLiteral(_) => {
                    let check_bb = self.new_block();
                    check_blocks.push((check_bb, i));
                }
                _ => {}
            }
        }

        // Set terminators for check blocks
        // Each check block compares and jumps to arm body or next check
        for (idx, &(check_bb, arm_idx)) in check_blocks.iter().enumerate() {
            let arm_body_bb = arm_body_blocks[arm_idx];
            let next_target = if idx + 1 < check_blocks.len() {
                check_blocks[idx + 1].0 // Next check block
            } else {
                otherwise_bb // No more checks, go to wildcard or exit
            };

            // Build the comparison in this block
            self.current_block = check_bb;

            let cmp_result = self.new_temp(Type::Bool);

            match &pattern_kinds[arm_idx] {
                PatternDispatch::Literal(val) => {
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(cmp_result),
                            value: MirRvalue::BinaryOp(
                                MirBinOp::Eq,
                                MirOperand::Copy(MirPlace::local(subject_local)),
                                MirOperand::Constant(MirConstant::Int(*val)),
                            ),
                        },
                        span: roast_common::Span::default(),
                    });
                }
                PatternDispatch::BoolLiteral(val) => {
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(cmp_result),
                            value: MirRvalue::BinaryOp(
                                MirBinOp::Eq,
                                MirOperand::Copy(MirPlace::local(subject_local)),
                                MirOperand::Constant(MirConstant::Bool(*val)),
                            ),
                        },
                        span: roast_common::Span::default(),
                    });
                }
                PatternDispatch::StringLiteral(val) => {
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(cmp_result),
                            value: MirRvalue::BinaryOp(
                                MirBinOp::Eq,
                                MirOperand::Copy(MirPlace::local(subject_local)),
                                MirOperand::Constant(MirConstant::Str(val.clone())),
                            ),
                        },
                        span: roast_common::Span::default(),
                    });
                }
                _ => unreachable!(),
            }

            // Set terminator: if comparison is true, go to arm body, else next check
            self.set_terminator(MirTerminator::SwitchInt {
                discr: MirOperand::Copy(MirPlace::local(cmp_result)),
                targets: vec![(1, arm_body_bb)],
                otherwise: next_target,
            });
        }

        // The first check block or otherwise (if no literal patterns)
        let first_check = if !check_blocks.is_empty() {
            check_blocks[0].0
        } else {
            otherwise_bb
        };

        // Set up the dispatch block to jump to the first check
        if let Some(block) = self.blocks.get_mut(dispatch_bb as usize) {
            if matches!(block.terminator, MirTerminator::Unreachable) {
                block.terminator = MirTerminator::Goto(first_check);
            }
        }

        // Make sure we're at the exit block for subsequent code
        self.current_block = exit_bb;
    }

    /// Binds pattern variables to the matched value.
    fn bind_pattern_vars(&mut self, pattern: &HirPattern, subject_local: LocalId) {
        match &pattern.kind {
            HirPatternKind::Binding(name) => {
                let local = self.new_local(Some(*name), pattern.ty.clone(), false);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(local.id),
                        value: MirRvalue::Use(MirOperand::Copy(MirPlace::local(subject_local))),
                    },
                    span: pattern.span,
                });
            }
            HirPatternKind::Tuple(patterns) => {
                for (i, p) in patterns.iter().enumerate() {
                    // Create projection for tuple element
                    let elem_local = self.new_temp(p.ty.clone());
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(elem_local),
                            value: MirRvalue::Use(MirOperand::Copy(MirPlace {
                                local: subject_local,
                                projections: smallvec![MirProjection::Field(i as u32)],
                            })),
                        },
                        span: p.span,
                    });
                    self.bind_pattern_vars(p, elem_local);
                }
            }
            HirPatternKind::Struct { fields, .. } => {
                for (i, (_, p)) in fields.iter().enumerate() {
                    let field_local = self.new_temp(p.ty.clone());
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(field_local),
                            value: MirRvalue::Use(MirOperand::Copy(MirPlace {
                                local: subject_local,
                                projections: smallvec![MirProjection::Field(i as u32)],
                            })),
                        },
                        span: p.span,
                    });
                    self.bind_pattern_vars(p, field_local);
                }
            }
            HirPatternKind::Or(patterns) => {
                // For OR patterns, just bind the first matching one
                // In reality, all branches must bind the same names
                if let Some(first) = patterns.first() {
                    self.bind_pattern_vars(first, subject_local);
                }
            }
            HirPatternKind::Wildcard | HirPatternKind::Literal(_) => {
                // No bindings needed
            }
        }
    }

    fn build_expr(&mut self, expr_id: &HirExprId) -> MirOperand {
        let expr = self.expr_arena.get(*expr_id).expect("Expr not found");
        match &expr.kind {
            HirExprKind::Literal(lit) => {
                let constant = match lit {
                    HirLiteral::Int(n) => MirConstant::Int(*n),
                    HirLiteral::Float(f) => MirConstant::Float(*f),
                    HirLiteral::Bool(b) => MirConstant::Bool(*b),
                    HirLiteral::Str(s) => MirConstant::Str(s.clone()),
                    HirLiteral::Bytes(b) => MirConstant::Bytes(b.clone()),
                    HirLiteral::None => MirConstant::None,
                };
                MirOperand::Constant(constant)
            }
            HirExprKind::Var(name) => {
                // Look up variable in locals map
                if let Some(&local_id) = self.name_to_local.get(name) {
                    // Get the local's type to determine Copy vs Move
                    // We use the local's type rather than expr.ty because the local
                    // has the canonical type from when it was declared
                    let local_ty = &self.locals[local_id as usize].ty;
                    let ownership_info = self.ownership_analyzer.analyze(local_ty);
                    if ownership_info.is_copy() {
                        MirOperand::Copy(MirPlace::local(local_id))
                    } else {
                        MirOperand::Move(MirPlace::local(local_id))
                    }
                } else {
                    // Global/builtin - emit as a global reference
                    MirOperand::Global(*name)
                }
            }
            HirExprKind::Call { callee, args } => {
                // Check if this is a method call (callee is a field access)
                let callee_expr = self.expr_arena.get(*callee).expect("callee expression");

                // Create a temp for the result
                let result_temp = self.new_temp(expr.ty.clone());

                if let HirExprKind::Field { base, field } = &callee_expr.kind {
                    // This is a method call: obj.method(args)
                    // Get the receiver's type to determine the class for static dispatch
                    let base_expr = self.expr_arena.get(*base).expect("base expression");

                    // Check if this is a super().method() call
                    // Supports both super() and super(ClassName, self) forms
                    let (is_super_call, super_class_override) = if let HirExprKind::Call { callee: super_callee, args: super_args } = &base_expr.kind {
                        let super_callee_expr = self.expr_arena.get(*super_callee).expect("super callee");
                        if let HirExprKind::Var(var_name) = &super_callee_expr.kind {
                            let is_super = self.super_symbol.map_or(false, |s| *var_name == s);
                            if is_super {
                                if super_args.is_empty() {
                                    // super() - no arguments, use current class's MRO
                                    (true, None)
                                } else if super_args.len() == 2 {
                                    // super(ClassName, self) - use ClassName to find position in MRO
                                    let first_arg = self.expr_arena.get(super_args[0]).expect("first super arg");
                                    if let HirExprKind::Var(class_name_sym) = &first_arg.kind {
                                        // The class name from super(ClassName, self)
                                        // We need to look up this class in MRO and return the next one
                                        (true, Some(*class_name_sym))
                                    } else {
                                        (true, None)
                                    }
                                } else {
                                    (false, None)
                                }
                            } else {
                                (false, None)
                            }
                        } else {
                            (false, None)
                        }
                    } else {
                        (false, None)
                    };

                    // For super() calls, use the next class in MRO for static dispatch
                    let receiver_class = if is_super_call && !self.class_mro.is_empty() {
                        // Find next class in MRO after current class (or specified class)
                        // MRO is [CurrentClass, Parent1, Parent2, ...]
                        // super() should resolve to the next class after current
                        if let Some(_class_sym) = super_class_override {
                            // For super(ClassName, self), find ClassName in MRO and return next
                            // For now, just use the standard MRO lookup
                            // TODO: properly resolve the class name symbol to string
                            self.get_next_in_mro()
                        } else {
                            self.get_next_in_mro()
                        }
                    } else if let Type::Class(class_type) = &base_expr.ty {
                        Some(class_type.name.clone())
                    } else if let HirExprKind::Var(name) = &base_expr.kind {
                        // Look up type from our tracking
                        if let Some(Type::Class(class_type)) = self.name_to_type.get(name) {
                            Some(class_type.name.clone())
                        } else {
                            None
                        }
                    } else {
                        None  // Dynamic dispatch will be needed
                    };

                    // Build receiver: for super(), we need to pass 'self' not the super proxy
                    // because the parent method expects 'self' (the actual object)
                    let receiver = if is_super_call {
                        // For super().method(), pass 'self' (the first param, %v0) as receiver
                        // The super proxy is just for lookup; the method operates on self
                        MirOperand::Copy(MirPlace::local(0))  // v0 is always self
                    } else {
                        self.build_expr_as_copy(&base)
                    };

                    // Build arguments (not including self - receiver is passed separately)
                    let arg_ops: Vec<_> = args.iter()
                        .map(|arg| self.build_expr_as_copy(arg))
                        .collect();

                    // NOW get return block and create next block
                    let return_block = self.current_block;
                    let next_block = self.new_block();
                    // Restore current_block to return_block so terminator is set correctly
                    self.current_block = return_block;

                    self.blocks[return_block as usize].terminator = MirTerminator::MethodCall {
                        receiver,
                        receiver_class,
                        receiver_type: Some(base_expr.ty.clone()),
                        method: *field,
                        args: arg_ops,
                        destination: MirPlace::local(result_temp),
                        target: Some(next_block),
                    };

                    // Continue in the next block after the call
                    self.current_block = next_block;
                } else {
                    // Regular function call
                    // Build callee and args BEFORE creating next_block
                    let callee_op = self.build_expr(callee);

                    // Build all arguments
                    let arg_ops: Vec<_> = args.iter()
                        .map(|arg| self.build_expr_as_copy(arg))
                        .collect();

                    // NOW get return block and create next block
                    let return_block = self.current_block;
                    let next_block = self.new_block();
                    // Restore current_block to return_block so terminator is set correctly
                    self.current_block = return_block;

                    self.blocks[return_block as usize].terminator = MirTerminator::Call {
                        func: callee_op,
                        args: arg_ops,
                        destination: MirPlace::local(result_temp),
                        target: Some(next_block),
                        unwind: None,
                    };

                    // Continue in the next block after the call
                    self.current_block = next_block;
                }

                MirOperand::Move(MirPlace::local(result_temp))
            }
            HirExprKind::Binary { op, left, right } => {
                // All binary operations borrow their operands instead of moving them.
                // This matches Python semantics where `a + b` or `a == b` doesn't consume a or b.
                // The operands are borrowed (copied if Copy type), allowing reuse after the operation.
                let l = self.build_expr_as_copy(left);
                let r = self.build_expr_as_copy(right);

                let mir_op = match op {
                    HirBinOp::Add => MirBinOp::Add,
                    HirBinOp::Sub => MirBinOp::Sub,
                    HirBinOp::Mul => MirBinOp::Mul,
                    HirBinOp::Div => MirBinOp::Div,
                    HirBinOp::FloorDiv => MirBinOp::FloorDiv,
                    HirBinOp::Mod => MirBinOp::Rem,
                    HirBinOp::Pow => MirBinOp::Pow,
                    HirBinOp::Eq => MirBinOp::Eq,
                    HirBinOp::Ne => MirBinOp::Ne,
                    HirBinOp::Lt => MirBinOp::Lt,
                    HirBinOp::Le => MirBinOp::Le,
                    HirBinOp::Gt => MirBinOp::Gt,
                    HirBinOp::Ge => MirBinOp::Ge,
                    HirBinOp::In => MirBinOp::In,
                    HirBinOp::NotIn => MirBinOp::NotIn,
                    HirBinOp::BitAnd => MirBinOp::BitAnd,
                    HirBinOp::BitOr => MirBinOp::BitOr,
                    HirBinOp::BitXor => MirBinOp::BitXor,
                    HirBinOp::Shl => MirBinOp::Shl,
                    HirBinOp::Shr => MirBinOp::Shr,
                    // Logical And/Or need special handling for short-circuit evaluation
                    HirBinOp::And | HirBinOp::Or => {
                        // For now treat as bitwise - real impl needs branching
                        if matches!(op, HirBinOp::And) {
                            MirBinOp::BitAnd
                        } else {
                            MirBinOp::BitOr
                        }
                    }
                    _ => MirBinOp::Add, // Fallback for Is/IsNot
                };

                let temp = self.new_temp(expr.ty.clone());
                let rvalue = MirRvalue::BinaryOp(mir_op, l, r);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: rvalue,
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Unary { op, operand } => {
                let inner = self.build_expr(operand);
                let mir_op = match op {
                    HirUnaryOp::Neg => MirUnaryOp::Neg,
                    HirUnaryOp::Not => MirUnaryOp::Not,
                    HirUnaryOp::BitNot => MirUnaryOp::BitNot,
                };
                let temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::UnaryOp(mir_op, inner),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Tuple(elements) => {
                let mut ops = Vec::with_capacity(elements.len());
                for elem in elements {
                    ops.push(self.build_expr(elem));
                }
                let temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Aggregate(MirAggregateKind::Tuple, ops),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::List(elements) => {
                let mut ops = Vec::with_capacity(elements.len());
                for elem in elements {
                    ops.push(self.build_expr(elem));
                }
                let temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Aggregate(MirAggregateKind::List, ops),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Dict(pairs) => {
                let mut ops = Vec::with_capacity(pairs.len() * 2);
                for (k, v) in pairs {
                    ops.push(self.build_expr(k));
                    ops.push(self.build_expr(v));
                }
                let temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Aggregate(MirAggregateKind::Dict, ops),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Set(elements) => {
                let mut ops = Vec::with_capacity(elements.len());
                for elem in elements {
                    ops.push(self.build_expr(elem));
                }
                let temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Aggregate(MirAggregateKind::Set, ops),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Slice { lower, upper, step } => {
                // Build a slice object with optional lower, upper, step
                // For None values, we'll use MirConstant::None
                let lower_op = lower.as_ref().map(|e| self.build_expr(e))
                    .unwrap_or(MirOperand::Constant(MirConstant::None));
                let upper_op = upper.as_ref().map(|e| self.build_expr(e))
                    .unwrap_or(MirOperand::Constant(MirConstant::None));
                let step_op = step.as_ref().map(|e| self.build_expr(e))
                    .unwrap_or(MirOperand::Constant(MirConstant::None));

                let temp = self.new_temp(Type::Unknown);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Aggregate(MirAggregateKind::Slice, vec![lower_op, upper_op, step_op]),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Index { base, index } => {
                // Load base[index] - borrow the base, don't move it
                // This matches Python semantics where indexing doesn't consume the container
                let base_op = self.build_expr_as_copy(base);

                // Get the base expression's type
                let base_expr = self.expr_arena.get(*base).expect("base expression");
                let mut base_ty = base_expr.ty.clone();

                // If base is a variable, check if we have a tracked class type for it
                if let HirExprKind::Var(sym) = &base_expr.kind {
                    if let Some(tracked_ty) = self.name_to_type.get(sym) {
                        base_ty = tracked_ty.clone();
                    }
                }

                // Store base in temp with its actual type
                let base_temp = self.new_temp(base_ty);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(base_temp),
                        value: MirRvalue::Use(base_op),
                    },
                    span: expr.span,
                });

                // Check if the index is a slice expression
                let index_expr = self.expr_arena.get(*index).expect("index expression");
                if let HirExprKind::Slice { lower, upper, step } = &index_expr.kind {
                    // Build slice bounds
                    let lower_op = lower.as_ref().map(|e| self.build_expr(e))
                        .unwrap_or(MirOperand::Constant(MirConstant::None));
                    let upper_op = upper.as_ref().map(|e| self.build_expr(e))
                        .unwrap_or(MirOperand::Constant(MirConstant::None));
                    let step_op = step.as_ref().map(|e| self.build_expr(e))
                        .unwrap_or(MirOperand::Constant(MirConstant::None));

                    // Store each bound in temps
                    let lower_temp = self.new_temp(Type::Unknown);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(lower_temp),
                            value: MirRvalue::Use(lower_op),
                        },
                        span: expr.span,
                    });
                    let upper_temp = self.new_temp(Type::Unknown);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(upper_temp),
                            value: MirRvalue::Use(upper_op),
                        },
                        span: expr.span,
                    });
                    let step_temp = self.new_temp(Type::Unknown);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(step_temp),
                            value: MirRvalue::Use(step_op),
                        },
                        span: expr.span,
                    });

                    // Return place with slice projection
                    MirOperand::Copy(MirPlace {
                        local: base_temp,
                        projections: smallvec![MirProjection::Slice {
                            lower: lower_temp,
                            upper: upper_temp,
                            step: step_temp
                        }],
                    })
                } else {
                    // Regular index
                    let index_op = self.build_expr_as_copy(index);
                    // Store index in temp
                    let index_temp = self.new_temp(Type::Unknown);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(index_temp),
                            value: MirRvalue::Use(index_op),
                        },
                        span: expr.span,
                    });
                    // Return place with index projection
                    MirOperand::Copy(MirPlace {
                        local: base_temp,
                        projections: smallvec![MirProjection::Index(index_temp)],
                    })
                }
            }
            HirExprKind::Field { base, field } => {
                // For field access, we borrow the base, not move it
                // So we build the base as a Copy operand regardless of type
                let base_op = self.build_expr_as_copy(base);
                // Use expr.ty to preserve the field's type information
                let result_temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(result_temp),
                        value: MirRvalue::Attr(base_op, *field),
                    },
                    span: expr.span,
                });
                MirOperand::Copy(MirPlace::local(result_temp))
            }
            // Try expression (? operator): unwrap Result/Option or early return
            HirExprKind::Try { expr: inner_expr } => {
                let inner_op = self.build_expr(inner_expr);
                let result_temp = self.new_temp(expr.ty.clone());

                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(result_temp),
                        value: MirRvalue::Use(inner_op),
                    },
                    span: expr.span,
                });

                let success_block = self.new_block();
                let error_block = self.new_block();
                let continue_block = self.new_block();

                let current = self.current_block;
                self.blocks[current as usize].terminator = MirTerminator::SwitchInt {
                    discr: MirOperand::Copy(MirPlace::local(result_temp)),
                    targets: vec![(1, success_block)],
                    otherwise: error_block,
                };

                self.current_block = error_block;
                self.set_terminator(MirTerminator::Return(None));

                self.current_block = success_block;
                let unwrapped = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(unwrapped),
                        value: MirRvalue::Use(MirOperand::Move(MirPlace::local(result_temp))),
                    },
                    span: expr.span,
                });
                self.set_terminator(MirTerminator::Goto(continue_block));

                self.current_block = continue_block;
                MirOperand::Move(MirPlace::local(unwrapped))
            }
            HirExprKind::ListComp { element, pattern, iter, condition } => {
                // List comprehension: [element for pattern in iter if condition]
                // Desugar to: create empty list, iterate, optionally filter, append element

                // 1. Create empty list
                let list_temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(list_temp),
                        value: MirRvalue::Aggregate(MirAggregateKind::List, vec![]),
                    },
                    span: expr.span,
                });

                // 2. Build iterator expression - first get the source
                let iter_op = self.build_expr(iter);
                let source_local = self.new_temp(Type::Unknown);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(source_local),
                        value: MirRvalue::Use(iter_op),
                    },
                    span: expr.span,
                });

                // 2b. Create iterator local (will hold the wrapped iterator)
                let iter_local = self.new_temp(Type::Unknown);

                // 3. Create loop variable based on pattern
                let loop_var = match &pattern.kind {
                    HirPatternKind::Binding(name) => {
                        self.new_local(Some(*name), pattern.ty.clone(), true)
                    }
                    _ => self.new_local(None, pattern.ty.clone(), true),
                };
                let loop_var_id = loop_var.id;

                // Save entry block
                let entry_block = self.current_block;

                // Create blocks
                let init_bb = self.new_block();  // For iterator creation
                let header_bb = self.new_block();
                let body_bb = self.new_block();
                let append_bb = self.new_block();
                let exit_bb = self.new_block();

                // Jump to init block from entry
                self.blocks[entry_block as usize].terminator = MirTerminator::Goto(init_bb);

                // Init block: call __make_iter__ to wrap the source into an iterator
                self.current_block = init_bb;
                self.set_terminator(MirTerminator::Call {
                    func: MirOperand::Global(Symbol::MAKE_ITER),
                    args: vec![MirOperand::Move(MirPlace::local(source_local))],
                    destination: MirPlace::local(iter_local),
                    target: Some(header_bb),
                    unwind: None,
                });
                // Header: ForIter handles getting next item
                self.current_block = header_bb;
                self.set_terminator(MirTerminator::ForIter {
                    iter: MirPlace::local(iter_local),
                    loop_var: loop_var_id,
                    body: body_bb,
                    exit: exit_bb,
                });

                // Body: bind pattern vars (for tuple unpacking etc)
                self.current_block = body_bb;
                if !matches!(pattern.kind, HirPatternKind::Binding(_)) {
                    self.bind_pattern_vars(pattern, loop_var_id);
                }

                // Check condition if present
                if let Some(cond_expr) = condition {
                    let cond_op = self.build_expr(cond_expr);
                    let cond_temp = self.new_temp(Type::Bool);
                    self.push_stmt(MirStmt {
                        kind: MirStmtKind::Assign {
                            place: MirPlace::local(cond_temp),
                            value: MirRvalue::Use(cond_op),
                        },
                        span: expr.span,
                    });
                    // If false, skip to header (continue loop)
                    self.set_terminator(MirTerminator::SwitchInt {
                        discr: MirOperand::Copy(MirPlace::local(cond_temp)),
                        targets: vec![(1, append_bb)],
                        otherwise: header_bb,
                    });
                } else {
                    // No condition, go directly to append
                    self.set_terminator(MirTerminator::Goto(append_bb));
                }

                // Append block: build element and append to list
                self.current_block = append_bb;
                let elem_op = self.build_expr(element);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::ListAppend {
                        list: MirPlace::local(list_temp),
                        value: elem_op,
                    },
                    span: expr.span,
                });
                self.set_terminator(MirTerminator::Goto(header_bb));

                // Continue from exit
                self.current_block = exit_bb;
                MirOperand::Move(MirPlace::local(list_temp))
            }
            HirExprKind::Lambda { params, body } => {
                // Create a sub-MIR body for the lambda
                // We need to build the lambda as a separate function body

                // Create a new MirBuilder for the lambda
                let mut lambda_builder = MirBuilder::new(self.expr_arena);

                // Create entry block for the lambda
                lambda_builder.new_block();

                // Add parameters as locals in the lambda's context and collect param MirParams
                let param_symbols: Vec<Symbol> = params.iter().map(|p| p.name).collect();
                let mut param_mir_params: Vec<MirParam> = Vec::new();
                for param in params {
                    let local = lambda_builder.new_local(Some(param.name), param.ty.clone(), false);
                    lambda_builder.name_to_local.insert(param.name, local.id);
                    param_mir_params.push(MirParam {
                        local,
                        kind: MirParamKind::Regular,
                    });
                }

                // Build the body expression in the lambda's context
                let body_operand = lambda_builder.build_expr(body);

                // Add return statement for the body
                lambda_builder.blocks[lambda_builder.current_block as usize].terminator =
                    MirTerminator::Return(Some(body_operand));

                // Construct the lambda's MirBody
                let lambda_body = MirBody {
                    name: Symbol::from_raw(0), // Anonymous
                    params: param_mir_params,
                    locals: lambda_builder.locals,
                    blocks: lambda_builder.blocks,
                    return_ty: Type::Unknown,
                    span: expr.span,
                };

                // Create an aggregate for the lambda
                let temp = self.new_temp(Type::Unknown);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Aggregate(
                            MirAggregateKind::Lambda {
                                params: param_symbols,
                                body: Box::new(lambda_body),
                            },
                            vec![],
                        ),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            HirExprKind::Await { value } => {
                // Build the awaited value
                let awaited = self.build_expr(value);

                // Create a temp to hold the awaited value
                let temp = self.new_temp(expr.ty.clone());
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(temp),
                        value: MirRvalue::Await(awaited),
                    },
                    span: expr.span,
                });
                MirOperand::Move(MirPlace::local(temp))
            }
            _ => MirOperand::Constant(MirConstant::None),
        }
    }

    /// Build an expression, but always use Copy (not Move) for variable reads.
    /// Used for contexts like field access where we borrow, not move.
    fn build_expr_as_copy(&mut self, expr_id: &HirExprId) -> MirOperand {
        let expr = self.expr_arena.get(*expr_id).expect("Expr not found");
        match &expr.kind {
            HirExprKind::Var(name) => {
                // Look up variable in locals map - always use Copy
                if let Some(&local_id) = self.name_to_local.get(name) {
                    MirOperand::Copy(MirPlace::local(local_id))
                } else {
                    MirOperand::Global(*name)
                }
            }
            // For other expression kinds, delegate to normal build_expr
            _ => self.build_expr(expr_id),
        }
    }

    /// Build a place expression (for assignment targets)
    fn build_place(&mut self, expr_id: &HirExprId) -> MirPlace {
        let expr = self.expr_arena.get(*expr_id).expect("Expr not found");
        match &expr.kind {
            HirExprKind::Var(name) => {
                if let Some(&local_id) = self.name_to_local.get(name) {
                    MirPlace::local(local_id)
                } else {
                    // Create a new local for undefined variable
                    let local = self.new_local(Some(*name), expr.ty.clone(), true);
                    MirPlace::local(local.id)
                }
            }
            HirExprKind::Index { base, index } => {
                let base_place = self.build_place(base);
                let index_op = self.build_expr(index);
                let index_temp = self.new_temp(Type::Unknown);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: MirPlace::local(index_temp),
                        value: MirRvalue::Use(index_op),
                    },
                    span: expr.span,
                });
                MirPlace {
                    local: base_place.local,
                    projections: {
                        let mut projs = base_place.projections;
                        projs.push(MirProjection::Index(index_temp));
                        projs
                    },
                }
            }
            _ => {
                // For complex targets, fall back to creating a temp
                let temp = self.new_temp(expr.ty.clone());
                MirPlace::local(temp)
            }
        }
    }

    fn new_temp(&mut self, ty: Type) -> LocalId {
        let local = self.new_local(None, ty, false);
        local.id
    }
}

// Removed Default impl as it requires arena
