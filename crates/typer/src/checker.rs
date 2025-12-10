//! Type checker for Roast.

use crate::context::TypeContext;
use crate::types::*;
use roast_ast::*;
use roast_common::{Diagnostic, DiagnosticSink, Interner, Span};
use std::sync::Arc;

/// The type checker.
pub struct TypeChecker<'a> {
    ctx: &'a mut TypeContext,
    interner: &'a Interner,
    diagnostics: &'a mut DiagnosticSink,
    /// Current function return type (for checking return statements).
    current_return_type: Option<Type>,
    /// Current class type (for checking methods and attributes).
    current_class: Option<Type>,
    /// Whether we're in an async function.
    in_async: bool,
}

impl<'a> TypeChecker<'a> {
    /// Creates a new type checker.
    pub fn new(
        ctx: &'a mut TypeContext,
        interner: &'a Interner,
        diagnostics: &'a mut DiagnosticSink,
    ) -> Self {
        Self {
            ctx,
            interner,
            diagnostics,
            current_return_type: None,
            current_class: None,
            in_async: false,
        }
    }

    /// Type checks a module.
    pub fn check_module(&mut self, module: &Module) {
        // First pass: collect all function and class signatures
        for stmt in &module.body {
            self.collect_definitions(stmt);
        }

        // Second pass: check all statement bodies
        for stmt in &module.body {
            self.check_stmt(stmt);
        }
    }

    /// First pass: collect function/class signatures without checking bodies
    fn collect_definitions(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::FunctionDef {
                name,
                args,
                returns,
                is_async,
                ..
            } => {
                // Create function type from signature only
                let mut params = Vec::new();
                
                // Handle positional-only args
                for arg in &args.posonlyargs {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::PosOnly,
                    });
                }
                
