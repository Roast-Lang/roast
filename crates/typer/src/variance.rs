//! Variance annotations for the Roast type system.
//!
//! Variance describes how subtyping of container types relates to subtyping
//! of their element types.
//!
//! - **Covariant**: If T <: U, then Container<T> <: Container<U>
//! - **Contravariant**: If T <: U, then Container<U> <: Container<T>
//! - **Invariant**: No subtyping relationship regardless of T and U

use std::fmt;

/// Variance annotation for type parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Variance {
    /// Covariant (+T): T can be replaced with supertypes.
    /// Example: List[Cat] <: List[Animal] (if Cat <: Animal)
    Covariant,

    /// Contravariant (-T): T can be replaced with subtypes.
    /// Example: Callable[[Animal], None] <: Callable[[Cat], None]
    Contravariant,

    /// Invariant (T): No variance, must be exactly the same type.
    /// Example: Mutable containers are typically invariant.
    Invariant,

    /// Bivariant: Both covariant and contravariant (rarely used).
    Bivariant,
}

impl Variance {
    /// Combine two variances (for nested type parameters).
    pub fn combine(self, other: Variance) -> Variance {
        match (self, other) {
            // Invariant absorbs everything
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,

            // Bivariant is neutral
            (Variance::Bivariant, v) | (v, Variance::Bivariant) => v,

            // Same variance composes
            (Variance::Covariant, Variance::Covariant) => Variance::Covariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Covariant,

            // Opposite variances combine to contravariance
            (Variance::Covariant, Variance::Contravariant)
            | (Variance::Contravariant, Variance::Covariant) => Variance::Contravariant,
        }
    }

    /// Check if this variance allows subtyping.
    pub fn allows_subtype(self) -> bool {
        matches!(self, Variance::Covariant | Variance::Bivariant)
    }

    /// Check if this variance allows supertyping.
    pub fn allows_supertype(self) -> bool {
        matches!(self, Variance::Contravariant | Variance::Bivariant)
    }
}

impl fmt::Display for Variance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Variance::Covariant => write!(f, "+"),
            Variance::Contravariant => write!(f, "-"),
            Variance::Invariant => write!(f, "="),
            Variance::Bivariant => write!(f, "±"),
        }
    }
}

impl Default for Variance {
    fn default() -> Self {
        Variance::Invariant
    }
}

// =============================================================================
// Type Parameter with Variance
// =============================================================================

use roast_common::Symbol;
use super::types::Type;

/// A type parameter with variance annotation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeParam {
    /// Parameter name.
    pub name: Symbol,
    /// Variance annotation.
    pub variance: Variance,
    /// Upper bound (if any).
    pub bound: Option<Box<Type>>,
    /// Default type (if any).
    pub default: Option<Box<Type>>,
}

impl TypeParam {
    /// Create a new invariant type parameter.
    pub fn new(name: Symbol) -> Self {
        Self {
            name,
            variance: Variance::Invariant,
            bound: None,
            default: None,
        }
    }

    /// Create a covariant type parameter.
    pub fn covariant(name: Symbol) -> Self {
        Self {
            name,
            variance: Variance::Covariant,
            bound: None,
            default: None,
        }
    }

    /// Create a contravariant type parameter.
    pub fn contravariant(name: Symbol) -> Self {
        Self {
            name,
            variance: Variance::Contravariant,
            bound: None,
            default: None,
        }
    }

    /// Add an upper bound.
    pub fn with_bound(mut self, bound: Type) -> Self {
        self.bound = Some(Box::new(bound));
        self
    }

    /// Add a default type.
    pub fn with_default(mut self, default: Type) -> Self {
        self.default = Some(Box::new(default));
        self
    }
}

// =============================================================================
// Const Generics
// =============================================================================

/// A constant value that can be used as a generic parameter.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstGeneric {
    /// Integer constant.
    Int(i64),
    /// Boolean constant.
    Bool(bool),
    /// String constant.
    Str(String),
    /// A const parameter (unevaluated).
    Param(Symbol),
    /// Expression (for computed constants).
    Expr(ConstExpr),
}

