//! HIR builder - converts AST to HIR.

use crate::nodes::*;
use id_arena::Arena;
use roast_ast::*;
use roast_common::{DiagnosticSink, Interner, Span, Symbol};
use roast_typer::{Type, TypeContext, TypeChecker, compute_mro};
use std::collections::HashMap;

/// HIR builder context.
pub struct HirBuilder<'a> {
    interner: &'a Interner,
    type_ctx: &'a mut TypeContext,
    diagnostics: &'a mut DiagnosticSink,
    expr_arena: Arena<HirExpr>,
    /// Current class name (for private name mangling)
    current_class: Option<String>,
    /// MROs for all classes built so far (for multiple inheritance resolution)
    class_mros: HashMap<String, Vec<String>>,
}

impl<'a> HirBuilder<'a> {
    pub fn new(
        interner: &'a Interner,
        type_ctx: &'a mut TypeContext,
        diagnostics: &'a mut DiagnosticSink,
    ) -> Self {
        Self {
            interner,
            type_ctx,
            diagnostics,
            expr_arena: Arena::new(),
            current_class: None,
            class_mros: HashMap::new(),
        }
    }

    pub fn expr_arena(&self) -> &Arena<HirExpr> {
        &self.expr_arena
    }

    /// Builds HIR for a module.
    pub fn build_module(&mut self, module: &Module) -> HirModule {
        let mut items = Vec::new();
        let mut top_level_stmts: Vec<&Stmt> = Vec::new();

        for stmt in &module.body {
            if let Some(item) = self.build_item(stmt) {
                items.push(item);
            } else {
                // Collect top-level statements that aren't function/class definitions
                top_level_stmts.push(stmt);
            }
        }

        // If there are top-level statements, wrap them in a __module_init__ function
        if !top_level_stmts.is_empty() {
            let init_func = self.build_module_init(&top_level_stmts, module.span);
            items.push(HirItem::Function(init_func));
        }

        HirModule {
            name: module.name.clone(),
            items,
        }
    }

    /// Builds a synthetic __module_init__ function for top-level statements.
    fn build_module_init(&mut self, stmts: &[&Stmt], span: Span) -> HirFunction {
        // Intern the name
        let name = self.interner.intern("__module_init__");

        // Build body from top-level statements
        let hir_stmts: Vec<_> = stmts.iter()
            .flat_map(|stmt| self.build_stmt(stmt))
            .collect();

        let body = HirBlock {
            stmts: hir_stmts,
            span,
        };

        HirFunction {
            name,
            params: Vec::new(),
            return_type: Type::NoneType,
            body,
            is_async: false,
            decorators: Vec::new(),
            span,
        }
    }

    fn build_item(&mut self, stmt: &Stmt) -> Option<HirItem> {
        match &stmt.kind {
            StmtKind::FunctionDef {
                name,
                args,
                body,
                returns,
                decorators,
                is_async,
                ..
            } => {
                let func = self.build_function(name, args, body, returns.as_deref(), decorators, *is_async, stmt.span);
                Some(HirItem::Function(func))
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                type_params,
                decorators,
                ..
            } => {
                let class = self.build_class(name, bases, body, type_params, decorators, stmt.span);
                Some(HirItem::Class(class))
            }
            StmtKind::Import { names } => {
                let import = HirImport {
                    module: String::new(),
                    names: names.iter().map(|a| (a.name.name, a.asname.as_ref().map(|n| n.name))).collect(),
                    span: stmt.span,
                };
                Some(HirItem::Import(import))
            }
            StmtKind::TypeAlias { name, value, .. } => {
                let ty = self.resolve_type(value);
                Some(HirItem::TypeAlias(HirTypeAlias {
                    name: name.name,
                    ty,
                    span: stmt.span,
                }))
            }
            _ => None,
        }
    }

    fn build_function(
        &mut self,
        name: &Ident,
        args: &Arguments,
        body: &[Stmt],
        returns: Option<&TypeExpr>,
        decorators: &[Decorator],
        is_async: bool,
        span: Span,
    ) -> HirFunction {
        // Build regular params
        let mut params: Vec<_> = args.args.iter().map(|arg| {
            let ty = arg.annotation.as_ref()
                .map(|ann| self.resolve_type(ann))
                .unwrap_or(Type::Any);
            let default = arg.default.as_ref().map(|d| self.build_expr(d));
            HirParam {
                name: arg.name.name,
                ty,
                default,
                kind: HirParamKind::Regular,
                span: arg.span,
            }
        }).collect();

        // Add *args if present
        if let Some(vararg) = &args.vararg {
            let ty = vararg.annotation.as_ref()
                .map(|ann| self.resolve_type(ann))
                .unwrap_or(Type::Any);
            params.push(HirParam {
                name: vararg.name.name,
                ty: Type::Tuple(vec![ty]),
                default: None,
                kind: HirParamKind::VarPositional,
                span: vararg.span,
            });
        }

        // Add **kwargs if present
        if let Some(kwarg) = &args.kwarg {
            let ty = kwarg.annotation.as_ref()
                .map(|ann| self.resolve_type(ann))
                .unwrap_or(Type::Any);
            params.push(HirParam {
                name: kwarg.name.name,
                ty: Type::dict(Type::Str, ty),
                default: None,
                kind: HirParamKind::VarKeyword,
                span: kwarg.span,
            });
        }

        // Build decorators
        let hir_decorators: Vec<HirDecorator> = decorators.iter().map(|d| {
            self.build_decorator(d)
        }).collect();

        let return_type = returns.map(|r| self.resolve_type(r)).unwrap_or(Type::NoneType);
        let hir_body = self.build_block(body, span);

        HirFunction {
            name: name.name,
            params,
            return_type,
            body: hir_body,
            is_async,
            decorators: hir_decorators,
            span,
        }
    }

