//! HIR node definitions.

use id_arena::Id;
use roast_common::{Span, Symbol};
use roast_typer::Type;

/// A HIR module.
pub struct HirModule {
    pub name: String,
    pub items: Vec<HirItem>,
}

/// Top-level items in HIR.
pub enum HirItem {
    Function(HirFunction),
    Class(HirClass),
    Import(HirImport),
    TypeAlias(HirTypeAlias),
}

/// A function in HIR.
pub struct HirFunction {
    pub name: Symbol,
    pub params: Vec<HirParam>,
    pub return_type: Type,
    pub body: HirBlock,
    pub is_async: bool,
    pub decorators: Vec<HirDecorator>,
    pub span: Span,
}

/// A function parameter.
#[derive(Clone, Debug)]
pub struct HirParam {
    pub name: Symbol,
    pub ty: Type,
    pub default: Option<HirExprId>,
    pub kind: HirParamKind,
    pub span: Span,
}

/// Parameter kind for *args/**kwargs support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HirParamKind {
    #[default]
    Regular,
    /// *args - collects positional arguments into tuple
    VarPositional,
    /// **kwargs - collects keyword arguments into dict
    VarKeyword,
}

/// A decorator applied to a function or class.
#[derive(Clone, Debug)]
pub struct HirDecorator {
    pub name: Symbol,
    pub args: Vec<HirExprId>,
    pub span: Span,
}

/// A class definition.
pub struct HirClass {
    pub name: Symbol,
    pub type_params: Vec<HirTypeParam>,
    pub bases: Vec<Type>,
    pub members: Vec<HirClassMember>,
    pub decorators: Vec<HirDecorator>,
    /// Method Resolution Order - computed via C3 linearization for multiple inheritance.
    /// The MRO starts with this class and includes all ancestors in resolution order.
    pub mro: Vec<String>,
    /// True if this class has abstract methods (directly or inherited not overridden).
    /// Abstract classes cannot be instantiated directly.
    pub is_abstract: bool,
    /// Names of abstract methods that must be implemented by concrete subclasses.
    pub abstract_methods: Vec<String>,
    pub span: Span,
}

/// Type parameter.
pub struct HirTypeParam {
    pub name: Symbol,
    pub bound: Option<Type>,
}

/// Method kind for OOP support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MethodKind {
    #[default]
    Instance,
    /// @staticmethod - no implicit first argument
    Static,
    /// @classmethod - first argument is cls (the class)
    Class,
    /// @abstractmethod - must be overridden in subclass
    Abstract,
}

/// Class member.
pub enum HirClassMember {
    Field { name: Symbol, ty: Type, span: Span },
    Method {
        func: HirFunction,
        kind: MethodKind,
    },
    /// Property with getter (and optionally setter)
    Property {
        name: Symbol,
        getter: HirFunction,
        setter: Option<Box<HirFunction>>,
        span: Span,
    },
    /// Class variable (shared across all instances)
    ClassVariable {
        name: Symbol,
        ty: Type,
        value: Option<HirExprId>,
        span: Span,
    },
}

/// Import statement.
pub struct HirImport {
    pub module: String,
    pub names: Vec<(Symbol, Option<Symbol>)>,
    pub span: Span,
}

/// Type alias.
pub struct HirTypeAlias {
    pub name: Symbol,
    pub ty: Type,
    pub span: Span,
}

/// A block of statements.
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub span: Span,
}

/// HIR statement.
pub struct HirStmt {
    pub kind: HirStmtKind,
    pub span: Span,
}

