//! Lint checks for common programming pitfalls.
//!
//! This module implements warnings and errors for patterns that are technically
//! valid but likely to cause bugs, following the principle of making common
//! mistakes hard.

use roast_ast::{Arguments, Expr, ExprKind, Stmt, StmtKind};
use roast_common::{Diagnostic, DiagnosticSink, Span};

/// Collection of lint checks.
pub struct LintChecker<'a> {
    diagnostics: &'a mut DiagnosticSink,
    /// Whether to treat mutable default arguments as errors (strict mode)
    strict_mutable_defaults: bool,
}

impl<'a> LintChecker<'a> {
    /// Create a new lint checker.
    pub fn new(diagnostics: &'a mut DiagnosticSink) -> Self {
        Self {
            diagnostics,
            strict_mutable_defaults: false,
        }
    }

    /// Enable strict mode where mutable defaults are errors.
    pub fn with_strict_mutable_defaults(mut self, strict: bool) -> Self {
        self.strict_mutable_defaults = strict;
        self
    }

    /// Check an entire module for lint issues.
    pub fn check_module(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.check_stmt(stmt);
        }
    }

    /// Check a single statement for lint issues.
    pub fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::FunctionDef { args, body, .. } => {
                self.check_mutable_default_args(args);
                for s in body {
                    self.check_stmt(s);
                }
            }
            StmtKind::ClassDef { body, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            StmtKind::If { body, orelse, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
                for s in orelse {
                    self.check_stmt(s);
                }
            }
            StmtKind::While { body, orelse, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
                for s in orelse {
                    self.check_stmt(s);
                }
            }
            StmtKind::For { body, orelse, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
                for s in orelse {
                    self.check_stmt(s);
                }
            }
            StmtKind::With { body, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for s in body {
                    self.check_stmt(s);
                }
                for handler in handlers {
                    for s in &handler.body {
                        self.check_stmt(s);
                    }
                }
                for s in orelse {
                    self.check_stmt(s);
                }
                for s in finalbody {
                    self.check_stmt(s);
                }
            }
            StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for s in body {
                    self.check_stmt(s);
                }
                for handler in handlers {
                    for s in &handler.body {
                        self.check_stmt(s);
                    }
                }
                for s in orelse {
                    self.check_stmt(s);
                }
                for s in finalbody {
                    self.check_stmt(s);
                }
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    for s in &case.body {
                        self.check_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    /// Check for mutable default arguments (classic Python pitfall).
    ///
    /// Warns when a function has a default argument that is a mutable value
    /// like `[]`, `{}`, or `set()`. This is dangerous because the default
    /// value is shared between all calls to the function.
    ///
    /// # Example (Bad)
    /// ```roast
    /// def append_to(item, lst: list = []):
    ///     lst.append(item)
    ///     return lst
    ///
    /// append_to(1)  # Returns [1]
    /// append_to(2)  # Returns [1, 2] - Bug! Same list is reused!
    /// ```
    ///
    /// # Example (Good)
    /// ```roast
    /// def append_to(item, lst: list = None):
    ///     if lst is None:
    ///         lst = []
    ///     lst.append(item)
    ///     return lst
    /// ```
    fn check_mutable_default_args(&mut self, args: &Arguments) {
        // Check all argument groups
        for arg in args.posonlyargs.iter()
            .chain(args.args.iter())
            .chain(args.kwonlyargs.iter())
        {
            if let Some(default) = &arg.default {
                if let Some(mutable_kind) = self.classify_mutable_default(default) {
                    self.report_mutable_default(&default.span, mutable_kind);
                }
            }
        }

        // Also check defaults Vec (for backwards compatibility with Python AST)
        for default in &args.defaults {
            if let Some(mutable_kind) = self.classify_mutable_default(default) {
                self.report_mutable_default(&default.span, mutable_kind);
            }
        }

        // Check kw_defaults
        for default in args.kw_defaults.iter().flatten() {
            if let Some(mutable_kind) = self.classify_mutable_default(default) {
                self.report_mutable_default(&default.span, mutable_kind);
            }
        }
    }

    /// Classify what kind of mutable default an expression is, if any.
    fn classify_mutable_default(&self, expr: &Expr) -> Option<MutableDefaultKind> {
        match &expr.kind {
            // Empty list literal: []
            ExprKind::List { elts, .. } => {
                Some(MutableDefaultKind::List { empty: elts.is_empty() })
            }
            // Dict literal: {} or {k: v}
            ExprKind::Dict { keys, .. } => {
                Some(MutableDefaultKind::Dict { empty: keys.is_empty() })
            }
            // Set literal: {a, b}
            ExprKind::Set { elts, .. } => {
                Some(MutableDefaultKind::Set { empty: elts.is_empty() })
            }
            // List/dict/set comprehensions are also mutable
            ExprKind::ListComp { .. } => Some(MutableDefaultKind::ListComprehension),
            ExprKind::DictComp { .. } => Some(MutableDefaultKind::DictComprehension),
            ExprKind::SetComp { .. } => Some(MutableDefaultKind::SetComprehension),
            // Calls to known mutable constructors
            ExprKind::Call { func, .. } => {
                self.is_mutable_constructor_call(func)
            }
            _ => None,
        }
    }

    /// Check if a call expression is to a mutable constructor.
    fn is_mutable_constructor_call(&self, func: &Expr) -> Option<MutableDefaultKind> {
        // For now, we only detect direct Name calls to list(), dict(), set()
        // This is a simplified check - in production, we'd look up the symbol
        // and check if it's a known mutable constructor
        if let ExprKind::Name { .. } = &func.kind {
            // Without interner access, we can't easily check the name
            // Future enhancement: pass interner to the lint checker
            None
        } else {
            None
        }
    }

    /// Report a mutable default argument warning/error.
    fn report_mutable_default(&mut self, span: &Span, kind: MutableDefaultKind) {
        let msg = format!(
            "mutable default argument: `{}` is shared between all function calls",
            kind.type_name(),
        );
        
        let help = match kind {
            MutableDefaultKind::List { .. } => {
                "use `None` as default and create a new list in the function body"
            }
            MutableDefaultKind::Dict { .. } => {
                "use `None` as default and create a new dict in the function body"
            }
            MutableDefaultKind::Set { .. } => {
                "use `None` as default and create a new set in the function body"
            }
            _ => {
                "mutable defaults are shared between all calls, leading to bugs"
            }
        };

        let diagnostic = if self.strict_mutable_defaults {
            Diagnostic::error(msg)
                .with_span(*span)
                .with_note(help.to_string())
        } else {
            Diagnostic::warning(msg)
                .with_span(*span)
                .with_note(help.to_string())
        };

        self.diagnostics.report(diagnostic);
    }
}

/// Classification of mutable default argument types.
#[derive(Debug, Clone, Copy)]
pub enum MutableDefaultKind {
    /// List literal `[]` or `[a, b, c]`
    List { empty: bool },
    /// Dict literal `{}` or `{k: v}`
    Dict { empty: bool },
    /// Set literal `{a, b}`
    Set { empty: bool },
    /// List comprehension `[x for x in ...]`
    ListComprehension,
    /// Dict comprehension `{k: v for ...}`
    DictComprehension,
    /// Set comprehension `{x for x in ...}`
    SetComprehension,
    /// Call to list() constructor
    ListCall,
    /// Call to dict() constructor
    DictCall,
    /// Call to set() constructor
    SetCall,
}

impl MutableDefaultKind {
    /// Get a human-readable type name for this kind.
    pub fn type_name(&self) -> &'static str {
        match self {
            MutableDefaultKind::List { .. } => "list",
            MutableDefaultKind::Dict { .. } => "dict",
            MutableDefaultKind::Set { .. } => "set",
            MutableDefaultKind::ListComprehension => "list comprehension",
            MutableDefaultKind::DictComprehension => "dict comprehension",
            MutableDefaultKind::SetComprehension => "set comprehension",
            MutableDefaultKind::ListCall => "list()",
            MutableDefaultKind::DictCall => "dict()",
            MutableDefaultKind::SetCall => "set()",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a simple expression for testing
    fn make_list_expr(span: Span) -> Expr {
        Expr::new(ExprKind::List { elts: vec![], ctx: roast_ast::ExprContext::Load }, span)
    }

    fn make_dict_expr(span: Span) -> Expr {
        Expr::new(ExprKind::Dict { keys: vec![], values: vec![] }, span)
    }

    fn make_none_expr(span: Span) -> Expr {
        Expr::new(ExprKind::NoneLit, span)
    }

    #[test]
    fn test_classify_list_as_mutable() {
        let mut sink = DiagnosticSink::default();
        let checker = LintChecker::new(&mut sink);
        
        let list_expr = make_list_expr(Span::default());
        let result = checker.classify_mutable_default(&list_expr);
        
        assert!(matches!(result, Some(MutableDefaultKind::List { empty: true })));
    }

    #[test]
    fn test_classify_dict_as_mutable() {
        let mut sink = DiagnosticSink::default();
        let checker = LintChecker::new(&mut sink);
        
        let dict_expr = make_dict_expr(Span::default());
        let result = checker.classify_mutable_default(&dict_expr);
        
        assert!(matches!(result, Some(MutableDefaultKind::Dict { empty: true })));
    }

    #[test]
    fn test_classify_none_as_not_mutable() {
        let mut sink = DiagnosticSink::default();
        let checker = LintChecker::new(&mut sink);
        
        let none_expr = make_none_expr(Span::default());
        let result = checker.classify_mutable_default(&none_expr);
        
        assert!(result.is_none());
    }

    #[test]
    fn test_mutable_kind_type_names() {
        assert_eq!(MutableDefaultKind::List { empty: true }.type_name(), "list");
        assert_eq!(MutableDefaultKind::Dict { empty: false }.type_name(), "dict");
        assert_eq!(MutableDefaultKind::Set { empty: true }.type_name(), "set");
        assert_eq!(MutableDefaultKind::ListComprehension.type_name(), "list comprehension");
    }
}