    fn build_decorator(&mut self, decorator: &Decorator) -> HirDecorator {
        // Extract decorator name from expression
        let name = match &decorator.name.kind {
            ExprKind::Name { id, .. } => id.name,
            ExprKind::Attribute { value: _, attr, .. } => attr.name,
            _ => Symbol::UNRESOLVED,
        };

        let args: Vec<HirExprId> = decorator.arguments.iter()
            .map(|a| self.build_expr(a))
            .collect();

        HirDecorator {
            name,
            args,
            span: decorator.span,
        }
    }

    fn build_class(
        &mut self,
        name: &Ident,
        bases: &[Expr],
        body: &[Stmt],
        type_params: &[TypeParam],
        decorators: &[Decorator],
        span: Span,
    ) -> HirClass {
        // Get class name for private name mangling
        let class_name = self.interner.resolve(name.name).unwrap_or("?").to_string();

        // Set current class context for name mangling in methods
        let old_class = self.current_class.take();
        self.current_class = Some(class_name.clone());

        let hir_type_params: Vec<_> = type_params.iter().map(|tp| {
            HirTypeParam {
                name: tp.name.name,
                bound: tp.bound.as_ref().map(|b| self.resolve_type(b)),
            }
        }).collect();

        // Resolve base class types from expressions
        let base_types: Vec<_> = bases.iter().map(|base| {
            // Base classes are usually Name expressions like `Parent` in `class Child(Parent):`
            if let roast_ast::ExprKind::Name { id, .. } = &base.kind {
                // Create a ClassType for the base class
                let class_name = self.interner.resolve(id.name)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("<sym:{}>", id.name.as_raw()));
                Type::Class(roast_typer::ClassType {
                    name: class_name,
                    module: None,
                    type_params: Vec::new(),
                    bases: Vec::new(),
                    members: Vec::new(),
                    methods: Vec::new(),
                })
            } else {
                // Fallback for complex base expressions (e.g., Generic[T])
                Type::Any
            }
        }).collect();

        // Build decorators
        let hir_decorators: Vec<HirDecorator> = decorators.iter().map(|d| {
            self.build_decorator(d)
        }).collect();

        // First pass: collect property getters and identify setters
        let mut property_getters: std::collections::HashMap<Symbol, (HirFunction, Span)> = std::collections::HashMap::new();
        let mut property_setters: std::collections::HashMap<String, HirFunction> = std::collections::HashMap::new();
        let mut regular_members: Vec<HirClassMember> = Vec::new();