                // Handle regular args
                for arg in &args.args {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::Regular,
                    });
                }
                
                // Handle *args (vararg)
                if let Some(vararg) = &args.vararg {
                    let ty = if let Some(ann) = &vararg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(vararg.name.name),
                        ty,
                        default: false,
                        kind: ParamKind::VarPositional,
                    });
                }
                
                // Handle keyword-only args
                for arg in &args.kwonlyargs {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::KwOnly,
                    });
                }
                
                // Handle **kwargs (kwarg)
                if let Some(kwarg) = &args.kwarg {
                    let ty = if let Some(ann) = &kwarg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(kwarg.name.name),
                        ty,
                        default: false,
                        kind: ParamKind::VarKeyword,
                    });
                }
                
                let return_type = if let Some(ret) = returns {
                    self.resolve_type_expr(ret)
                } else {
                    Type::NoneType
                };
                let func_type = Type::callable(params, return_type, *is_async);
                self.ctx.define(name.name, func_type);
            }
            StmtKind::ClassDef { name, bases, .. } => {
                // Create class type placeholder
                let base_types: Vec<Type> = bases.iter().map(|b| self.check_expr(b)).collect();
                let class_type = Type::Class(ClassType {
                    name: self.interner.resolve(name.name).unwrap_or("?").to_string(),
                    module: None,
                    type_params: Vec::new(),
                    bases: base_types,
                    members: Vec::new(),
                    methods: Vec::new(),
                });
                self.ctx.define(name.name, class_type);
            }
            _ => {}
        }
    }

    /// Type checks a statement.
    pub fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::FunctionDef {
                name,
                args,
                body,
                returns,
                is_async,
                ..
            } => {
                self.check_function_def(name, args, body, returns.as_deref(), *is_async, stmt.span);
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                ..
            } => {
                self.check_class_def(name, bases, body, stmt.span);
            }
            StmtKind::Return { value } => {
                self.check_return(value.as_deref(), stmt.span);
            }
            StmtKind::Assign { targets, value } => {
                self.check_assign(targets, value, stmt.span);
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                ..
            } => {
                self.check_ann_assign(target, annotation, value.as_deref(), stmt.span);
            }
            StmtKind::AugAssign { target, op, value } => {
                self.check_aug_assign(target, *op, value, stmt.span);
            }
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
                target_annotation,
                ..
            } => {
                self.check_for(target, iter, body, orelse, target_annotation.as_deref(), stmt.span);
            }
            StmtKind::While { test, body, orelse } => {
                self.check_while(test, body, orelse, stmt.span);
            }
            StmtKind::If { test, body, orelse } => {
                self.check_if(test, body, orelse, stmt.span);
            }
            StmtKind::With { items, body, .. } => {
                self.check_with(items, body, stmt.span);
            }
            StmtKind::Raise { exc, cause } => {
                self.check_raise(exc.as_deref(), cause.as_deref(), stmt.span);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                self.check_try(body, handlers, orelse, finalbody, stmt.span);
            }
            StmtKind::Assert { test, msg } => {
                self.check_assert(test, msg.as_deref(), stmt.span);
            }
            StmtKind::Expr { value } => {
                self.check_expr(value);
            }
            StmtKind::Pass | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => {
                // Import handling would go here
            }
            StmtKind::Global { .. } | StmtKind::Nonlocal { .. } => {
                // Scope handling
            }
            StmtKind::Match { subject, cases } => {
                self.check_match(subject, cases, stmt.span);
            }
            _ => {}
        }
    }

    fn check_function_def(
        &mut self,
        name: &Ident,
        args: &Arguments,
        body: &[Stmt],
        returns: Option<&TypeExpr>,
        is_async: bool,
        _span: Span,
    ) {
        // Create function type
        let mut params = Vec::new();

        // Handle positional-only args
        for arg in &args.posonlyargs {
            let ty = if let Some(ann) = &arg.annotation {
                self.resolve_type_expr(ann)
            } else {
                self.ctx.fresh_var()
            };
            params.push(FuncParam {
                name: Some(arg.name.name),
                ty,
                default: arg.default.is_some(),
                kind: ParamKind::PosOnly,
            });
        }

        // Handle regular args
        for (i, arg) in args.args.iter().enumerate() {
            let ty = if let Some(ann) = &arg.annotation {
                self.resolve_type_expr(ann)
            } else {
                // If this is the first argument of a method, infer it as the class type
                if i == 0 && self.current_class.is_some() && args.posonlyargs.is_empty() {
                     self.current_class.clone().unwrap()
                } else {
                    self.ctx.fresh_var()
                }
            };
            params.push(FuncParam {
                name: Some(arg.name.name),
                ty,
                default: arg.default.is_some(),
                kind: ParamKind::Regular,
            });
        }

        // Handle *args (vararg)
        if let Some(vararg) = &args.vararg {
            let ty = if let Some(ann) = &vararg.annotation {
                self.resolve_type_expr(ann)
            } else {
                self.ctx.fresh_var()
            };
            params.push(FuncParam {
                name: Some(vararg.name.name),
                ty,
                default: false,
                kind: ParamKind::VarPositional,
            });
        }

        // Handle keyword-only args
        for arg in &args.kwonlyargs {
            let ty = if let Some(ann) = &arg.annotation {
                self.resolve_type_expr(ann)
            } else {
                self.ctx.fresh_var()
            };
            params.push(FuncParam {
                name: Some(arg.name.name),
                ty,
                default: arg.default.is_some(),
                kind: ParamKind::KwOnly,
            });
        }

        // Handle **kwargs (kwarg)
        if let Some(kwarg) = &args.kwarg {
            let ty = if let Some(ann) = &kwarg.annotation {
                self.resolve_type_expr(ann)
            } else {
                self.ctx.fresh_var()
            };
            params.push(FuncParam {
                name: Some(kwarg.name.name),
                ty,
                default: false,
                kind: ParamKind::VarKeyword,
            });
        }

        let return_type = if let Some(ret) = returns {
            self.resolve_type_expr(ret)
        } else {
            Type::NoneType
        };

        let func_type = Type::callable(params.clone(), return_type.clone(), is_async);
        self.ctx.define(name.name, func_type);

        // Check body
        self.ctx.push_scope();

        // Add parameters to scope - including vararg and kwarg
        for arg in &args.posonlyargs {
            if let Some(param) = params.iter().find(|p| p.name == Some(arg.name.name)) {
                self.ctx.define(arg.name.name, param.ty.clone());
            }
        }
        for arg in &args.args {
            if let Some(param) = params.iter().find(|p| p.name == Some(arg.name.name)) {
                self.ctx.define(arg.name.name, param.ty.clone());
            }
        }
        if let Some(vararg) = &args.vararg {
            if let Some(param) = params.iter().find(|p| p.name == Some(vararg.name.name)) {
                // Vararg is a tuple of the element type
                self.ctx.define(vararg.name.name, Type::Tuple(vec![param.ty.clone()]));
            }
        }
        for arg in &args.kwonlyargs {
            if let Some(param) = params.iter().find(|p| p.name == Some(arg.name.name)) {
                self.ctx.define(arg.name.name, param.ty.clone());
            }
        }
        if let Some(kwarg) = &args.kwarg {
            if let Some(param) = params.iter().find(|p| p.name == Some(kwarg.name.name)) {
                // Kwarg is a dict of str to the element type
                self.ctx.define(kwarg.name.name, Type::Dict(Arc::new(Type::Str), Arc::new(param.ty.clone())));
            }
        }

        let prev_return = self.current_return_type.replace(return_type);
        let prev_async = self.in_async;
        self.in_async = is_async;

        for stmt in body {
            self.check_stmt(stmt);
        }

        self.current_return_type = prev_return;
        self.in_async = prev_async;
        self.ctx.pop_scope();
    }

    fn check_class_def(
        &mut self,
        name: &Ident,
        bases: &[Expr],
        body: &[Stmt],
        _span: Span,
    ) {
        // Resolve base classes
        let base_types: Vec<Type> = bases.iter().map(|b| self.check_expr(b)).collect();

        // Create class type
        let mut class_type = ClassType {
            name: self.interner.resolve(name.name).unwrap_or("?").to_string(),
            module: None,
            type_params: Vec::new(),
            bases: base_types,
            members: Vec::new(),
            methods: Vec::new(),
        };

        // Pre-scan body for methods
        for stmt in body {
            if let StmtKind::FunctionDef { name: func_name, args, returns, is_async, .. } = &stmt.kind {
                 // Create function type from signature only
                let mut params = Vec::new();

                // Handle positional-only args
                for arg in &args.posonlyargs {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::PosOnly,
                    });
                }

                // Handle regular args
                for arg in &args.args {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::Regular,
                    });
                }

                // Handle *args (vararg)
                if let Some(vararg) = &args.vararg {
                    let ty = if let Some(ann) = &vararg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(vararg.name.name),
                        ty,
                        default: false,
                        kind: ParamKind::VarPositional,
                    });
                }

                // Handle keyword-only args
                for arg in &args.kwonlyargs {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::KwOnly,
                    });
                }

                // Handle **kwargs (kwarg)
                if let Some(kwarg) = &args.kwarg {
                    let ty = if let Some(ann) = &kwarg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    params.push(FuncParam {
                        name: Some(kwarg.name.name),
                        ty,
                        default: false,
                        kind: ParamKind::VarKeyword,
                    });
                }

                let return_type = if let Some(ret) = returns {
                    self.resolve_type_expr(ret)
                } else {
                    Type::NoneType
                };
                let func_type = Type::callable(params, return_type, *is_async);

                class_type.methods.push((self.interner.resolve(func_name.name).unwrap_or("?").to_string(), func_type));
            }
        }

        let class_ty = Type::Class(class_type);
        self.ctx.define(name.name, class_ty.clone());

        // Check body
        self.ctx.push_scope();

        let prev_class = self.current_class.replace(class_ty.clone());

        for stmt in body {
            self.check_stmt(stmt);
        }

        // Save the updated class definition with any new members found
        let updated_class = self.current_class.take();

        self.current_class = prev_class;
        self.ctx.pop_scope();

        // Update the class definition in the *outer* scope (after pop_scope)
        if let Some(Type::Class(cls)) = updated_class {
            self.ctx.define(name.name, Type::Class(cls));
        }
    }

    fn check_return(&mut self, value: Option<&Expr>, span: Span) {
        let return_type = if let Some(val) = value {
            self.check_expr(val)
        } else {
            Type::NoneType
        };

        if let Some(expected) = &self.current_return_type {
            if !self.ctx.is_subtype(&return_type, expected) {
                self.diagnostics.report(
                    Diagnostic::error(format!(
                        "return type mismatch: expected {}, got {}",
                        expected, return_type
                    ))
                    .with_span(span),
                );
            }
        }
    }

    fn check_assign(&mut self, targets: &[Expr], value: &Expr, _span: Span) {
        let value_type = self.check_expr(value);

        for target in targets {
            match &target.kind {
                ExprKind::Name { id, .. } => {
                    self.ctx.define(id.name, value_type.clone());
                }
                ExprKind::Tuple { elts, .. } | ExprKind::List { elts, .. } => {
                    // Destructuring assignment
                    if let Type::Tuple(elem_types) = &value_type {
                        if elts.len() != elem_types.len() {
                            self.diagnostics.report(
                                Diagnostic::error(format!(
                                    "cannot unpack {} values into {} targets",
                                    elem_types.len(),
                                    elts.len()
                                ))
                                .with_span(target.span),
                            );
                        } else {
                            for (elt, ty) in elts.iter().zip(elem_types.iter()) {
                                if let ExprKind::Name { id, .. } = &elt.kind {
                                    self.ctx.define(id.name, ty.clone());
                                }
                            }
                        }
                    }
                }
                ExprKind::Attribute { value, attr, .. } => {
                    // Check if value is 'self' and we're inside a class
                    if let ExprKind::Name { id, .. } = &value.kind {
                        if self.interner.resolve(id.name).unwrap_or("") == "self" && self.current_class.is_some() {
                            // Register member with inferred type from value
                            if let Some(Type::Class(cls)) = &mut self.current_class {
                                let attr_name = self.interner.resolve(attr.name).unwrap_or("?").to_string();
                                // Check if already exists
                                if !cls.members.iter().any(|(n, _)| n == &attr_name) {
                                    cls.members.push((attr_name, value_type.clone()));
                                }
                            }
                        }
                    }
                    self.check_expr(target);
                }
                _ => {
                    self.check_expr(target);
                }
            }
        }
    }

    fn check_ann_assign(
        &mut self,
        target: &Expr,
        annotation: &TypeExpr,
        value: Option<&Expr>,
        span: Span,
    ) {
        let declared_type = self.resolve_type_expr(annotation);

        if let Some(val) = value {
            let value_type = self.check_expr(val);
            if !self.ctx.is_subtype(&value_type, &declared_type) {
                self.diagnostics.report(
                    Diagnostic::error(format!(
                        "type mismatch: expected {}, got {}",
                        declared_type, value_type
                    ))
                    .with_span(span),
                );
            }
        }

        if let ExprKind::Name { id, .. } = &target.kind {
            self.ctx.define(id.name, declared_type);
        } else if let ExprKind::Attribute { value, attr, .. } = &target.kind {
             // Check if value is 'self'
             if let ExprKind::Name { id, .. } = &value.kind {
                 if self.interner.resolve(id.name).unwrap_or("") == "self" && self.current_class.is_some() {
                     // Register member
                     if let Some(Type::Class(cls)) = &mut self.current_class {
                         let attr_name = self.interner.resolve(attr.name).unwrap_or("?").to_string();
                         // Check if already exists
                         if !cls.members.iter().any(|(n, _)| n == &attr_name) {
                             cls.members.push((attr_name, declared_type.clone()));
                         }
                     }
                 }
             }
        }
    }

    fn check_aug_assign(&mut self, target: &Expr, op: AugOp, value: &Expr, span: Span) {
        let target_type = self.check_expr(target);
        let value_type = self.check_expr(value);

        // Check that the operation is valid
        let _result_type = self.check_binary_op(op.to_binop(), &target_type, &value_type, span);
    }

    fn check_for(
        &mut self,
        target: &Expr,
        iter: &Expr,
        body: &[Stmt],
        orelse: &[Stmt],
        target_annotation: Option<&TypeExpr>,
        span: Span,
    ) {
        let iter_type = self.check_expr(iter);

        // Infer element type from iterator
        let elem_type = self.get_iterator_element_type(&iter_type, span);

        // Check type annotation if provided
        if let Some(ann) = target_annotation {
            let declared_type = self.resolve_type_expr(ann);
            if !self.ctx.is_subtype(&elem_type, &declared_type) {
                self.diagnostics.report(
                    Diagnostic::error(format!(
                        "loop variable type mismatch: declared {}, but iterator yields {}",
                        declared_type, elem_type
                    ))
                    .with_span(span),
                );
            }
        }

        self.ctx.push_scope();

        if let ExprKind::Name { id, .. } = &target.kind {
            self.ctx.define(id.name, elem_type);
        }

        for stmt in body {
            self.check_stmt(stmt);
        }

        self.ctx.pop_scope();

        for stmt in orelse {
            self.check_stmt(stmt);
        }
    }

    fn check_while(&mut self, test: &Expr, body: &[Stmt], orelse: &[Stmt], span: Span) {
        let test_type = self.check_expr(test);
        self.check_bool_context(&test_type, span);

        self.ctx.push_scope();
        for stmt in body {
            self.check_stmt(stmt);
        }
        self.ctx.pop_scope();

        for stmt in orelse {
            self.check_stmt(stmt);
        }
    }

    fn check_if(&mut self, test: &Expr, body: &[Stmt], orelse: &[Stmt], span: Span) {
        let test_type = self.check_expr(test);
        self.check_bool_context(&test_type, span);

        self.ctx.push_scope();
        for stmt in body {
            self.check_stmt(stmt);
        }
        self.ctx.pop_scope();

        self.ctx.push_scope();
        for stmt in orelse {
            self.check_stmt(stmt);
        }
        self.ctx.pop_scope();
    }

    fn check_with(&mut self, items: &[WithItem], body: &[Stmt], _span: Span) {
        self.ctx.push_scope();

        for item in items {
            let ctx_type = self.check_expr(&item.context_expr);

            if let Some(vars) = &item.optional_vars {
                if let ExprKind::Name { id, .. } = &vars.kind {
                    // Would need to get __enter__ return type
                    self.ctx.define(id.name, ctx_type);
                }
            }
        }

        for stmt in body {
            self.check_stmt(stmt);
        }

        self.ctx.pop_scope();
    }

    fn check_raise(&mut self, exc: Option<&Expr>, cause: Option<&Expr>, _span: Span) {
        if let Some(e) = exc {
            self.check_expr(e);
        }
        if let Some(c) = cause {
            self.check_expr(c);
        }
    }

    fn check_try(
        &mut self,
        body: &[Stmt],
        handlers: &[ExceptHandler],
        orelse: &[Stmt],
        finalbody: &[Stmt],
        _span: Span,
    ) {
        for stmt in body {
            self.check_stmt(stmt);
        }

        for handler in handlers {
            self.ctx.push_scope();

            if let Some(ty) = &handler.ty {
                let exc_type = self.check_expr(ty);
                if let Some(name) = &handler.name {
                    self.ctx.define(name.name, exc_type);
                }
            }

            for stmt in &handler.body {
                self.check_stmt(stmt);
            }

            self.ctx.pop_scope();
        }

        for stmt in orelse {
            self.check_stmt(stmt);
        }

        for stmt in finalbody {
            self.check_stmt(stmt);
        }
    }

    fn check_assert(&mut self, test: &Expr, msg: Option<&Expr>, span: Span) {
        let test_type = self.check_expr(test);
        self.check_bool_context(&test_type, span);

        if let Some(m) = msg {
            self.check_expr(m);
        }
    }

    fn check_match(&mut self, subject: &Expr, cases: &[MatchCase], _span: Span) {
        let subject_type = self.check_expr(subject);

        // Check each case
        for case in cases {
            self.ctx.push_scope();

            // Check pattern against subject type
            self.check_pattern(&case.pattern, &subject_type);

            // Check guard if present
            if let Some(guard) = &case.guard {
                let guard_type = self.check_expr(guard);
                self.check_bool_context(&guard_type, guard.span);
            }

            // Check body
            for stmt in &case.body {
                self.check_stmt(stmt);
            }

            self.ctx.pop_scope();
        }

        let patterns: Vec<_> = cases.iter().map(|c| &c.pattern).collect();
        let analysis = crate::exhaustiveness::analyze_patterns(&subject_type, &patterns);

        // Report non-exhaustive match
        if let crate::exhaustiveness::ExhaustivenessResult::NonExhaustive(witnesses) = &analysis.exhaustiveness {
            let missing: Vec<String> = witnesses.iter().map(|w| w.to_string()).collect();
            self.diagnostics.report(
                Diagnostic::error(format!(
                    "match is not exhaustive: missing patterns: {}",
                    missing.join(", ")
                ))
                .with_span(_span),
            );
        }

        // Report redundant arms as warnings
        for arm_idx in &analysis.redundant_arms {
            if let Some(case) = cases.get(*arm_idx) {
                self.diagnostics.report(
                    Diagnostic::warning(format!("unreachable pattern"))
                        .with_span(case.pattern.span),
                );
            }
        }
    }

    fn check_pattern(&mut self, pattern: &Pattern, subject_type: &Type) {
        match &pattern.kind {
            PatternKind::MatchValue { value } => {
                let value_type = self.check_expr(value);
                if !self.ctx.is_subtype(&value_type, subject_type) {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "pattern type mismatch: expected {}, got {}",
                            subject_type, value_type
                        ))
                        .with_span(pattern.span),
                    );
                }
            }
            PatternKind::Capture { name } => {
                self.ctx.define(name.name, subject_type.clone());
            }
            PatternKind::Wildcard => {}
            PatternKind::MatchAs { pattern: sub, name } => {
                if let Some(p) = sub {
                    self.check_pattern(p, subject_type);
                }
                if let Some(n) = name {
                    self.ctx.define(n.name, subject_type.clone());
                }
            }
            PatternKind::MatchOr { patterns } => {
                for p in patterns {
                    self.check_pattern(p, subject_type);
                }
            }
            PatternKind::MatchSequence { patterns } => {
                // Simplified sequence checking
                if let Type::List(elem_type) = subject_type {
                    for p in patterns {
                        self.check_pattern(p, elem_type);
                    }
                } else if let Type::Tuple(elem_types) = subject_type {
                    if patterns.len() != elem_types.len() {
                         self.diagnostics.report(
                            Diagnostic::error(format!(
                                "tuple pattern length mismatch: expected {}, got {}",
                                elem_types.len(), patterns.len()
                            ))
                            .with_span(pattern.span),
                        );
                    } else {
                        for (p, ty) in patterns.iter().zip(elem_types.iter()) {
                            self.check_pattern(p, ty);
                        }
                    }
                }
                // TODO: Handle other sequence types
            }
            // TODO: Implement other pattern kinds
            _ => {}
        }
    }

    /// Type checks an expression and returns its type.
    pub fn check_expr(&mut self, expr: &Expr) -> Type {
        let ty = self.check_expr_inner(expr);
        self.ctx.record_expr_type(expr.span, ty.clone());
        ty
    }

    fn check_expr_inner(&mut self, expr: &Expr) -> Type {
        match &expr.kind {
            ExprKind::IntLit { .. } => Type::Int,
            ExprKind::FloatLit { .. } => Type::Float,
            ExprKind::StringLit { .. } => Type::Str,
            ExprKind::BytesLit { .. } => Type::Bytes,
            ExprKind::BoolLit { .. } => Type::Bool,
            ExprKind::NoneLit => Type::NoneType,
            ExprKind::Ellipsis => Type::Any,

            ExprKind::Name { id, .. } => {
                if let Some(ty) = self.ctx.lookup(id.name) {
                    ty.clone()
                } else {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "undefined name: {}",
                            self.interner.resolve(id.name).unwrap_or("?")
                        ))
                        .with_span(expr.span),
                    );
                    Type::Error
                }
            }

            ExprKind::BinOp { left, op, right } => {
                let left_type = self.check_expr(left);
                let right_type = self.check_expr(right);
                self.check_binary_op(*op, &left_type, &right_type, expr.span)
            }

            ExprKind::UnaryOp { op, operand } => {
                let operand_type = self.check_expr(operand);
                self.check_unary_op(*op, &operand_type, expr.span)
            }

            ExprKind::BoolOp { values, .. } => {
                for val in values {
                    let ty = self.check_expr(val);
                    self.check_bool_context(&ty, val.span);
                }
                Type::Bool
            }

            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let mut prev_type = self.check_expr(left);
                for (op, comp) in ops.iter().zip(comparators.iter()) {
                    let comp_type = self.check_expr(comp);
                    self.check_comparison_op(*op, &prev_type, &comp_type, expr.span);
                    prev_type = comp_type;
                }
                Type::Bool
            }

            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                let func_type = self.check_expr(func);
                self.check_call(&func_type, args, keywords, expr.span)
            }

            ExprKind::Attribute { value, attr, .. } => {
                let value_type = self.check_expr(value);
                self.check_attribute(&value_type, attr, expr.span)
            }

            ExprKind::Subscript { value, slice, .. } => {
                let value_type = self.check_expr(value);
                let slice_type = self.check_expr(slice);
                self.check_subscript(&value_type, &slice_type, expr.span)
            }

            ExprKind::List { elts, .. } => {
                if elts.is_empty() {
                    Type::list(self.ctx.fresh_var())
                } else {
                    let elem_types: Vec<_> = elts.iter().map(|e| self.check_expr(e)).collect();
                    let unified = elem_types
                        .iter()
                        .skip(1)
                        .fold(elem_types[0].clone(), |acc, ty| self.ctx.join(&acc, ty));
                    Type::list(unified)
                }
            }

            ExprKind::Tuple { elts, .. } => {
                let elem_types: Vec<_> = elts.iter().map(|e| self.check_expr(e)).collect();
                Type::Tuple(elem_types)
            }

            ExprKind::Dict { keys, values } => {
                if keys.is_empty() {
                    Type::dict(self.ctx.fresh_var(), self.ctx.fresh_var())
                } else {
                    let key_types: Vec<_> = keys
                        .iter()
                        .filter_map(|k| k.as_ref().map(|k| self.check_expr(k)))
                        .collect();
                    let value_types: Vec<_> = values.iter().map(|v| self.check_expr(v)).collect();

                    let key_type = key_types
                        .iter()
                        .skip(1)
                        .fold(key_types.first().cloned().unwrap_or(Type::Any), |acc, ty| {
                            self.ctx.join(&acc, ty)
                        });
                    let value_type = value_types
                        .iter()
                        .skip(1)
                        .fold(value_types[0].clone(), |acc, ty| self.ctx.join(&acc, ty));

                    Type::dict(key_type, value_type)
                }
            }

            ExprKind::Set { elts } => {
                if elts.is_empty() {
                    Type::set(self.ctx.fresh_var())
                } else {
                    let elem_types: Vec<_> = elts.iter().map(|e| self.check_expr(e)).collect();
                    let unified = elem_types
                        .iter()
                        .skip(1)
                        .fold(elem_types[0].clone(), |acc, ty| self.ctx.join(&acc, ty));
                    Type::set(unified)
                }
            }

            ExprKind::IfExp { test, body, orelse } => {
                let test_type = self.check_expr(test);
                self.check_bool_context(&test_type, test.span);

                let body_type = self.check_expr(body);
                let else_type = self.check_expr(orelse);

                self.ctx.join(&body_type, &else_type)
            }

            ExprKind::Lambda { args, body } => {
                self.ctx.push_scope();

                let mut params = Vec::new();
                for arg in &args.args {
                    let ty = if let Some(ann) = &arg.annotation {
                        self.resolve_type_expr(ann)
                    } else {
                        self.ctx.fresh_var()
                    };
                    self.ctx.define(arg.name.name, ty.clone());
                    params.push(FuncParam {
                        name: Some(arg.name.name),
                        ty,
                        default: arg.default.is_some(),
                        kind: ParamKind::Regular,
                    });
                }

                let return_type = self.check_expr(body);
                self.ctx.pop_scope();

                Type::callable(params, return_type, false)
            }

            ExprKind::Await { value } => {
                if !self.in_async {
                    self.diagnostics.report(
                        Diagnostic::error("'await' used outside of async function")
                            .with_span(expr.span),
                    );
                }
                let awaited = self.check_expr(value);
                // Would unwrap coroutine/awaitable type here
                awaited
            }

            ExprKind::Yield { value } => {
                if let Some(val) = value {
                    self.check_expr(val)
                } else {
                    Type::NoneType
                }
            }

            ExprKind::YieldFrom { value } => {
                self.check_expr(value)
            }

            ExprKind::ListComp { elt, generators } => {
                self.ctx.push_scope();
                for gen in generators {
                    self.check_comprehension(gen);
                }
                let elem_type = self.check_expr(elt);
                self.ctx.pop_scope();
                Type::list(elem_type)
            }

            ExprKind::SetComp { elt, generators } => {
                self.ctx.push_scope();
                for gen in generators {
                    self.check_comprehension(gen);
                }
                let elem_type = self.check_expr(elt);
                self.ctx.pop_scope();
                Type::set(elem_type)
            }

            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.ctx.push_scope();
                for gen in generators {
                    self.check_comprehension(gen);
                }
                let key_type = self.check_expr(key);
                let value_type = self.check_expr(value);
                self.ctx.pop_scope();
                Type::dict(key_type, value_type)
            }

            ExprKind::GeneratorExp { elt, generators } => {
                self.ctx.push_scope();
                for gen in generators {
                    self.check_comprehension(gen);
                }
                let elem_type = self.check_expr(elt);
                self.ctx.pop_scope();
                // Generator type would be more complex
                Type::list(elem_type)
            }

            ExprKind::Try { value } => {
                // The ? operator: unwraps Result[T, E] to T or propagates E
                let value_type = self.check_expr(value);
                let resolved = self.ctx.resolve(&value_type);

                // Check if it's a Result type (Generic with base=Result)
                if let Type::Generic { base, args } = &resolved {
                    if let Type::Class(cls) = base.as_ref() {
                        if cls.name == "Result" && args.len() == 2 {
                            // Return the Ok type (first arg)
                            return args[0].clone();
                        }
                    }
                }

                // Check if it's an Optional type
                if let Type::Optional(inner) = &resolved {
                    return (**inner).clone();
                }

                // Not a Result or Optional - error
                self.diagnostics.report(
                    Diagnostic::error(format!(
                        "the `?` operator can only be applied to Result or Optional types, found '{}'",
                        resolved
                    ))
                    .with_span(expr.span),
                );
                Type::Error
            }

            _ => Type::Unknown,
        }
    }

    fn check_comprehension(&mut self, comp: &Comprehension) {
        let iter_type = self.check_expr(&comp.iter);
        let elem_type = self.get_iterator_element_type(&iter_type, comp.span);

        if let ExprKind::Name { id, .. } = &comp.target.kind {
            self.ctx.define(id.name, elem_type);
        }

        for cond in &comp.ifs {
            let cond_type = self.check_expr(cond);
            self.check_bool_context(&cond_type, cond.span);
        }
    }

    fn check_binary_op(&mut self, op: BinOp, left: &Type, right: &Type, span: Span) -> Type {
        let left = self.ctx.resolve(left);
        let right = self.ctx.resolve(right);

        // Numeric operations
        if left.is_numeric() && right.is_numeric() {
            return self.ctx.join(&left, &right);
        }

        // String operations
        if matches!(&left, Type::Str) && matches!(op, BinOp::Add | BinOp::Mult) {
            match op {
                BinOp::Add if matches!(&right, Type::Str) => return Type::Str,
                BinOp::Mult if right.is_integer() => return Type::Str,
                _ => {}
            }
        }

        // List operations
        if let Type::List(elem) = &left {
            match op {
                BinOp::Add => {
                    if let Type::List(other_elem) = &right {
                        return Type::list(self.ctx.join(elem, other_elem));
                    }
                }
                BinOp::Mult if right.is_integer() => return left.clone(),
                _ => {}
            }
        }

        self.diagnostics.report(
            Diagnostic::error(format!(
                "unsupported operand type(s) for {}: '{}' and '{}'",
                op.as_str(),
                left,
                right
            ))
            .with_span(span),
        );
        Type::Error
    }

    fn check_unary_op(&mut self, op: UnaryOp, operand: &Type, span: Span) -> Type {
        let operand = self.ctx.resolve(operand);

        match op {
            UnaryOp::Not => Type::Bool,
            UnaryOp::UAdd | UnaryOp::USub => {
                if operand.is_numeric() {
                    operand.clone()
                } else {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "bad operand type for unary {}: '{}'",
                            op.as_str(),
                            operand
                        ))
                        .with_span(span),
                    );
                    Type::Error
                }
            }
            UnaryOp::Invert => {
                if operand.is_integer() {
                    operand.clone()
                } else {
                    self.diagnostics.report(
                        Diagnostic::error(format!("bad operand type for unary ~: '{}'", operand))
                            .with_span(span),
                    );
                    Type::Error
                }
            }
        }
    }

    fn check_comparison_op(&mut self, op: CmpOp, left: &Type, right: &Type, span: Span) {
        let left = self.ctx.resolve(left);
        let right = self.ctx.resolve(right);

        match op {
            CmpOp::Eq | CmpOp::NotEq => {
                // Any types can be compared for equality
            }
            CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE => {
                if !self.ctx.is_subtype(&left, &right) && !self.ctx.is_subtype(&right, &left) {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "'{}' not supported between instances of '{}' and '{}'",
                            op.as_str(),
                            left,
                            right
                        ))
                        .with_span(span),
                    );
                }
            }
            CmpOp::Is | CmpOp::IsNot => {
                // Identity comparison works on any types
            }
            CmpOp::In | CmpOp::NotIn => {
                // Check that right is iterable
                self.get_iterator_element_type(&right, span);
            }
        }
    }

    fn check_call(
        &mut self,
        func_type: &Type,
        args: &[Expr],
        keywords: &[Keyword],
        span: Span,
    ) -> Type {
        let func_type = self.ctx.resolve(func_type);

        match &func_type {
            Type::Callable { params, returns, .. } => {
                // Check for variadic positional parameters (*args)
                let has_var_positional = params.iter().any(|p| matches!(p.kind, ParamKind::VarPositional));

                // Check positional arguments
                let required_count = params.iter()
                    .filter(|p| !p.default && !matches!(p.kind, ParamKind::VarPositional | ParamKind::VarKeyword))
                    .count();
                let max_positional = if has_var_positional {
                    usize::MAX
                } else {
                    params.iter()
                        .filter(|p| !matches!(p.kind, ParamKind::VarKeyword))
                        .count()
                };
                let provided = args.len();

                if provided < required_count {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "function takes at least {} arguments but {} were given",
                            required_count, provided
                        ))
                        .with_span(span),
                    );
                } else if provided > max_positional {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "function takes at most {} arguments but {} were given",
                            max_positional,
                            provided
                        ))
                        .with_span(span),
                    );
                }

                // Type check arguments (for non-variadic params)
                for (arg, param) in args.iter().zip(params.iter().filter(|p| !matches!(p.kind, ParamKind::VarPositional | ParamKind::VarKeyword))) {
                    let arg_type = self.check_expr(arg);
                    if !self.ctx.is_subtype(&arg_type, &param.ty) {
                        self.diagnostics.report(
                            Diagnostic::error(format!(
                                "argument type mismatch: expected {}, got {}",
                                param.ty, arg_type
                            ))
                            .with_span(arg.span),
                        );
                    }
                }

                // Check keyword arguments
                for kw in keywords {
                    self.check_expr(&kw.value);
                }

                (**returns).clone()
            }
            Type::Class(cls) => {
                // Constructor call - would look up __init__
                Type::Class(cls.clone())
            }
            Type::Any => Type::Any,
            Type::Error => Type::Error,
            _ => {
                self.diagnostics.report(
                    Diagnostic::error(format!("'{}' is not callable", func_type)).with_span(span),
                );
                Type::Error
            }
        }
    }

    fn check_attribute(&mut self, value_type: &Type, attr: &Ident, span: Span) -> Type {
        let value_type = self.ctx.resolve(value_type);

        match &value_type {
            Type::Class(cls) => {
                // Check if this is the current class being defined
                let cls_to_check = if let Some(Type::Class(current)) = &self.current_class {
                    if current.name == cls.name {
                        current
                    } else {
                        cls
                    }
                } else {
                    cls
                };

                // Look up attribute in class
                for (name, ty) in &cls_to_check.members {
                    if name == self.interner.resolve(attr.name).unwrap_or("") {
                        return ty.clone();
                    }
                }
                for (name, ty) in &cls_to_check.methods {
                    if name == self.interner.resolve(attr.name).unwrap_or("") {
                        // If it's a method, we need to bind it (remove self)
                        if let Type::Callable { params, returns, is_async } = ty {
                            if !params.is_empty() {
                                // Create a new Callable type with the first parameter removed
                                return Type::Callable {
                                    params: params[1..].to_vec(),
                                    returns: returns.clone(),
                                    is_async: *is_async,
                                };
                            }
                        }
                        return ty.clone();
                    }
                }
                self.diagnostics.report(
                    Diagnostic::error(format!(
                        "type '{}' has no attribute '{}'",
                        cls.name,
                        self.interner.resolve(attr.name).unwrap_or("?")
                    ))
                    .with_span(span),
                );
                Type::Error
            }
            Type::Any => Type::Any,
            Type::Error => Type::Error,
            _ => {
                // For now, allow attribute access on any type
                Type::Any
            }
        }
    }

    fn check_subscript(&mut self, value_type: &Type, slice_type: &Type, span: Span) -> Type {
        let value_type = self.ctx.resolve(value_type);

        match &value_type {
            Type::List(elem) => {
                if !slice_type.is_integer() {
                    self.diagnostics.report(
                        Diagnostic::error(format!(
                            "list indices must be integers, not {}",
                            slice_type
                        ))
                        .with_span(span),
                    );
                }
                (**elem).clone()
            }
            Type::Dict(_, value) => (**value).clone(),
            Type::Tuple(elems) => {
                // For literal integer indices, could return exact type
                if elems.is_empty() {
                    Type::Never
                } else {
                    Type::union(elems.clone())
                }
            }
            Type::Str => Type::Str,
            Type::Bytes => Type::Int,
            Type::Any => Type::Any,
            Type::Error => Type::Error,
            _ => {
                self.diagnostics.report(
                    Diagnostic::error(format!("'{}' is not subscriptable", value_type))
                        .with_span(span),
                );
                Type::Error
            }
        }
    }

    fn check_bool_context(&mut self, ty: &Type, _span: Span) {
        // All types can be used in boolean context in Python/Roast
        let _ = ty;
    }

    fn get_iterator_element_type(&mut self, iter_type: &Type, span: Span) -> Type {
        let iter_type = self.ctx.resolve(iter_type);

        match &iter_type {
            Type::List(elem) | Type::Set(elem) => (**elem).clone(),
            Type::Dict(key, _) => (**key).clone(),
            Type::Tuple(elems) => {
                if elems.is_empty() {
                    Type::Never
                } else {
                    Type::union(elems.clone())
                }
            }
            Type::Str => Type::Str,
            Type::Bytes => Type::Int,
            Type::Any => Type::Any,
            Type::Error => Type::Error,
            _ => {
                self.diagnostics.report(
                    Diagnostic::error(format!("'{}' is not iterable", iter_type)).with_span(span),
                );
                Type::Error
            }
        }
    }

    /// Resolves a type expression AST node to a Type.
    pub fn resolve_type_expr(&mut self, ty_expr: &TypeExpr) -> Type {
        match &ty_expr.kind {
            TypeExprKind::Name { name } => {
                let name_str = self.interner.resolve(name.name);
                match name_str {
                    Some("int") => Type::Int,
                    Some("float") => Type::Float,
                    Some("str") => Type::Str,
                    Some("bytes") => Type::Bytes,
                    Some("bool") => Type::Bool,
                    Some("None") => Type::NoneType,
                    Some("Any") => Type::Any,
                    Some("Never") => Type::Never,
                    Some("Self") => Type::SelfType,
                    Some("list") | Some("List") | Some("dict") | Some("Dict") | 
                    Some("set") | Some("Set") | Some("tuple") | Some("Tuple") |
                    Some("Optional") => {
                        // Treat these as type constructors when used in type expressions
                        self.ctx.fresh_named_var(name.name)
                    }
                    _ => {
                        if let Some(ty) = self.ctx.lookup(name.name) {
                            ty.clone()
                        } else {
                            // Unknown type - create a fresh type variable
                            self.ctx.fresh_named_var(name.name)
                        }
                    }
                }
            }
            TypeExprKind::Subscript { value, slice } => {
                let base = self.resolve_type_expr(value);
                let arg = self.resolve_type_expr(slice);

                match &base {
                    Type::Var(var) => {
                        let name_str = var.name.and_then(|n| self.interner.resolve(n));
                        match name_str {
                            Some("List") | Some("list") => Type::list(arg),
                            Some("Set") | Some("set") => Type::set(arg),
                            Some("Tuple") | Some("tuple") => {
                                // tuple[int, int] -> Tuple(vec![int, int])
                                if let Type::Tuple(args) = arg {
                                    Type::Tuple(args)
                                } else {
                                    // Single element tuple
                                    Type::Tuple(vec![arg])
                                }
                            },
                            Some("Dict") | Some("dict") => {
                                if let Type::Tuple(args) = &arg {
                                    if args.len() == 2 {
                                        Type::dict(args[0].clone(), args[1].clone())
                                    } else {
                                        Type::Error
                                    }
                                } else {
                                    Type::Error
                                }
                            },
                            Some("Optional") => Type::optional(arg),
                            _ => Type::Generic {
                                base: std::sync::Arc::new(base),
                                args: vec![arg],
                            },
                        }
                    }
                    _ => Type::Generic {
                        base: std::sync::Arc::new(base),
                        args: vec![arg],
                    },
                }
            }
            TypeExprKind::Tuple { elts } => {
                let elem_types: Vec<_> = elts.iter().map(|e| self.resolve_type_expr(e)).collect();
                Type::Tuple(elem_types)
            }
            TypeExprKind::Union { types } => {
                let type_list: Vec<_> = types.iter().map(|t| self.resolve_type_expr(t)).collect();
                Type::union(type_list)
            }
            TypeExprKind::Optional { inner } => {
                let inner_type = self.resolve_type_expr(inner);
                Type::optional(inner_type)
            }
            TypeExprKind::Callable { params, returns } => {
                let param_types: Vec<_> = params
                    .iter()
                    .map(|p| FuncParam {
                        name: None,
                        ty: self.resolve_type_expr(p),
                        default: false,
                        kind: ParamKind::Regular,
                    })
                    .collect();
                let return_type = self.resolve_type_expr(returns);
                Type::callable(param_types, return_type, false)
            }
            TypeExprKind::None => Type::NoneType,
            TypeExprKind::Any => Type::Any,
            TypeExprKind::SelfType => Type::SelfType,
            TypeExprKind::Never => Type::Never,
            TypeExprKind::Infer => self.ctx.fresh_var(),
            TypeExprKind::Ref { inner, mutable } => Type::Ref {
                inner: std::sync::Arc::new(self.resolve_type_expr(inner)),
                mutable: *mutable,
            },
            TypeExprKind::Owned { inner } => {
                Type::Owned(std::sync::Arc::new(self.resolve_type_expr(inner)))
            }
            _ => Type::Unknown,
        }
    }
}
