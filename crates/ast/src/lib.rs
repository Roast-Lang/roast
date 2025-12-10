//! Abstract Syntax Tree for the Roast language.
//!
//! This crate defines the AST nodes that represent Roast source code.
//! The structure is designed to be compatible with Python's AST while
//! supporting Roast's additional features like static typing and ownership.

pub mod expr;
pub mod macros;
pub mod module;
pub mod operators;
pub mod pattern;
pub mod stmt;
pub mod types;
pub mod visitor;

pub use expr::*;
pub use module::*;
pub use operators::*;
pub use pattern::*;
pub use stmt::*;
pub use types::*;
pub use visitor::*;

use roast_common::{Span, Symbol};

/// A module containing all AST nodes.
pub mod prelude {
    pub use crate::expr::*;
    pub use crate::module::*;
    pub use crate::operators::*;
    pub use crate::pattern::*;
    pub use crate::stmt::*;
    pub use crate::types::*;
    pub use crate::visitor::*;
}

/// Identifier with span information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub name: Symbol,
    pub span: Span,
}

impl Ident {
    pub fn new(name: Symbol, span: Span) -> Self {
        Self { name, span }
    }

    pub fn dummy(name: Symbol) -> Self {
        Self {
            name,
            span: Span::dummy(),
        }
    }
}

/// Docstring extracted from source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocString {
    pub content: String,
    pub span: Span,
}

/// Decorator applied to a function or class.
#[derive(Clone, Debug)]
pub struct Decorator {
    pub name: Expr,
    pub arguments: Vec<Expr>,
    pub keywords: Vec<Keyword>,
    pub span: Span,
}

/// A keyword argument in a function call.
#[derive(Clone, Debug)]
pub struct Keyword {
    pub name: Option<Ident>,
    pub value: Expr,
    pub span: Span,
}

/// Comprehension clause (for x in iter if cond).
#[derive(Clone, Debug)]
pub struct Comprehension {
    pub target: Box<Expr>,
    pub iter: Box<Expr>,
    pub ifs: Vec<Expr>,
    pub is_async: bool,
    pub span: Span,
}

/// Exception handler in try/except.
#[derive(Clone, Debug)]
pub struct ExceptHandler {
    pub ty: Option<Box<Expr>>,
    pub name: Option<Ident>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Match case for pattern matching.
#[derive(Clone, Debug)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// With item (context manager).
#[derive(Clone, Debug)]
pub struct WithItem {
    pub context_expr: Box<Expr>,
    pub optional_vars: Option<Box<Expr>>,
    pub span: Span,
}

/// Alias in import statements.
#[derive(Clone, Debug)]
pub struct Alias {
    pub name: Ident,
    pub asname: Option<Ident>,
    pub span: Span,
}

/// Function/method argument.
#[derive(Clone, Debug)]
pub struct Arg {
    pub name: Ident,
    pub annotation: Option<Box<TypeExpr>>,
    pub default: Option<Box<Expr>>,
    pub ownership: Ownership,
    pub span: Span,
}

/// Function arguments specification.
#[derive(Clone, Debug, Default)]
pub struct Arguments {
    pub posonlyargs: Vec<Arg>,
    pub args: Vec<Arg>,
    pub vararg: Option<Box<Arg>>,
    pub kwonlyargs: Vec<Arg>,
    pub kw_defaults: Vec<Option<Expr>>,
    pub kwarg: Option<Box<Arg>>,
    pub defaults: Vec<Expr>,
}

impl Arguments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.posonlyargs.is_empty()
            && self.args.is_empty()
            && self.vararg.is_none()
            && self.kwonlyargs.is_empty()
            && self.kwarg.is_none()
    }
}

/// Ownership/mutability modifier for Roast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Ownership {
    #[default]
    None,
    /// Mutable reference
    Mut,
    /// Immutable reference
    Imm,
    /// Borrowed reference
    Borrow,
    /// Moved ownership
    Move,
    /// Owned value
    Own,
}

impl Ownership {
    pub fn is_none(&self) -> bool {
        matches!(self, Ownership::None)
    }
}