        for stmt in body.iter() {
            match &stmt.kind {
                StmtKind::FunctionDef { name: method_name, args, body, returns, decorators, is_async, .. } => {
                    let func = self.build_function(method_name, args, body, returns.as_deref(), decorators, *is_async, stmt.span);

                    // Helper to check simple decorator name (like @property, @staticmethod)
                    let has_decorator = |decorator_name: &str| -> bool {
                        decorators.iter().any(|d| {
                            if let ExprKind::Name { id, .. } = &d.name.kind {
                                self.interner.resolve(id.name).map(|s| s == decorator_name).unwrap_or(false)
                            } else {
                                false
                            }
                        })
                    };

                    // Check for @name.setter pattern (attribute access decorator)
                    let get_setter_property_name = || -> Option<String> {
                        for d in decorators.iter() {
                            if let ExprKind::Attribute { value, attr, .. } = &d.name.kind {
                                if let ExprKind::Name { id, .. } = &value.kind {
                                    let attr_name = self.interner.resolve(attr.name)
                                        .map(|s| s.to_string())
                                        .unwrap_or_default();
                                    if attr_name == "setter" {
                                        return self.interner.resolve(id.name).map(|s| s.to_string());
                                    }
                                }
                            }
                        }
                        None
                    };

                    // Check for different decorator types
                    let is_property = has_decorator("property");
                    let is_staticmethod = has_decorator("staticmethod");
                    let is_classmethod = has_decorator("classmethod");
                    let is_abstractmethod = has_decorator("abstractmethod");
                    let setter_prop_name = get_setter_property_name();

                    if is_property {
                        // This is a property getter
                        property_getters.insert(method_name.name, (func, stmt.span));
                    } else if let Some(prop_name) = setter_prop_name {
                        // This is a property setter (@propname.setter)
                        property_setters.insert(prop_name, func);
                    } else {
                        // Regular method
                        let kind = if is_staticmethod {
                            MethodKind::Static
                        } else if is_classmethod {
                            MethodKind::Class
                        } else if is_abstractmethod {
                            MethodKind::Abstract
                        } else {
                            MethodKind::Instance
                        };
                        regular_members.push(HirClassMember::Method { func, kind });
                    }
                }
                StmtKind::AnnAssign { target, annotation, .. } => {
                    if let ExprKind::Name { id, .. } = &target.kind {
                        let ty = self.resolve_type(annotation);
                        regular_members.push(HirClassMember::Field {
                            name: id.name,
                            ty,
                            span: stmt.span,
                        });
                    }
                }
                StmtKind::Assign { targets, value } => {
                    // Class variable assignment (e.g., count = 0)
                    if targets.len() == 1 {
                        if let ExprKind::Name { id, .. } = &targets[0].kind {
                            let hir_value = self.build_expr(value);
                            // Infer type from value
                            let ty = match &value.kind {
                                ExprKind::IntLit { .. } => Type::Int,
                                ExprKind::FloatLit { .. } => Type::Float,
                                ExprKind::BoolLit { .. } => Type::Bool,
                                ExprKind::StringLit { .. } => Type::Str,
                                ExprKind::NoneLit => Type::NoneType,
                                _ => Type::Any,
                            };
                            regular_members.push(HirClassMember::ClassVariable {
                                name: id.name,
                                ty,
                                value: Some(hir_value),
                                span: stmt.span,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        // Second pass: combine getters and setters into Property members
        let mut members: Vec<HirClassMember> = Vec::new();
        for (name, (getter, span)) in property_getters {
            let prop_name = self.interner.resolve(name)
                .map(|s| s.to_string())
                .unwrap_or_default();
            let setter = property_setters.remove(&prop_name).map(Box::new);
            members.push(HirClassMember::Property {
                name,
                getter,
                setter,
                span,
            });
        }
        members.extend(regular_members);

        // Restore previous class context
        self.current_class = old_class;

        // Compute Method Resolution Order (MRO) using C3 linearization
        let mro = match compute_mro(&class_name, &base_types, &self.class_mros) {
            Ok(mro) => {
                // Store the MRO for use by subclasses
                self.class_mros.insert(class_name.clone(), mro.clone());
                mro
            }
            Err(err) => {
                // Report MRO error and fall back to simple class-first MRO
                self.diagnostics.error(format!("MRO error: {}", err), span);
                let mut fallback_mro = vec![class_name.clone()];
                for base in &base_types {
                    if let Type::Class(ct) = base {
                        fallback_mro.push(ct.name.clone());
                    }
                }
                self.class_mros.insert(class_name.clone(), fallback_mro.clone());
                fallback_mro
            }
        };

        // Collect abstract method names from this class
        let mut abstract_methods: Vec<String> = Vec::new();
        for member in &members {
            if let HirClassMember::Method { func, kind } = member {
                if *kind == MethodKind::Abstract {
                    let method_name = self.interner.resolve(func.name)
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    abstract_methods.push(method_name);
                }
            }
        }

        // Check for concrete method names in this class
        let concrete_methods: std::collections::HashSet<String> = members.iter()
            .filter_map(|m| {
                if let HirClassMember::Method { func, kind } = m {
                    if *kind != MethodKind::Abstract {
                        return self.interner.resolve(func.name).map(|s| s.to_string());
                    }
                }
                None
            })
            .collect();

        // TODO: In the future, inherit abstract methods from base classes
        // and remove any that are implemented in this class

        // Class is abstract if it has any unimplemented abstract methods
        let is_abstract = !abstract_methods.is_empty();

        HirClass {
            name: name.name,
            type_params: hir_type_params,
            bases: base_types,
            members,
            decorators: hir_decorators,
            mro,
            is_abstract,
            abstract_methods,
            span,
        }
    }

    fn build_block(&mut self, stmts: &[Stmt], span: Span) -> HirBlock {
        let hir_stmts: Vec<_> = stmts.iter()
            .flat_map(|s| self.build_stmt(s))
            .collect();
        HirBlock { stmts: hir_stmts, span }
    }

    fn build_stmt(&mut self, stmt: &Stmt) -> Vec<HirStmt> {
        let kind = match &stmt.kind {
            StmtKind::Assign { targets, value } => {
                if targets.len() == 1 {
                    let target = &targets[0];

                    // Handle tuple unpacking: a, b = (1, 2)
                    if let ExprKind::Tuple { elts, .. } = &target.kind {
                        return self.build_tuple_unpack_stmts(elts, value, stmt.span);
                    }

                    if let ExprKind::Name { id, .. } = &target.kind {
                        let init = self.build_expr(value);
                        // Look up the type of the initializer expression
                        let init_ty = self.expr_type_from_span(value.span);
                        HirStmtKind::Let {
                            name: id.name,
                            ty: init_ty,
                            init: Some(init),
                            mutable: true,
                        }
                    } else {
                        let target_hir = self.build_expr(target);
                        let val = self.build_expr(value);
                        HirStmtKind::Assign { target: target_hir, value: val }
                    }
                } else {
                    return vec![];
                }
            }
            StmtKind::AnnAssign { target, annotation, value, .. } => {
                if let ExprKind::Name { id, .. } = &target.kind {
                    let ty = self.resolve_type(annotation);
                    let init = value.as_ref().map(|v| self.build_expr(v));
                    HirStmtKind::Let {
                        name: id.name,
                        ty,
                        init,
                        mutable: true,
                    }
                } else if let ExprKind::Attribute { .. } = &target.kind {
                    // Handle attribute assignment: `self.x: int = value`
                    // This is just an assignment to an attribute
                    if let Some(val) = value {
                        let target_hir = self.build_expr(target);
                        let val_hir = self.build_expr(val);
                        HirStmtKind::Assign { target: target_hir, value: val_hir }
                    } else {
                        // Just a type annotation with no value - skip
                        return vec![];
                    }
                } else {
                    return vec![];
                }
            }
            StmtKind::Expr { value } => {
                let expr = self.build_expr(value);
                HirStmtKind::Expr(expr)
            }
            StmtKind::Return { value } => {
                let ret = value.as_ref().map(|v| self.build_expr(v));
                HirStmtKind::Return(ret)
            }
            StmtKind::If { test, body, orelse } => {
                let cond = self.build_expr(test);
                let then_block = self.build_block(body, stmt.span);
                let else_block = if orelse.is_empty() {
                    None
                } else {
                    Some(self.build_block(orelse, stmt.span))
                };
                HirStmtKind::If { cond, then_block, else_block }
            }
            StmtKind::While { test, body, .. } => {
                let cond = self.build_expr(test);
                let body = self.build_block(body, stmt.span);
                HirStmtKind::While { cond, body }
            }
            StmtKind::For { target, iter, body, target_annotation, .. } => {
                if let ExprKind::Name { id, .. } = &target.kind {
                    // Simple case: `for x in items:`
                    let var_ty = target_annotation.as_ref()
                        .map(|ann| self.resolve_type(ann))
                        .unwrap_or(Type::Unknown);
                    let iter_expr = self.build_expr(iter);
                    let body = self.build_block(body, stmt.span);
                    HirStmtKind::For {
                        var: id.name,
                        var_ty,
                        iter: iter_expr,
                        body,
                    }
                } else if let ExprKind::Tuple { elts, .. } = &target.kind {
                    // Tuple unpacking case: `for x, y in pairs:`
                    // Desugar into: for __iter_item in pairs: x = __iter_item[0]; y = __iter_item[1]; ...body...
                    let temp_var = self.interner.intern("__iter_item");
                    let iter_expr = self.build_expr(iter);

                    // Build the unpacking statements
                    let mut body_stmts = Vec::new();

                    // Create reference to temp variable
                    let temp_ref = self.expr_arena.alloc(HirExpr {
                        kind: HirExprKind::Var(temp_var),
                        ty: Type::Unknown,
                        span: stmt.span,
                    });

                    // For each element in the target tuple, create an assignment
                    for (i, elem) in elts.iter().enumerate() {
                        if let ExprKind::Name { id, .. } = &elem.kind {
                            // Create index expression: __iter_item[i]
                            let index_lit = self.expr_arena.alloc(HirExpr {
                                kind: HirExprKind::Literal(HirLiteral::Int(i as i128)),
                                ty: Type::Int,
                                span: stmt.span,
                            });

                            let index_expr = self.expr_arena.alloc(HirExpr {
                                kind: HirExprKind::Index { base: temp_ref, index: index_lit },
                                ty: Type::Unknown,
                                span: stmt.span,
                            });

                            // Create let statement: x = __iter_item[0]
                            body_stmts.push(HirStmt {
                                kind: HirStmtKind::Let {
                                    name: id.name,
                                    ty: Type::Unknown,
                                    init: Some(index_expr),
                                    mutable: true,
                                },
                                span: stmt.span,
                            });
                        }
                    }

                    // Add the original body statements
                    let original_body = self.build_block(body, stmt.span);
                    body_stmts.extend(original_body.stmts);

                    let new_body = HirBlock {
                        stmts: body_stmts,
                        span: stmt.span,
                    };

                    HirStmtKind::For {
                        var: temp_var,
                        var_ty: Type::Unknown,
                        iter: iter_expr,
                        body: new_body,
                    }
                } else {
                    return vec![];
                }
            }
            StmtKind::AugAssign { target, op, value } => {
                // Transform x += 5 into x = x + 5
                if let ExprKind::Name { id, .. } = &target.kind {
                    // Build the binary operation expression
                    let target_expr = self.build_expr(target);
                    let value_expr = self.build_expr(value);
                    let binop = op.to_binop();
                    let hir_op = match binop {
                        BinOp::Add => HirBinOp::Add,
                        BinOp::Sub => HirBinOp::Sub,
                        BinOp::Mult => HirBinOp::Mul,
                        BinOp::Div => HirBinOp::Div,
                        BinOp::FloorDiv => HirBinOp::FloorDiv,
                        BinOp::Mod => HirBinOp::Mod,
                        BinOp::Pow => HirBinOp::Pow,
                        BinOp::BitAnd => HirBinOp::BitAnd,
                        BinOp::BitOr => HirBinOp::BitOr,
                        BinOp::BitXor => HirBinOp::BitXor,
                        BinOp::LShift => HirBinOp::Shl,
                        BinOp::RShift => HirBinOp::Shr,
                        _ => HirBinOp::Add,
                    };
                    let binary_expr = self.expr_arena.alloc(HirExpr {
                        kind: HirExprKind::Binary { op: hir_op, left: target_expr, right: value_expr },
                        span: stmt.span,
                        ty: Type::Unknown,
                    });
                    HirStmtKind::Assign { target: target_expr, value: binary_expr }
                } else {
                    // For other targets (like subscripts, attributes), handle similarly
                    let target_expr = self.build_expr(target);
                    let value_expr = self.build_expr(value);
                    let binop = op.to_binop();
                    let hir_op = match binop {
                        BinOp::Add => HirBinOp::Add,
                        BinOp::Sub => HirBinOp::Sub,
                        BinOp::Mult => HirBinOp::Mul,
                        BinOp::Div => HirBinOp::Div,
                        BinOp::FloorDiv => HirBinOp::FloorDiv,
                        BinOp::Mod => HirBinOp::Mod,
                        BinOp::Pow => HirBinOp::Pow,
                        BinOp::BitAnd => HirBinOp::BitAnd,
                        BinOp::BitOr => HirBinOp::BitOr,
                        BinOp::BitXor => HirBinOp::BitXor,
                        BinOp::LShift => HirBinOp::Shl,
                        BinOp::RShift => HirBinOp::Shr,
                        _ => HirBinOp::Add,
                    };
                    let binary_expr = self.expr_arena.alloc(HirExpr {
                        kind: HirExprKind::Binary { op: hir_op, left: target_expr, right: value_expr },
                        span: stmt.span,
                        ty: Type::Unknown,
                    });
                    HirStmtKind::Assign { target: target_expr, value: binary_expr }
                }
            }
            StmtKind::Break => HirStmtKind::Break,
            StmtKind::Continue => HirStmtKind::Continue,
            StmtKind::Assert { test, msg } => {
                let test_expr = self.build_expr(test);
                let msg_expr = msg.as_ref().map(|m| self.build_expr(m));
                HirStmtKind::Assert { test: test_expr, msg: msg_expr }
            }
            StmtKind::Match { subject, cases } => {
                let subject_expr = self.build_expr(subject);
                let arms = cases.iter().map(|c| self.build_match_arm(c)).collect();
                HirStmtKind::Match { subject: subject_expr, arms }
            }
            StmtKind::Try { body, handlers, orelse, finalbody } => {
                let body_block = self.build_block(body, stmt.span);
                let hir_handlers: Vec<HirExceptHandler> = handlers.iter().map(|h| {
                    HirExceptHandler {
                        exc_type: h.ty.as_ref().map(|t| self.build_expr(t)),
                        name: h.name.as_ref().map(|n| n.name),
                        body: self.build_block(&h.body, h.span),
                        span: h.span,
                    }
                }).collect();
                let orelse_block = if orelse.is_empty() { None } else { Some(self.build_block(orelse, stmt.span)) };
                let finally_block = if finalbody.is_empty() { None } else { Some(self.build_block(finalbody, stmt.span)) };
                HirStmtKind::Try {
                    body: body_block,
                    handlers: hir_handlers,
                    orelse: orelse_block,
                    finalbody: finally_block,
                }
            }
            StmtKind::Raise { exc, cause } => {
                let exc_expr = exc.as_ref().map(|e| self.build_expr(e));
                let cause_expr = cause.as_ref().map(|c| self.build_expr(c));
                HirStmtKind::Raise { exc: exc_expr, cause: cause_expr }
            }
            StmtKind::Pass => return vec![],
            _ => return vec![],
        };

        vec![HirStmt { kind, span: stmt.span }]
    }

    /// Handle tuple unpacking assignment: `a, b = (1, 2)` or `a, b = b, a`
    /// This expands into temporary variables first to handle swap patterns correctly:
    /// For `a, b = b, a`:
    ///   __temp_0 = b
    ///   __temp_1 = a
    ///   a = __temp_0
    ///   b = __temp_1
    fn build_tuple_unpack_stmts(&mut self, targets: &[Expr], value: &Expr, span: Span) -> Vec<HirStmt> {
        let mut stmts = Vec::new();

        // If the RHS is a tuple literal, evaluate all elements into temps first
        // This handles swap patterns: a, b = b, a + b
        if let ExprKind::Tuple { elts: value_elts, .. } = &value.kind {
            if value_elts.len() == targets.len() {
                // First pass: evaluate all RHS expressions into temporaries
                let mut temp_ids: Vec<HirExprId> = Vec::new();
                for (i, val_expr) in value_elts.iter().enumerate() {
                    let val = self.build_expr(val_expr);
                    let temp_name = self.interner.intern(&format!("__tuple_temp_{}", i));

                    // Create let statement for temp
                    stmts.push(HirStmt {
                        kind: HirStmtKind::Let {
                            name: temp_name,
                            ty: Type::Unknown,
                            init: Some(val),
                            mutable: false,
                        },
                        span,
                    });

                    // Create reference to the temp for later assignment
                    let temp_ref = self.expr_arena.alloc(HirExpr {
                        kind: HirExprKind::Var(temp_name),
                        ty: Type::Unknown,
                        span,
                    });
                    temp_ids.push(temp_ref);
                }

                // Second pass: assign temps to targets
                for (target, temp_ref) in targets.iter().zip(temp_ids.iter()) {
                    if let ExprKind::Name { id, .. } = &target.kind {
                        stmts.push(HirStmt {
                            kind: HirStmtKind::Let {
                                name: id.name,
                                ty: Type::Unknown,
                                init: Some(*temp_ref),
                                mutable: true,
                            },
                            span,
                        });
                    }
                }
                return stmts;
            }
        }

        // Fall back to index-based unpacking for non-tuple RHS (e.g., function return)
        let value_expr = self.build_expr(value);

        // For each target in the tuple, create an assignment from the index
        for (i, target) in targets.iter().enumerate() {
            if let ExprKind::Name { id, .. } = &target.kind {
                // Create index expression: value_expr[i]
                let index_lit = self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Literal(HirLiteral::Int(i as i128)),
                    ty: Type::Int,
                    span,
                });

                let index_expr = self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Index { base: value_expr, index: index_lit },
                    ty: Type::Unknown,
                    span,
                });

                stmts.push(HirStmt {
                    kind: HirStmtKind::Let {
                        name: id.name,
                        ty: Type::Unknown,
                        init: Some(index_expr),
                        mutable: true,
                    },
                    span,
                });
            }
        }

        stmts
    }

    /// Helper to determine type of an expression by looking up from type context
    fn expr_type_from_span(&self, span: Span) -> Type {
        if let Some(ty) = self.type_ctx.lookup_expr_type(span) {
            ty.clone()
        } else {
            Type::Unknown
        }
    }

    fn build_expr(&mut self, expr: &Expr) -> HirExprId {
        let kind = match &expr.kind {
            ExprKind::IntLit { value } => {
                let n = value.try_into().unwrap_or(0);
                HirExprKind::Literal(HirLiteral::Int(n))
            }
            ExprKind::FloatLit { value } => HirExprKind::Literal(HirLiteral::Float(*value)),
            ExprKind::BoolLit { value } => HirExprKind::Literal(HirLiteral::Bool(*value)),
            ExprKind::StringLit { value, .. } => HirExprKind::Literal(HirLiteral::Str(value.clone())),
            ExprKind::BytesLit { value } => HirExprKind::Literal(HirLiteral::Bytes(value.clone())),
            ExprKind::NoneLit => HirExprKind::Literal(HirLiteral::None),
            ExprKind::Name { id, .. } => HirExprKind::Var(id.name),
            ExprKind::BinOp { left, op, right } => {
                let l = self.build_expr(left);
                let r = self.build_expr(right);
                let hir_op = match op {
                    BinOp::Add => HirBinOp::Add,
                    BinOp::Sub => HirBinOp::Sub,
                    BinOp::Mult => HirBinOp::Mul,
                    BinOp::Div => HirBinOp::Div,
                    BinOp::FloorDiv => HirBinOp::FloorDiv,
                    BinOp::Mod => HirBinOp::Mod,
                    BinOp::Pow => HirBinOp::Pow,
                    BinOp::BitAnd => HirBinOp::BitAnd,
                    BinOp::BitOr => HirBinOp::BitOr,
                    BinOp::BitXor => HirBinOp::BitXor,
                    BinOp::LShift => HirBinOp::Shl,
                    BinOp::RShift => HirBinOp::Shr,
                    _ => HirBinOp::Add,
                };
                HirExprKind::Binary { op: hir_op, left: l, right: r }
            }
            ExprKind::UnaryOp { op, operand } => {
                let inner = self.build_expr(operand);
                let hir_op = match op {
                    UnaryOp::USub => HirUnaryOp::Neg,
                    UnaryOp::Not => HirUnaryOp::Not,
                    UnaryOp::Invert => HirUnaryOp::BitNot,
                    UnaryOp::UAdd => return inner,
                };
                HirExprKind::Unary { op: hir_op, operand: inner }
            }
            ExprKind::BoolOp { op, values } => {
                // Build True and True and True as nested And/Or operations
                if values.len() < 2 {
                    // Single value, just return it (though this shouldn't happen)
                    return self.build_expr(&values[0]);
                }

                let hir_op = match op {
                    BoolOp::And => HirBinOp::And,
                    BoolOp::Or => HirBinOp::Or,
                };

                // Build the chain: a and b and c -> (a and b) and c
                let mut result = self.build_expr(&values[0]);
                for val in values.iter().skip(1) {
                    let right = self.build_expr(val);
                    result = self.expr_arena.alloc(HirExpr {
                        kind: HirExprKind::Binary { op: hir_op, left: result, right },
                        span: expr.span,
                        ty: Type::Unknown,
                    });
                }

                // Return early since we're returning an id
                return result;
            }
            ExprKind::Call { func, args, .. } => {
                let callee = self.build_expr(func);
                let hir_args: Vec<_> = args.iter().map(|a| self.build_expr(a)).collect();
                HirExprKind::Call { callee, args: hir_args }
            }
            ExprKind::Attribute { value, attr, .. } => {
                let base = self.build_expr(value);
                // Apply private name mangling for __private attributes (not __dunder__)
                let attr_name = self.interner.resolve(attr.name).unwrap_or("?");
                let mangled_field = if attr_name.starts_with("__") && !attr_name.ends_with("__") {
                    // Private attribute: mangle to _ClassName__attr
                    if let Some(ref class_name) = self.current_class {
                        let mangled = format!("_{}{}", class_name, attr_name);
                        self.interner.intern(&mangled)
                    } else {
                        attr.name
                    }
                } else {
                    attr.name
                };
                HirExprKind::Field { base, field: mangled_field }
            }
            ExprKind::Subscript { value, slice, .. } => {
                let base = self.build_expr(value);
                let index = self.build_expr(slice);
                HirExprKind::Index { base, index }
            }
            ExprKind::Slice { lower, upper, step } => {
                let lower = lower.as_ref().map(|e| self.build_expr(e));
                let upper = upper.as_ref().map(|e| self.build_expr(e));
                let step = step.as_ref().map(|e| self.build_expr(e));
                HirExprKind::Slice { lower, upper, step }
            }
            ExprKind::List { elts, .. } => {
                let items: Vec<_> = elts.iter().map(|e| self.build_expr(e)).collect();
                HirExprKind::List(items)
            }
            ExprKind::Tuple { elts, .. } => {
                let items: Vec<_> = elts.iter().map(|e| self.build_expr(e)).collect();
                HirExprKind::Tuple(items)
            }
            ExprKind::Dict { keys, values } => {
                let pairs: Vec<_> = keys.iter().zip(values.iter()).filter_map(|(k, v)| {
                    k.as_ref().map(|key| (self.build_expr(key), self.build_expr(v)))
                }).collect();
                HirExprKind::Dict(pairs)
            }
            ExprKind::Set { elts } => {
                let items: Vec<_> = elts.iter().map(|e| self.build_expr(e)).collect();
                HirExprKind::Set(items)
            }
            ExprKind::IfExp { test, body, orelse } => {
                let cond = self.build_expr(test);
                let then_expr = self.build_expr(body);
                let else_expr = self.build_expr(orelse);
                HirExprKind::If { cond, then_expr, else_expr }
            }
            ExprKind::Compare { left, ops, comparators } => {
                let l = self.build_expr(left);
                if ops.len() == 1 {
                    let r = self.build_expr(&comparators[0]);
                    let hir_op = match ops[0] {
                        CmpOp::Eq => HirBinOp::Eq,
                        CmpOp::NotEq => HirBinOp::Ne,
                        CmpOp::Lt => HirBinOp::Lt,
                        CmpOp::LtE => HirBinOp::Le,
                        CmpOp::Gt => HirBinOp::Gt,
                        CmpOp::GtE => HirBinOp::Ge,
                        CmpOp::In => HirBinOp::In,
                        CmpOp::NotIn => HirBinOp::NotIn,
                        // TODO: Implement Is, IsNot
                        _ => HirBinOp::Eq,
                    };
                    HirExprKind::Binary { op: hir_op, left: l, right: r }
                } else {
                    // Chained comparisons: 1 < x < 10 becomes (1 < x) and (x < 10)
                    // Build the first comparison
                    let mut all_values = vec![left.as_ref().clone()];
                    all_values.extend(comparators.iter().cloned());

                    // Create individual comparisons
                    let mut comparisons: Vec<HirExprId> = Vec::new();
                    for i in 0..ops.len() {
                        let left_expr = self.build_expr(&all_values[i]);
                        let right_expr = self.build_expr(&all_values[i + 1]);
                        let hir_op = match ops[i] {
                            CmpOp::Eq => HirBinOp::Eq,
                            CmpOp::NotEq => HirBinOp::Ne,
                            CmpOp::Lt => HirBinOp::Lt,
                            CmpOp::LtE => HirBinOp::Le,
                            CmpOp::Gt => HirBinOp::Gt,
                            CmpOp::GtE => HirBinOp::Ge,
                            CmpOp::In => HirBinOp::In,
                            CmpOp::NotIn => HirBinOp::NotIn,
                            _ => HirBinOp::Eq,
                        };
                        let cmp = self.expr_arena.alloc(HirExpr {
                            kind: HirExprKind::Binary { op: hir_op, left: left_expr, right: right_expr },
                            span: expr.span,
                            ty: Type::Unknown,
                        });
                        comparisons.push(cmp);
                    }

                    // Chain them together with And
                    let mut result = comparisons[0];
                    for i in 1..comparisons.len() {
                        result = self.expr_arena.alloc(HirExpr {
                            kind: HirExprKind::Binary { op: HirBinOp::And, left: result, right: comparisons[i] },
                            span: expr.span,
                            ty: Type::Unknown,
                        });
                    }

                    // Return the kind of the result
                    return result;
                }
            }
            // Try expression (? operator): propagates errors early
            ExprKind::Try { value } => {
                let expr = self.build_expr(value);
                HirExprKind::Try { expr }
            }
            // List comprehension: [elt for x in iter if cond]
            ExprKind::ListComp { elt, generators } => {
                // Desugar into a for loop that builds a list
                // For simplicity, we'll create this as a special comprehension expression
                // that MIR/codegen will handle
                if generators.is_empty() {
                    HirExprKind::List(vec![])
                } else {
                    // Build the element expression and iterables
                    let element = self.build_expr(elt);

                    // For now, support single generator
                    let gen = &generators[0];
                    let iter_expr = self.build_expr(&gen.iter);

                    // Extract the variable name from the target expression
                    // The target is typically a Name expression
                    let target_pattern = match &gen.target.kind {
                        ExprKind::Name { id, .. } => {
                            HirPattern {
                                kind: HirPatternKind::Binding(id.name),
                                ty: Type::Unknown,
                                span: gen.target.span,
                            }
                        }
                        _ => {
                            // For more complex targets, use wildcard
                            HirPattern {
                                kind: HirPatternKind::Wildcard,
                                ty: Type::Unknown,
                                span: gen.target.span,
                            }
                        }
                    };
                    let condition = gen.ifs.first().map(|c| self.build_expr(c));

                    HirExprKind::ListComp {
                        element,
                        pattern: target_pattern,
                        iter: iter_expr,
                        condition,
                    }
                }
            }
            // F-string (JoinedStr): f"Hello {name}!"
            ExprKind::JoinedStr { values } => {
                // Convert f-string to string concatenation
                // Build calls to str() for each value and concatenate
                if values.is_empty() {
                    HirExprKind::Literal(HirLiteral::Str(String::new()))
                } else if values.len() == 1 {
                    // Single value - just convert it
                    return self.build_fstring_part(&values[0], expr.span);
                } else {
                    // Multiple values - concatenate them
                    let mut parts: Vec<HirExprId> = values.iter()
                        .map(|v| self.build_fstring_part(v, expr.span))
                        .collect();

                    // Build concatenation chain: a + b + c
                    let mut result = parts.remove(0);
                    for part in parts {
                        result = self.expr_arena.alloc(HirExpr {
                            kind: HirExprKind::Binary {
                                op: HirBinOp::Add,
                                left: result,
                                right: part,
                            },
                            ty: Type::Str,  // String concatenation produces Str
                            span: expr.span,
                        });
                    }
                    return result;
                }
            }
            // FormattedValue: single {expr} in f-string
            ExprKind::FormattedValue { value, .. } => {
                // Convert value to string via str() call
                let inner = self.build_expr(value);
                let str_func = self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Var(self.interner.intern("str")),
                    ty: Type::Unknown,
                    span: expr.span,
                });
                HirExprKind::Call { callee: str_func, args: vec![inner] }
            }
            // Lambda expression
            ExprKind::Lambda { args, body } => {
                // Convert lambda parameters to HIR params
                let params: Vec<HirParam> = args.args.iter().map(|arg| {
                    let default = arg.default.as_ref().map(|d| self.build_expr(d));
                    HirParam {
                        name: arg.name.name,
                        ty: Type::Unknown, // Lambda params are untyped in Python syntax
                        default,
                        kind: HirParamKind::Regular,
                        span: arg.span,
                    }
                }).collect();

                // Build the body expression
                let body_expr = self.build_expr(body);

                HirExprKind::Lambda { params, body: body_expr }
            }
            // Await expression
            ExprKind::Await { value } => {
                let awaited = self.build_expr(value);
                HirExprKind::Await { value: awaited }
            }
            _ => HirExprKind::Literal(HirLiteral::None),
        };

        // Look up the type from the type context (recorded during type checking)
        let ty = self.expr_type_from_span(expr.span);

        let hir_expr = HirExpr {
            kind,
            ty,
            span: expr.span,
        };
        self.expr_arena.alloc(hir_expr)
    }

