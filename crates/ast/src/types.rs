//! Type expression AST nodes.
//!
//! These nodes represent type annotations in Roast code,
//! following PEP 484/544/585 style with Roast extensions.

use crate::Ident;
use roast_common::Span;

/// A type expression node.
#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

impl TypeExpr {
    pub fn new(kind: TypeExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Type expression kinds.
#[derive(Clone, Debug)]
pub enum TypeExprKind {
    /// Named type: int, str, MyClass
    Name {
        name: Ident,
    },

    /// Qualified name: module.Type
    Attribute {
        value: Box<TypeExpr>,
        attr: Ident,
    },

    /// Generic type: List[int], Dict[str, int]
    Subscript {
        value: Box<TypeExpr>,
        slice: Box<TypeExpr>,
    },

    /// Tuple of types: (int, str, bool) or Tuple[int, str]
    Tuple {
        elts: Vec<TypeExpr>,
    },

    /// Union type: int | str or Union[int, str]
    Union {
        types: Vec<TypeExpr>,
    },

    /// Optional type: Optional[int] or int?
    Optional {
        inner: Box<TypeExpr>,
    },

    /// Callable type: Callable[[int, str], bool]
    Callable {
        params: Vec<TypeExpr>,
        returns: Box<TypeExpr>,
    },

    /// Literal type: Literal[1, 2, 3] or Literal["a", "b"]
    Literal {
        values: Vec<LiteralValue>,
    },

    /// String annotation (forward reference): "MyClass"
    StringAnnotation {
        value: String,
    },

    /// None type
    None,

    /// Any type
    Any,

    /// Self type
    SelfType,

    /// Type variable reference: T
    TypeVar {
        name: Ident,
    },

    /// Type variable tuple: *Ts
    TypeVarTuple {
        name: Ident,
    },

    /// ParamSpec: **P
    ParamSpec {
        name: Ident,
    },

    /// Unpack: *tuple
    Unpack {
        inner: Box<TypeExpr>,
    },

    // Roast extensions

    /// Reference type: &T
    Ref {
        inner: Box<TypeExpr>,
        mutable: bool,
    },

    /// Owned type: own T
    Owned {
        inner: Box<TypeExpr>,
    },

    /// Borrowed type: borrow T
    Borrowed {
        inner: Box<TypeExpr>,
        mutable: bool,
    },

    /// Never type (for functions that don't return)
    Never,

    /// Inferred type (for type inference): _
    Infer,
}

/// Literal values that can appear in Literal types.
#[derive(Clone, Debug)]
pub enum LiteralValue {
    Int(i64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    None,
}

impl TypeExpr {
    /// Creates a simple named type.
    pub fn name(name: Ident) -> Self {
        Self {
            kind: TypeExprKind::Name { name: name.clone() },
            span: name.span,
        }
    }

    /// Creates an infer type.
    pub fn infer(span: Span) -> Self {
        Self {
            kind: TypeExprKind::Infer,
            span,
        }
    }

    /// Returns true if this is the Any type.
    pub fn is_any(&self) -> bool {
        matches!(self.kind, TypeExprKind::Any)
    }

    /// Returns true if this is an optional type.
    pub fn is_optional(&self) -> bool {
        matches!(self.kind, TypeExprKind::Optional { .. })
    }

    /// Returns true if this is the None type.
    pub fn is_none(&self) -> bool {
        matches!(self.kind, TypeExprKind::None)
    }
}

