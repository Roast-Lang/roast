//! Expression AST nodes.

use crate::{Comprehension, Ident, Keyword, Ownership, TypeExpr};
use num_bigint::BigInt;
use roast_common::Span;

/// An expression node.
#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Expression kinds.
#[derive(Clone, Debug)]
pub enum ExprKind {
    /// Boolean operation: and/or
    BoolOp {
        op: BoolOp,
        values: Vec<Expr>,
    },

    /// Named expression (walrus operator): target := value
    NamedExpr {
        target: Box<Expr>,
        value: Box<Expr>,
    },

    /// Binary operation: left op right
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },

    /// Unary operation: op operand
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    /// Lambda expression: lambda args: body
    Lambda {
        args: crate::Arguments,
        body: Box<Expr>,
    },

    /// Conditional expression: body if test else orelse
    IfExp {
        test: Box<Expr>,
        body: Box<Expr>,
        orelse: Box<Expr>,
    },

    /// Dictionary literal: {k: v, ...}
    Dict {
        keys: Vec<Option<Expr>>,
        values: Vec<Expr>,
    },

    /// Set literal: {a, b, ...}
    Set {
        elts: Vec<Expr>,
    },

    /// List comprehension: [elt for comp...]
    ListComp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Set comprehension: {elt for comp...}
    SetComp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Dict comprehension: {key: value for comp...}
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Generator expression: (elt for comp...)
    GeneratorExp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Await expression: await value
    Await {
        value: Box<Expr>,
    },

    /// Yield expression: yield value
    Yield {
        value: Option<Box<Expr>>,
    },

    /// Yield from expression: yield from value
    YieldFrom {
        value: Box<Expr>,
    },

    /// Comparison: left ops comparators
    Compare {
        left: Box<Expr>,
        ops: Vec<CmpOp>,
        comparators: Vec<Expr>,
    },

    /// Function call: func(args, **kwargs)
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        keywords: Vec<Keyword>,
    },

    /// Formatted value in f-string
    FormattedValue {
        value: Box<Expr>,
        conversion: Option<char>,
        format_spec: Option<Box<Expr>>,
    },

    /// Joined string (f-string): f"..."
    JoinedStr {
        values: Vec<Expr>,
    },

    /// Integer literal
    IntLit {
        value: BigInt,
    },

    /// Float literal
    FloatLit {
        value: f64,
    },

    /// Complex literal
    ComplexLit {
        real: f64,
        imag: f64,
    },

    /// String literal
    StringLit {
        value: String,
        prefix: StringPrefix,
    },

    /// Bytes literal
    BytesLit {
        value: Vec<u8>,
    },

    /// Ellipsis literal: ...
    Ellipsis,

    /// None literal
    NoneLit,

    /// Boolean literal: True/False
    BoolLit {
        value: bool,
    },

    /// Name/identifier reference
    Name {
        id: Ident,
        ctx: ExprContext,
    },

    /// Attribute access: value.attr
    Attribute {
        value: Box<Expr>,
        attr: Ident,
        ctx: ExprContext,
    },

    /// Subscript: value[slice]
    Subscript {
        value: Box<Expr>,
        slice: Box<Expr>,
        ctx: ExprContext,
    },

    /// Starred expression: *value
    Starred {
        value: Box<Expr>,
        ctx: ExprContext,
    },

    /// List literal: [a, b, ...]
    List {
        elts: Vec<Expr>,
        ctx: ExprContext,
    },

    /// Tuple literal: (a, b, ...)
    Tuple {
        elts: Vec<Expr>,
        ctx: ExprContext,
    },

    /// Slice: lower:upper:step
    Slice {
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },

    // Roast-specific extensions

    /// Type annotation expression: expr: Type
    TypeAnnotation {
        value: Box<Expr>,
        annotation: Box<TypeExpr>,
    },

    /// Ownership/borrow expression: borrow x, mut x, etc.
    OwnershipExpr {
        value: Box<Expr>,
        ownership: Ownership,
    },

    /// Try expression (? operator): expr?
    /// Propagates errors early, similar to Rust's ? operator.
    /// Unwraps Result[T, E] to T, or returns early with E.
    Try {
        value: Box<Expr>,
    },
}

/// Boolean operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    And,
    Or,
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mult,
    Div,
    FloorDiv,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    MatMult,
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Invert,
    Not,
    UAdd,
    USub,
}

/// Comparison operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

/// Expression context (load, store, del).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExprContext {
    #[default]
    Load,
    Store,
    Del,
}

/// String prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StringPrefix {
    #[default]
    None,
    Raw,
    Bytes,
    RawBytes,
    Format,
    RawFormat,
    Unicode,
}