    /// Build an f-string part (either a literal string or a formatted value)
    fn build_fstring_part(&mut self, expr: &Expr, span: Span) -> HirExprId {
        match &expr.kind {
            ExprKind::StringLit { value, .. } => {
                self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Literal(HirLiteral::Str(value.clone())),
                    ty: Type::Str,  // String literals are always Str type
                    span,
                })
            }
            ExprKind::FormattedValue { value, .. } => {
                // Convert value to string via str() call
                let inner = self.build_expr(value);
                let str_func = self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Var(self.interner.intern("str")),
                    ty: Type::Unknown,
                    span,
                });
                self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Call { callee: str_func, args: vec![inner] },
                    ty: Type::Str,  // str() always returns Str
                    span,
                })
            }
            // Other expression types - convert to string
            _ => {
                let inner = self.build_expr(expr);
                let str_func = self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Var(self.interner.intern("str")),
                    ty: Type::Unknown,
                    span,
                });
                self.expr_arena.alloc(HirExpr {
                    kind: HirExprKind::Call { callee: str_func, args: vec![inner] },
                    ty: Type::Str,  // str() always returns Str
                    span,
                })
            }
        }
    }

    fn build_match_arm(&mut self, case: &MatchCase) -> HirMatchArm {
        let pattern = self.build_pattern(&case.pattern);
        let guard = case.guard.as_ref().map(|g| self.build_expr(g));
        let body = self.build_block(&case.body, case.span);
        HirMatchArm { pattern, guard, body }
    }

    fn build_pattern(&mut self, pattern: &Pattern) -> HirPattern {
        let kind = match &pattern.kind {
            PatternKind::Wildcard => HirPatternKind::Wildcard,
            PatternKind::Capture { name } => HirPatternKind::Binding(name.name),
            PatternKind::MatchValue { value } => {
                // Convert expression to literal if possible
                match &value.kind {
                    ExprKind::IntLit { value: n } => {
                        let n: i128 = n.try_into().unwrap_or(0);
                        HirPatternKind::Literal(HirLiteral::Int(n))
                    }
                    ExprKind::FloatLit { value: f } => {
                        HirPatternKind::Literal(HirLiteral::Float(*f))
                    }
                    ExprKind::BoolLit { value: b } => {
                        HirPatternKind::Literal(HirLiteral::Bool(*b))
                    }
                    ExprKind::StringLit { value: s, .. } => {
                        HirPatternKind::Literal(HirLiteral::Str(s.clone()))
                    }
                    ExprKind::NoneLit => HirPatternKind::Literal(HirLiteral::None),
                    _ => HirPatternKind::Wildcard, // Fallback for complex expressions
                }
            }
            PatternKind::MatchSingleton { value } => {
                match &value.kind {
                    ExprKind::BoolLit { value: b } => {
                        HirPatternKind::Literal(HirLiteral::Bool(*b))
                    }
                    ExprKind::NoneLit => HirPatternKind::Literal(HirLiteral::None),
                    _ => HirPatternKind::Wildcard,
                }
            }
            PatternKind::MatchSequence { patterns } => {
                let sub_patterns = patterns.iter()
                    .map(|p| self.build_pattern(p))
                    .collect();
                HirPatternKind::Tuple(sub_patterns)
            }
            PatternKind::MatchAs { pattern: sub, name } => {
                if let Some(p) = sub {
                    // Pattern with binding
                    let _inner = self.build_pattern(p);
                    if let Some(n) = name {
                        HirPatternKind::Binding(n.name)
                    } else {
                        HirPatternKind::Wildcard
                    }
                } else if let Some(n) = name {
                    HirPatternKind::Binding(n.name)
                } else {
                    HirPatternKind::Wildcard
                }
            }
            PatternKind::MatchOr { patterns } => {
                let sub_patterns = patterns.iter()
                    .map(|p| self.build_pattern(p))
                    .collect();
                HirPatternKind::Or(sub_patterns)
            }
            PatternKind::MatchClass { cls, patterns, kwd_attrs, kwd_patterns } => {
                // Extract class name and build fields
                let ty = if let ExprKind::Name { id, .. } = &cls.kind {
                    let name = self.interner.resolve(id.name).unwrap_or("?").to_string();
                    Type::Class(roast_typer::ClassType {
                        name,
                        module: None,
                        type_params: vec![],
                        members: Default::default(),
                        methods: Default::default(),
                        bases: vec![],
                    })
                } else {
                    Type::Unknown
                };

                let mut fields = Vec::new();
                // Positional patterns (we'd need class info to know field names)
                for (i, p) in patterns.iter().enumerate() {
                    let field_name = self.interner.intern(&format!("_{}", i));
                    fields.push((field_name, self.build_pattern(p)));
                }
                // Keyword patterns
                for (attr, p) in kwd_attrs.iter().zip(kwd_patterns.iter()) {
                    fields.push((attr.name, self.build_pattern(p)));
                }

                HirPatternKind::Struct { ty, fields }
            }
            PatternKind::MatchStar { name } => {
                if let Some(n) = name {
                    HirPatternKind::Binding(n.name)
                } else {
                    HirPatternKind::Wildcard
                }
            }
            PatternKind::MatchMapping { .. } => {
                // Dict patterns - simplified for now
                HirPatternKind::Wildcard
            }
        };

        HirPattern {
            kind,
            ty: Type::Unknown,
            span: pattern.span,
        }
    }

    fn resolve_type(&mut self, ty_expr: &TypeExpr) -> Type {
        let mut checker = TypeChecker::new(self.type_ctx, self.interner, self.diagnostics);
        checker.resolve_type_expr(ty_expr)
    }
}