/// HIR statement kinds.
pub enum HirStmtKind {
    Let {
        name: Symbol,
        ty: Type,
        init: Option<HirExprId>,
        mutable: bool,
    },
    Assign {
        target: HirExprId,
        value: HirExprId,
    },
    Expr(HirExprId),
    Return(Option<HirExprId>),
    If {
        cond: HirExprId,
        then_block: HirBlock,
        else_block: Option<HirBlock>,
    },
    Loop {
        body: HirBlock,
    },
    While {
        cond: HirExprId,
        body: HirBlock,
    },
    For {
        var: Symbol,
        var_ty: Type,
        iter: HirExprId,
        body: HirBlock,
    },
    Break,
    Continue,
    Match {
        subject: HirExprId,
        arms: Vec<HirMatchArm>,
    },
    Assert {
        test: HirExprId,
        msg: Option<HirExprId>,
    },
    /// Try/except statement
    Try {
        body: HirBlock,
        handlers: Vec<HirExceptHandler>,
        orelse: Option<HirBlock>,
        finalbody: Option<HirBlock>,
    },
    /// Raise exception
    Raise {
        exc: Option<HirExprId>,
        cause: Option<HirExprId>,
    },
}

/// Exception handler for try/except
pub struct HirExceptHandler {
    pub exc_type: Option<HirExprId>,
    pub name: Option<Symbol>,
    pub body: HirBlock,
    pub span: Span,
}

/// Match arm.
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<HirExprId>,
    pub body: HirBlock,
}

/// HIR pattern.
pub struct HirPattern {
    pub kind: HirPatternKind,
    pub ty: Type,
    pub span: Span,
}

/// Pattern kinds.
pub enum HirPatternKind {
    Wildcard,
    Binding(Symbol),
    Literal(HirLiteral),
    Tuple(Vec<HirPattern>),
    Struct {
        ty: Type,
        fields: Vec<(Symbol, HirPattern)>,
    },
    Or(Vec<HirPattern>),
}

/// Expression ID (index into expression arena).
pub type HirExprId = Id<HirExpr>;

/// HIR expression.
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: Type,
    pub span: Span,
}

/// Expression kinds.
pub enum HirExprKind {
    Literal(HirLiteral),
    Var(Symbol),
    Binary {
        op: HirBinOp,
        left: HirExprId,
        right: HirExprId,
    },
    Unary {
        op: HirUnaryOp,
        operand: HirExprId,
    },
    Call {
        callee: HirExprId,
        args: Vec<HirExprId>,
    },
    MethodCall {
        receiver: HirExprId,
        method: Symbol,
        args: Vec<HirExprId>,
    },
    Field {
        base: HirExprId,
        field: Symbol,
    },
    Index {
        base: HirExprId,
        index: HirExprId,
    },
    /// Slice expression for sequences (e.g., list[1:3]).
    Slice {
        lower: Option<HirExprId>,
        upper: Option<HirExprId>,
        step: Option<HirExprId>,
    },
    Tuple(Vec<HirExprId>),
    List(Vec<HirExprId>),
    Set(Vec<HirExprId>),
    Dict(Vec<(HirExprId, HirExprId)>),
    If {
        cond: HirExprId,
        then_expr: HirExprId,
        else_expr: HirExprId,
    },
    Lambda {
        params: Vec<HirParam>,
        body: HirExprId,
    },
    Block(HirBlock),
    Ref {
        expr: HirExprId,
        mutable: bool,
    },
    Deref(HirExprId),
    Cast {
        expr: HirExprId,
        target_ty: Type,
    },
    /// Try expression (? operator): unwrap Result/Option or return early.
    Try {
        expr: HirExprId,
    },
    /// List comprehension [element for pattern in iter if condition]
    ListComp {
        element: HirExprId,
        pattern: HirPattern,
        iter: HirExprId,
        condition: Option<HirExprId>,
    },
    /// Await expression: await an async value
    Await {
        value: HirExprId,
    },
}

/// Literal values.
#[derive(Clone, Debug)]
pub enum HirLiteral {
    Int(i128),
    Float(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    None,
}

/// Binary operators.
#[derive(Clone, Copy, Debug)]
pub enum HirBinOp {
    Add, Sub, Mul, Div, FloorDiv, Mod, Pow,
    BitAnd, BitOr, BitXor, Shl, Shr,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    In, NotIn,
}

/// Unary operators.
#[derive(Clone, Copy, Debug)]
pub enum HirUnaryOp {
    Neg, Not, BitNot,
}
