//! Type representation for Roast.

use roast_common::Symbol;
use std::fmt;
use std::sync::Arc;

/// Unique identifier for a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// A Roast type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// Unknown type (for type inference).
    Unknown,

    /// Type variable (for generics and inference).
    Var(TypeVar),

    /// Never type (for functions that don't return).
    Never,

    /// None type.
    NoneType,

    /// Boolean type.
    Bool,

    /// Integer types.
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,

    /// Arbitrary-precision integer (BigInt).
    BigInt,

    /// Unsigned integer types.
    UInt,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,

    /// Floating point types.
    Float,
    Float32,
    Float64,

    /// Complex number types.
    Complex,
    Complex64,
    Complex128,

    /// String type.
    Str,

    /// Bytes type.
    Bytes,

    /// List type.
    List(Arc<Type>),

    /// Dict type.
    Dict(Arc<Type>, Arc<Type>),

    /// Set type.
    Set(Arc<Type>),

    /// Tuple type.
    Tuple(Vec<Type>),

    /// Slice type (for slicing operations like [1:3]).
    Slice,

    /// Optional type (T | None).
    Optional(Arc<Type>),

    /// Union type.
    Union(Vec<Type>),

    /// Function/callable type.
    Callable {
        params: Vec<FuncParam>,
        returns: Arc<Type>,
        is_async: bool,
    },

    /// Class/nominal type.
    Class(ClassType),

    /// Protocol (structural type / trait).
    Protocol(ProtocolType),

    /// Generic type application.
    Generic {
        base: Arc<Type>,
        args: Vec<Type>,
    },

    /// Type alias.
    Alias {
        name: Symbol,
        target: Arc<Type>,
    },

    /// Literal type.
    Literal(LiteralType),

    /// Reference type (Roast extension).
    Ref {
        inner: Arc<Type>,
        mutable: bool,
    },

    /// Owned type (Roast extension).
    Owned(Arc<Type>),

    /// Reference-counted type (shared ownership).
    Rc(Arc<Type>),

    /// Any type (escape hatch for dynamic typing).
    Any,

    /// Self type (for class methods).
    SelfType,

    /// Error type (for error recovery).
    Error,
}

impl Type {
    /// Returns true if this is a numeric type.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Type::Int
                | Type::Int8
                | Type::Int16
                | Type::Int32
                | Type::Int64
                | Type::Int128
                | Type::BigInt
                | Type::UInt
                | Type::UInt8
                | Type::UInt16
                | Type::UInt32
                | Type::UInt64
                | Type::UInt128
                | Type::Float
                | Type::Float32
                | Type::Float64
                | Type::Complex
                | Type::Complex64
                | Type::Complex128
        )
    }

    /// Returns true if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::Int
                | Type::Int8
                | Type::Int16
                | Type::Int32
                | Type::Int64
                | Type::Int128
                | Type::BigInt
                | Type::UInt
                | Type::UInt8
                | Type::UInt16
                | Type::UInt32
                | Type::UInt64
                | Type::UInt128
        )
    }

    /// Returns true if this is a floating point type.
    pub fn is_float(&self) -> bool {
        matches!(self, Type::Float | Type::Float32 | Type::Float64)
    }

    /// Returns true if this type is the Any type.
    pub fn is_any(&self) -> bool {
        matches!(self, Type::Any)
    }

    /// Returns true if this type is the Unknown type.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    /// Returns true if this type is an error type.
    pub fn is_error(&self) -> bool {
        matches!(self, Type::Error)
    }

    /// Returns true if this type contains any type variables.
    pub fn has_type_vars(&self) -> bool {
        match self {
            Type::Var(_) => true,
            Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => inner.has_type_vars(),
            Type::Dict(k, v) => k.has_type_vars() || v.has_type_vars(),
            Type::Tuple(elems) | Type::Union(elems) => elems.iter().any(|t| t.has_type_vars()),
            Type::Callable { params, returns, .. } => {
                params.iter().any(|p| p.ty.has_type_vars()) || returns.has_type_vars()
            }
            Type::Generic { base, args } => {
                base.has_type_vars() || args.iter().any(|t| t.has_type_vars())
            }
            Type::Ref { inner, .. } | Type::Owned(inner) => inner.has_type_vars(),
            _ => false,
        }
    }

    /// Creates a list type.
    pub fn list(elem: Type) -> Type {
        Type::List(Arc::new(elem))
    }

    /// Creates a dict type.
    pub fn dict(key: Type, value: Type) -> Type {
        Type::Dict(Arc::new(key), Arc::new(value))
    }

    /// Creates a set type.
    pub fn set(elem: Type) -> Type {
        Type::Set(Arc::new(elem))
    }

    /// Creates an optional type.
    pub fn optional(inner: Type) -> Type {
        Type::Optional(Arc::new(inner))
    }

    /// Creates a reference-counted type.
    pub fn rc(inner: Type) -> Type {
        Type::Rc(Arc::new(inner))
    }

    /// Creates a union type.
    pub fn union(types: Vec<Type>) -> Type {
        if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else {
            Type::Union(types)
        }
    }

    /// Creates a function type.
    pub fn callable(params: Vec<FuncParam>, returns: Type, is_async: bool) -> Type {
        Type::Callable {
            params,
            returns: Arc::new(returns),
            is_async,
        }
    }
}