/// A constant expression for const generics.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstExpr {
    /// Addition.
    Add(Box<ConstGeneric>, Box<ConstGeneric>),
    /// Subtraction.
    Sub(Box<ConstGeneric>, Box<ConstGeneric>),
    /// Multiplication.
    Mul(Box<ConstGeneric>, Box<ConstGeneric>),
    /// Division.
    Div(Box<ConstGeneric>, Box<ConstGeneric>),
}

impl ConstGeneric {
    /// Try to evaluate to an integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConstGeneric::Int(n) => Some(*n),
            ConstGeneric::Expr(expr) => expr.eval_int(),
            _ => None,
        }
    }
}

impl ConstExpr {
    /// Evaluate to an integer if possible.
    pub fn eval_int(&self) -> Option<i64> {
        match self {
            ConstExpr::Add(l, r) => Some(l.as_int()? + r.as_int()?),
            ConstExpr::Sub(l, r) => Some(l.as_int()? - r.as_int()?),
            ConstExpr::Mul(l, r) => Some(l.as_int()? * r.as_int()?),
            ConstExpr::Div(l, r) => {
                let right = r.as_int()?;
                if right == 0 { None } else { Some(l.as_int()? / right) }
            }
        }
    }
}

// =============================================================================
// Where Clauses
// =============================================================================

/// A where clause constraint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WhereClause {
    /// Type must implement a protocol/trait.
    /// `where T: Protocol`
    Implements {
        ty: Type,
        protocol: Symbol,
    },

    /// Type must be a subtype of another.
    /// `where T <: U`
    SubtypeOf {
        sub: Type,
        super_: Type,
    },

    /// Type must equal another.
    /// `where T == U`
    Equals {
        left: Type,
        right: Type,
    },

    /// Const constraint.
    /// `where N > 0`
    ConstConstraint {
        param: Symbol,
        predicate: ConstPredicate,
    },
}

/// A predicate for const constraints.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstPredicate {
    /// Greater than.
    GreaterThan(ConstGeneric),
    /// Less than.
    LessThan(ConstGeneric),
    /// Equal to.
    Equals(ConstGeneric),
    /// Not equal to.
    NotEquals(ConstGeneric),
    /// Greater than or equal.
    GreaterOrEqual(ConstGeneric),
    /// Less than or equal.
    LessOrEqual(ConstGeneric),
}

impl WhereClause {
    /// Create a protocol implementation constraint.
    pub fn implements(ty: Type, protocol: Symbol) -> Self {
        WhereClause::Implements { ty, protocol }
    }

    /// Create a subtype constraint.
    pub fn subtype_of(sub: Type, super_: Type) -> Self {
        WhereClause::SubtypeOf { sub, super_ }
    }

    /// Create an equality constraint.
    pub fn equals(left: Type, right: Type) -> Self {
        WhereClause::Equals { left, right }
    }
}

// =============================================================================
// Generic Bounds
// =============================================================================

/// Bounds on a generic type parameter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TypeBounds {
    /// Required protocols/traits.
    pub protocols: Vec<Symbol>,
    /// Upper bound type.
    pub upper: Option<Box<Type>>,
    /// Lower bound type.
    pub lower: Option<Box<Type>>,
}

impl TypeBounds {
    /// Create empty bounds.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a protocol requirement.
    pub fn require(mut self, protocol: Symbol) -> Self {
        self.protocols.push(protocol);
        self
    }

    /// Set upper bound.
    pub fn bounded_by(mut self, upper: Type) -> Self {
        self.upper = Some(Box::new(upper));
        self
    }

    /// Check if bounds are empty.
    pub fn is_empty(&self) -> bool {
        self.protocols.is_empty() && self.upper.is_none() && self.lower.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variance_combine() {
        assert_eq!(Variance::Covariant.combine(Variance::Covariant), Variance::Covariant);
        assert_eq!(Variance::Covariant.combine(Variance::Contravariant), Variance::Contravariant);
        assert_eq!(Variance::Invariant.combine(Variance::Covariant), Variance::Invariant);
    }

    #[test]
    fn test_const_expr_eval() {
        let expr = ConstExpr::Add(
            Box::new(ConstGeneric::Int(5)),
            Box::new(ConstGeneric::Int(3)),
        );
        assert_eq!(expr.eval_int(), Some(8));
    }

    #[test]
    fn test_type_bounds_empty() {
        let bounds = TypeBounds::new();
        assert!(bounds.is_empty());
        assert!(bounds.protocols.is_empty());
    }
}

