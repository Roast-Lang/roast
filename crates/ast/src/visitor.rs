//! AST visitor traits.

use crate::{Expr, ExprKind, Pattern, PatternKind, Stmt, StmtKind, TypeExpr, TypeExprKind};

/// A visitor that traverses the AST without modifying it.
pub trait Visitor<'ast>: Sized {
    /// Visit a statement.
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        walk_stmt(self, stmt);
    }

    /// Visit an expression.
    fn visit_expr(&mut self, expr: &'ast Expr) {
        walk_expr(self, expr);
    }

    /// Visit a type expression.
    fn visit_type_expr(&mut self, ty: &'ast TypeExpr) {
        walk_type_expr(self, ty);
    }

    /// Visit a pattern.
    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        walk_pattern(self, pattern);
    }
}

/// Walk a statement, visiting all children.
pub fn walk_stmt<'ast, V: Visitor<'ast>>(visitor: &mut V, stmt: &'ast Stmt) {
    match &stmt.kind {
        StmtKind::FunctionDef {
            args,
            body,
            decorators,
            returns,
            ..
        } => {
            for decorator in decorators {
                visitor.visit_expr(&decorator.name);
                for arg in &decorator.arguments {
                    visitor.visit_expr(arg);
                }
            }
            for arg in &args.args {
                if let Some(ann) = &arg.annotation {
                    visitor.visit_type_expr(ann);
                }
                if let Some(default) = &arg.default {
                    visitor.visit_expr(default);
                }
            }
            if let Some(ret) = returns {
                visitor.visit_type_expr(ret);
            }
            for s in body {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::ClassDef {
            bases,
            keywords,
            body,
            decorators,
            ..
        } => {
            for decorator in decorators {
                visitor.visit_expr(&decorator.name);
            }
            for base in bases {
                visitor.visit_expr(base);
            }
            for kw in keywords {
                visitor.visit_expr(&kw.value);
            }
            for s in body {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::Return { value } => {
            if let Some(v) = value {
                visitor.visit_expr(v);
            }
        }
        StmtKind::Delete { targets } => {
            for target in targets {
                visitor.visit_expr(target);
            }
        }
        StmtKind::Assign { targets, value } => {
            for target in targets {
                visitor.visit_expr(target);
            }
            visitor.visit_expr(value);
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
            ..
        } => {
            visitor.visit_expr(target);
            visitor.visit_type_expr(annotation);
            if let Some(v) = value {
                visitor.visit_expr(v);
            }
        }
        StmtKind::AugAssign { target, value, .. } => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }
        StmtKind::For {
            target,
            iter,
            body,
            orelse,
            target_annotation,
            ..
        } => {
            visitor.visit_expr(target);
            if let Some(ann) = target_annotation {
                visitor.visit_type_expr(ann);
            }
            visitor.visit_expr(iter);
            for s in body {
                visitor.visit_stmt(s);
            }
            for s in orelse {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::While { test, body, orelse } => {
            visitor.visit_expr(test);
            for s in body {
                visitor.visit_stmt(s);
            }
            for s in orelse {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::If { test, body, orelse } => {
            visitor.visit_expr(test);
            for s in body {
                visitor.visit_stmt(s);
            }
            for s in orelse {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::With { items, body, .. } => {
            for item in items {
                visitor.visit_expr(&item.context_expr);
                if let Some(vars) = &item.optional_vars {
                    visitor.visit_expr(vars);
                }
            }
            for s in body {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::Match { subject, cases } => {
            visitor.visit_expr(subject);
            for case in cases {
                visitor.visit_pattern(&case.pattern);
                if let Some(guard) = &case.guard {
                    visitor.visit_expr(guard);
                }
                for s in &case.body {
                    visitor.visit_stmt(s);
                }
            }
        }
        StmtKind::Raise { exc, cause } => {
            if let Some(e) = exc {
                visitor.visit_expr(e);
            }
            if let Some(c) = cause {
                visitor.visit_expr(c);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        }
        | StmtKind::TryStar {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            for s in body {
                visitor.visit_stmt(s);
            }
            for handler in handlers {
                if let Some(ty) = &handler.ty {
                    visitor.visit_expr(ty);
                }
                for s in &handler.body {
                    visitor.visit_stmt(s);
                }
            }
            for s in orelse {
                visitor.visit_stmt(s);
            }
            for s in finalbody {
                visitor.visit_stmt(s);
            }
        }
        StmtKind::Assert { test, msg } => {
            visitor.visit_expr(test);
            if let Some(m) = msg {
                visitor.visit_expr(m);
            }
        }
        StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => {}
        StmtKind::Global { .. } | StmtKind::Nonlocal { .. } => {}
        StmtKind::Expr { value } => {
            visitor.visit_expr(value);
        }
        StmtKind::Pass | StmtKind::Break | StmtKind::Continue => {}
        StmtKind::TypeAlias { value, .. } => {
            visitor.visit_type_expr(value);
        }
    }
}

/// Walk an expression, visiting all children.
pub fn walk_expr<'ast, V: Visitor<'ast>>(visitor: &mut V, expr: &'ast Expr) {
    match &expr.kind {
        ExprKind::BoolOp { values, .. } => {
            for v in values {
                visitor.visit_expr(v);
            }
        }
        ExprKind::NamedExpr { target, value } => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }
        ExprKind::BinOp { left, right, .. } => {
            visitor.visit_expr(left);
            visitor.visit_expr(right);
        }
        ExprKind::UnaryOp { operand, .. } => {
            visitor.visit_expr(operand);
        }
        ExprKind::Lambda { body, .. } => {
            visitor.visit_expr(body);
        }
        ExprKind::IfExp { test, body, orelse } => {
            visitor.visit_expr(test);
            visitor.visit_expr(body);
            visitor.visit_expr(orelse);
        }
        ExprKind::Dict { keys, values } => {
            for k in keys.iter().flatten() {
                visitor.visit_expr(k);
            }
            for v in values {
                visitor.visit_expr(v);
            }
        }
        ExprKind::Set { elts } | ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
            for e in elts {
                visitor.visit_expr(e);
            }
        }
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            visitor.visit_expr(elt);
            for gen in generators {
                visitor.visit_expr(&gen.target);
                visitor.visit_expr(&gen.iter);
                for cond in &gen.ifs {
                    visitor.visit_expr(cond);
                }
            }
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            visitor.visit_expr(key);
            visitor.visit_expr(value);
            for gen in generators {
                visitor.visit_expr(&gen.target);
                visitor.visit_expr(&gen.iter);
                for cond in &gen.ifs {
                    visitor.visit_expr(cond);
                }
            }
        }
        ExprKind::Await { value } | ExprKind::YieldFrom { value } => {
            visitor.visit_expr(value);
        }
        ExprKind::Yield { value } => {
            if let Some(v) = value {
                visitor.visit_expr(v);
            }
        }
        ExprKind::Compare {
            left, comparators, ..
        } => {
            visitor.visit_expr(left);
            for c in comparators {
                visitor.visit_expr(c);
            }
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            visitor.visit_expr(func);
            for a in args {
                visitor.visit_expr(a);
            }
            for kw in keywords {
                visitor.visit_expr(&kw.value);
            }
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            visitor.visit_expr(value);
            if let Some(spec) = format_spec {
                visitor.visit_expr(spec);
            }
        }
        ExprKind::JoinedStr { values } => {
            for v in values {
                visitor.visit_expr(v);
            }
        }
        ExprKind::Attribute { value, .. } => {
            visitor.visit_expr(value);
        }
        ExprKind::Subscript { value, slice, .. } => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
        }
        ExprKind::Starred { value, .. } => {
            visitor.visit_expr(value);
        }
        ExprKind::Slice { lower, upper, step } => {
            if let Some(l) = lower {
                visitor.visit_expr(l);
            }
            if let Some(u) = upper {
                visitor.visit_expr(u);
            }
            if let Some(s) = step {
                visitor.visit_expr(s);
            }
        }
        ExprKind::TypeAnnotation { value, annotation } => {
            visitor.visit_expr(value);
            visitor.visit_type_expr(annotation);
        }
        ExprKind::OwnershipExpr { value, .. } => {
            visitor.visit_expr(value);
        }
        ExprKind::Try { value } => {
            visitor.visit_expr(value);
        }
        ExprKind::Name { .. }
        | ExprKind::IntLit { .. }
        | ExprKind::FloatLit { .. }
        | ExprKind::ComplexLit { .. }
        | ExprKind::StringLit { .. }
        | ExprKind::BytesLit { .. }
        | ExprKind::BoolLit { .. }
        | ExprKind::Ellipsis
        | ExprKind::NoneLit => {}
    }
}

/// Walk a type expression, visiting all children.
pub fn walk_type_expr<'ast, V: Visitor<'ast>>(visitor: &mut V, ty: &'ast TypeExpr) {
    match &ty.kind {
        TypeExprKind::Attribute { value, .. } => {
            visitor.visit_type_expr(value);
        }
        TypeExprKind::Subscript { value, slice } => {
            visitor.visit_type_expr(value);
            visitor.visit_type_expr(slice);
        }
        TypeExprKind::Tuple { elts } | TypeExprKind::Union { types: elts } => {
            for e in elts {
                visitor.visit_type_expr(e);
            }
        }
        TypeExprKind::Optional { inner }
        | TypeExprKind::Ref { inner, .. }
        | TypeExprKind::Owned { inner }
        | TypeExprKind::Borrowed { inner, .. }
        | TypeExprKind::Unpack { inner } => {
            visitor.visit_type_expr(inner);
        }
        TypeExprKind::Callable { params, returns } => {
            for p in params {
                visitor.visit_type_expr(p);
            }
            visitor.visit_type_expr(returns);
        }
        TypeExprKind::Name { .. }
        | TypeExprKind::Literal { .. }
        | TypeExprKind::StringAnnotation { .. }
        | TypeExprKind::None
        | TypeExprKind::Any
        | TypeExprKind::SelfType
        | TypeExprKind::TypeVar { .. }
        | TypeExprKind::TypeVarTuple { .. }
        | TypeExprKind::ParamSpec { .. }
        | TypeExprKind::Never
        | TypeExprKind::Infer => {}
    }
}

/// Walk a pattern, visiting all children.
pub fn walk_pattern<'ast, V: Visitor<'ast>>(visitor: &mut V, pattern: &'ast Pattern) {
    match &pattern.kind {
        PatternKind::MatchValue { value } | PatternKind::MatchSingleton { value } => {
            visitor.visit_expr(value);
        }
        PatternKind::MatchSequence { patterns } | PatternKind::MatchOr { patterns } => {
            for p in patterns {
                visitor.visit_pattern(p);
            }
        }
        PatternKind::MatchMapping { keys, patterns, .. } => {
            for k in keys {
                visitor.visit_expr(k);
            }
            for p in patterns {
                visitor.visit_pattern(p);
            }
        }
        PatternKind::MatchClass {
            cls,
            patterns,
            kwd_patterns,
            ..
        } => {
            visitor.visit_expr(cls);
            for p in patterns {
                visitor.visit_pattern(p);
            }
            for p in kwd_patterns {
                visitor.visit_pattern(p);
            }
        }
        PatternKind::MatchAs { pattern, .. } => {
            if let Some(p) = pattern {
                visitor.visit_pattern(p);
            }
        }
        PatternKind::MatchStar { .. } | PatternKind::Wildcard | PatternKind::Capture { .. } => {}
    }
}

/// A mutable visitor that can transform the AST.
pub trait MutVisitor: Sized {
    fn visit_stmt_mut(&mut self, stmt: &mut Stmt) {
        walk_stmt_mut(self, stmt);
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        walk_expr_mut(self, expr);
    }

    fn visit_type_expr_mut(&mut self, ty: &mut TypeExpr) {
        walk_type_expr_mut(self, ty);
    }
}

/// Walk a statement mutably.
pub fn walk_stmt_mut<V: MutVisitor>(visitor: &mut V, stmt: &mut Stmt) {
    match &mut stmt.kind {
        StmtKind::FunctionDef {
            body,
            decorators,
            returns,
            args,
            ..
        } => {
            for decorator in decorators {
                visitor.visit_expr_mut(&mut decorator.name);
            }
            for arg in &mut args.args {
                if let Some(ann) = &mut arg.annotation {
                    visitor.visit_type_expr_mut(ann);
                }
            }
            if let Some(ret) = returns {
                visitor.visit_type_expr_mut(ret);
            }
            for s in body {
                visitor.visit_stmt_mut(s);
            }
        }
        StmtKind::Expr { value } => {
            visitor.visit_expr_mut(value);
        }
        // ... more cases would go here
        _ => {}
    }
}

/// Walk an expression mutably.
pub fn walk_expr_mut<V: MutVisitor>(visitor: &mut V, expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::BinOp { left, right, .. } => {
            visitor.visit_expr_mut(left);
            visitor.visit_expr_mut(right);
        }
        ExprKind::UnaryOp { operand, .. } => {
            visitor.visit_expr_mut(operand);
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            visitor.visit_expr_mut(func);
            for a in args {
                visitor.visit_expr_mut(a);
            }
            for kw in keywords {
                visitor.visit_expr_mut(&mut kw.value);
            }
        }
        // ... more cases would go here
        _ => {}
    }
}

/// Walk a type expression mutably.
pub fn walk_type_expr_mut<V: MutVisitor>(visitor: &mut V, ty: &mut TypeExpr) {
    match &mut ty.kind {
        TypeExprKind::Subscript { value, slice } => {
            visitor.visit_type_expr_mut(value);
            visitor.visit_type_expr_mut(slice);
        }
        TypeExprKind::Union { types } => {
            for t in types {
                visitor.visit_type_expr_mut(t);
            }
        }
        _ => {}
    }
}