impl Default for Type {
    fn default() -> Self {
        Type::Unknown
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Unknown => write!(f, "?"),
            Type::Var(v) => write!(f, "{}", v),
            Type::Never => write!(f, "Never"),
            Type::NoneType => write!(f, "None"),
            Type::Bool => write!(f, "bool"),
            Type::Int => write!(f, "int"),
            Type::Int8 => write!(f, "i8"),
            Type::Int16 => write!(f, "i16"),
            Type::Int32 => write!(f, "i32"),
            Type::Int64 => write!(f, "i64"),
            Type::Int128 => write!(f, "i128"),
            Type::BigInt => write!(f, "bint"),
            Type::UInt => write!(f, "uint"),
            Type::UInt8 => write!(f, "u8"),
            Type::UInt16 => write!(f, "u16"),
            Type::UInt32 => write!(f, "u32"),
            Type::UInt64 => write!(f, "u64"),
            Type::UInt128 => write!(f, "u128"),
            Type::Float => write!(f, "float"),
            Type::Float32 => write!(f, "f32"),
            Type::Float64 => write!(f, "f64"),
            Type::Complex => write!(f, "complex"),
            Type::Complex64 => write!(f, "c64"),
            Type::Complex128 => write!(f, "c128"),
            Type::Str => write!(f, "str"),
            Type::Bytes => write!(f, "bytes"),
            Type::List(elem) => write!(f, "List[{}]", elem),
            Type::Dict(k, v) => write!(f, "Dict[{}, {}]", k, v),
            Type::Set(elem) => write!(f, "Set[{}]", elem),
            Type::Tuple(elems) => {
                write!(f, "Tuple[")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            Type::Slice => write!(f, "slice"),
            Type::Optional(inner) => write!(f, "{}?", inner),
            Type::Rc(inner) => write!(f, "rc[{}]", inner),
            Type::Union(types) => {
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", ty)?;
                }
                Ok(())
            }
            Type::Callable {
                params,
                returns,
                is_async,
            } => {
                if *is_async {
                    write!(f, "async ")?;
                }
                write!(f, "(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param.ty)?;
                }
                write!(f, ") -> {}", returns)
            }
            Type::Class(cls) => write!(f, "{}", cls.name),
            Type::Protocol(proto) => write!(f, "{}", proto.name),
            Type::Generic { base, args } => {
                write!(f, "{}[", base)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, "]")
            }
            Type::Alias { name, .. } => write!(f, "<sym:{}>", name.as_raw()),
            Type::Literal(lit) => write!(f, "{}", lit),
            Type::Ref { inner, mutable } => {
                if *mutable {
                    write!(f, "&mut {}", inner)
                } else {
                    write!(f, "&{}", inner)
                }
            }
            Type::Owned(inner) => write!(f, "own {}", inner),
            Type::Any => write!(f, "Any"),
            Type::SelfType => write!(f, "Self"),
            Type::Error => write!(f, "<error>"),
        }
    }
}

/// A type variable for generics and type inference.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeVar {
    pub id: u32,
    pub name: Option<Symbol>,
    pub bound: Option<Arc<Type>>,
}

impl fmt::Display for TypeVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "T{}", name.as_raw())
        } else {
            write!(f, "T{}", self.id)
        }
    }
}

/// Function parameter type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncParam {
    pub name: Option<Symbol>,
    pub ty: Type,
    pub default: bool,
    pub kind: ParamKind,
}

/// Parameter kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamKind {
    /// Regular positional or keyword.
    Regular,
    /// Positional only (before /).
    PosOnly,
    /// Keyword only (after *).
    KwOnly,
    /// *args.
    VarPositional,
    /// **kwargs.
    VarKeyword,
}

/// Class type information.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClassType {
    pub name: String,
    pub module: Option<String>,
    pub type_params: Vec<TypeVar>,
    pub bases: Vec<Type>,
    pub members: Vec<(String, Type)>,
    pub methods: Vec<(String, Type)>,
}

/// Protocol (structural type / trait) type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProtocolType {
    pub name: String,
    pub members: Vec<(String, Type)>,
    pub methods: Vec<(String, Type)>,
}

/// Literal type values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralType {
    Int(i64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    None,
}

impl fmt::Display for LiteralType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralType::Int(n) => write!(f, "Literal[{}]", n),
            LiteralType::Bool(b) => write!(f, "Literal[{}]", b),
            LiteralType::Str(s) => write!(f, "Literal[\"{}\"]", s),
            LiteralType::Bytes(b) => write!(f, "Literal[b\"{}\"]", String::from_utf8_lossy(b)),
            LiteralType::None => write!(f, "Literal[None]"),
        }
    }
}

/// Ownership mode for Roast's ownership system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum OwnershipMode {
    #[default]
    Owned,
    Borrowed,
    MutBorrowed,
    Moved,
}
