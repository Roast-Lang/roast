//! Statement AST nodes.

use crate::{
    Alias, Arguments, Decorator, ExceptHandler, Expr, Ident, MatchCase, Ownership, TypeExpr,
    WithItem,
};
use roast_common::Span;

/// A statement node.
#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Statement kinds.
#[derive(Clone, Debug)]
pub enum StmtKind {
    /// Function definition: def name(args) -> returns: body
    FunctionDef {
        name: Ident,
        args: Arguments,
        body: Vec<Stmt>,
        decorators: Vec<Decorator>,
        returns: Option<Box<TypeExpr>>,
        type_params: Vec<TypeParam>,
        where_clause: Option<WhereClause>,
        is_async: bool,
    },

    /// Class definition: class Name(bases): body
    ClassDef {
        name: Ident,
        bases: Vec<Expr>,
        keywords: Vec<crate::Keyword>,
        body: Vec<Stmt>,
        decorators: Vec<Decorator>,
        type_params: Vec<TypeParam>,
        where_clause: Option<WhereClause>,
    },

    /// Return statement: return value
    Return {
        value: Option<Box<Expr>>,
    },

    /// Delete statement: del targets
    Delete {
        targets: Vec<Expr>,
    },

    /// Assignment: targets = value
    Assign {
        targets: Vec<Expr>,
        value: Box<Expr>,
    },

    /// Annotated assignment: target: annotation = value
    AnnAssign {
        target: Box<Expr>,
        annotation: Box<TypeExpr>,
        value: Option<Box<Expr>>,
        simple: bool,
        ownership: Ownership,
    },

    /// Augmented assignment: target op= value
    AugAssign {
        target: Box<Expr>,
        op: crate::AugOp,
        value: Box<Expr>,
    },

    /// For loop: for target in iter: body else: orelse
    For {
        target: Box<Expr>,
        iter: Box<Expr>,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
        is_async: bool,
        /// Roast extension: type annotation on loop variable
        target_annotation: Option<Box<TypeExpr>>,
    },

    /// While loop: while test: body else: orelse
    While {
        test: Box<Expr>,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },

    /// If statement: if test: body elif...: ... else: orelse
    If {
        test: Box<Expr>,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },

    /// With statement: with items: body
    With {
        items: Vec<WithItem>,
        body: Vec<Stmt>,
        is_async: bool,
    },

    /// Match statement (Python 3.10+): match subject: cases
    Match {
        subject: Box<Expr>,
        cases: Vec<MatchCase>,
    },

    /// Raise statement: raise exc from cause
    Raise {
        exc: Option<Box<Expr>>,
        cause: Option<Box<Expr>>,
    },

    /// Try statement: try: body except handlers: ... else: orelse finally: finalbody
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },

    /// Try-star statement (Python 3.11+): try: body except* handlers: ...
    TryStar {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },

    /// Assert statement: assert test, msg
    Assert {
        test: Box<Expr>,
        msg: Option<Box<Expr>>,
    },

    /// Import statement: import names
    Import {
        names: Vec<Alias>,
    },

    /// From import: from module import names
    ImportFrom {
        module: Option<Ident>,
        names: Vec<Alias>,
        level: u32,
    },

    /// Global declaration: global names
    Global {
        names: Vec<Ident>,
    },

    /// Nonlocal declaration: nonlocal names
    Nonlocal {
        names: Vec<Ident>,
    },

    /// Expression statement
    Expr {
        value: Box<Expr>,
    },

    /// Pass statement
    Pass,

    /// Break statement
    Break,

    /// Continue statement
    Continue,

    /// Type alias: type Name = Type
    TypeAlias {
        name: Ident,
        type_params: Vec<TypeParam>,
        value: Box<TypeExpr>,
    },
}

/// Type parameter for generic definitions.
#[derive(Clone, Debug)]
pub struct TypeParam {
    pub name: Ident,
    pub bound: Option<Box<TypeExpr>>,
    pub default: Option<Box<TypeExpr>>,
    pub kind: TypeParamKind,
    pub span: Span,
}

/// Kind of type parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeParamKind {
    /// Regular type variable: T
    TypeVar,
    /// Type variable tuple: *Ts
    TypeVarTuple,
    /// Parameter specification: **P
    ParamSpec,
}

/// A where clause that adds trait bounds to type parameters.
/// Example: `where T: Hashable + Eq, U: Printable`
#[derive(Clone, Debug)]
pub struct WhereClause {
    /// Individual constraints in the where clause
    pub constraints: Vec<WhereConstraint>,
    pub span: Span,
}

/// A single constraint in a where clause.
/// Example: `T: Hashable + Eq`
#[derive(Clone, Debug)]
pub struct WhereConstraint {
    /// The type parameter being constrained (e.g., `T`)
    pub type_param: Ident,
    /// The trait/protocol bounds (e.g., `[Hashable, Eq]`)
    pub bounds: Vec<TypeExpr>,
    pub span: Span,
}

