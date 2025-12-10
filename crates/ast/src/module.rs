//! Module-level AST nodes.

use crate::{DocString, Stmt};
use roast_common::{FileId, Span};

/// A complete Roast module (file).
#[derive(Clone, Debug)]
pub struct Module {
    pub file_id: FileId,
    pub name: String,
    pub docstring: Option<DocString>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

impl Module {
    pub fn new(file_id: FileId, name: String, body: Vec<Stmt>, span: Span) -> Self {
        let docstring = body.first().and_then(|stmt| {
            if let crate::StmtKind::Expr { value } = &stmt.kind {
                if let crate::ExprKind::StringLit { value: s, .. } = &value.kind {
                    return Some(DocString {
                        content: s.clone(),
                        span: stmt.span,
                    });
                }
            }
            None
        });

        Self {
            file_id,
            name,
            docstring,
            body,
            span,
        }
    }
}

/// Interactive mode (REPL) input.
#[derive(Clone, Debug)]
pub struct Interactive {
    pub body: Vec<Stmt>,
}

/// Expression mode (for eval).
#[derive(Clone, Debug)]
pub struct Expression {
    pub body: Box<crate::Expr>,
}

/// Type comment or stub file module.
#[derive(Clone, Debug)]
pub struct TypeStub {
    pub body: Vec<Stmt>,
}

