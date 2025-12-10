//! Type inference for Roast.
//!
//! This module implements Hindley-Milner style type inference with extensions
//! for Python's gradual typing.

use crate::context::TypeContext;
use crate::types::*;

/// Unifies two types, updating the type context with any inferred bindings.
pub fn unify(ctx: &mut TypeContext, a: &Type, b: &Type) -> Result<(), UnifyError> {
    let a = ctx.resolve(a);
    let b = ctx.resolve(b);

    match (&a, &b) {
        // Same types unify
        (x, y) if x == y => Ok(()),

        // Type variables unify with anything
        (Type::Var(var), other) | (other, Type::Var(var)) => {
            // Occurs check
            if occurs_in(var.id, other) {
                return Err(UnifyError::OccursCheck {
                    var: var.clone(),
                    ty: other.clone(),
                });
            }
            ctx.bind(var.id, other.clone());
            Ok(())
        }

        // Any unifies with anything
        (Type::Any, _) | (_, Type::Any) => Ok(()),

        // Error types unify with anything (for error recovery)
        (Type::Error, _) | (_, Type::Error) => Ok(()),

        // Never is a subtype of everything
        (Type::Never, _) => Ok(()),

        // None is a subtype of Optional[T]
        (Type::NoneType, Type::Optional(_)) => Ok(()),

        // T is a subtype of Optional[T]
        (t, Type::Optional(inner)) => unify(ctx, t, inner),

        // List unification
        (Type::List(a), Type::List(b)) => unify(ctx, a, b),

        // Set unification
        (Type::Set(a), Type::Set(b)) => unify(ctx, a, b),

        // Dict unification
        (Type::Dict(k1, v1), Type::Dict(k2, v2)) => {
            unify(ctx, k1, k2)?;
            unify(ctx, v1, v2)
        }

        // Tuple unification (same length)
        (Type::Tuple(a), Type::Tuple(b)) if a.len() == b.len() => {
            for (a_elem, b_elem) in a.iter().zip(b.iter()) {
                unify(ctx, a_elem, b_elem)?;
            }
            Ok(())
        }

        // Optional unification
        (Type::Optional(a), Type::Optional(b)) => unify(ctx, a, b),

        // Union unification (simplified)
        (Type::Union(types), other) | (other, Type::Union(types)) => {
            // Check if other matches any type in the union
            for t in types {
                if unify(ctx, t, other).is_ok() {
                    return Ok(());
                }
            }
            Err(UnifyError::Mismatch {
                expected: Type::Union(types.clone()),
                found: other.clone(),
            })
        }

        // Function unification
        (
            Type::Callable {
                params: p1,
                returns: r1,
                is_async: a1,
            },
            Type::Callable {
                params: p2,
                returns: r2,
                is_async: a2,
            },
        ) => {
            if a1 != a2 {
                return Err(UnifyError::Mismatch {
                    expected: a.clone(),
                    found: b.clone(),
                });
            }
            if p1.len() != p2.len() {
                return Err(UnifyError::Mismatch {
                    expected: a.clone(),
                    found: b.clone(),
                });
            }
            for (p1, p2) in p1.iter().zip(p2.iter()) {
                // Contravariant in parameters
                unify(ctx, &p2.ty, &p1.ty)?;
            }
            // Covariant in return type
            unify(ctx, r1, r2)
        }

        // Generic type unification
        (
            Type::Generic {
                base: b1,
                args: a1,
            },
            Type::Generic {
                base: b2,
                args: a2,
            },
        ) => {
            unify(ctx, b1, b2)?;
            if a1.len() != a2.len() {
                return Err(UnifyError::Mismatch {
                    expected: a.clone(),
                    found: b.clone(),
                });
            }
            for (a1, a2) in a1.iter().zip(a2.iter()) {
                unify(ctx, a1, a2)?;
            }
            Ok(())
        }

        // Reference types
        (
            Type::Ref {
                inner: i1,
                mutable: m1,
            },
            Type::Ref {
                inner: i2,
                mutable: m2,
            },
        ) => {
            if m1 != m2 {
                return Err(UnifyError::Mismatch {
                    expected: a.clone(),
                    found: b.clone(),
                });
            }
            unify(ctx, i1, i2)
        }

        // Owned types
        (Type::Owned(a), Type::Owned(b)) => unify(ctx, a, b),

        // No unification possible
        _ => Err(UnifyError::Mismatch {
            expected: a.clone(),
            found: b.clone(),
        }),
    }
}

/// Checks if a type variable occurs in a type (for occurs check).
fn occurs_in(var_id: u32, ty: &Type) -> bool {
    match ty {
        Type::Var(var) => var.id == var_id,
        Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => occurs_in(var_id, inner),
        Type::Dict(k, v) => occurs_in(var_id, k) || occurs_in(var_id, v),
        Type::Tuple(elems) | Type::Union(elems) => elems.iter().any(|t| occurs_in(var_id, t)),
        Type::Callable { params, returns, .. } => {
            params.iter().any(|p| occurs_in(var_id, &p.ty)) || occurs_in(var_id, returns)
        }
        Type::Generic { base, args } => {
            occurs_in(var_id, base) || args.iter().any(|t| occurs_in(var_id, t))
        }
        Type::Ref { inner, .. } | Type::Owned(inner) => occurs_in(var_id, inner),
        _ => false,
    }
}

/// Unification error.
#[derive(Debug, Clone)]
pub enum UnifyError {
    /// Type mismatch.
    Mismatch { expected: Type, found: Type },
    /// Occurs check failed (infinite type).
    OccursCheck { var: TypeVar, ty: Type },
}

impl std::fmt::Display for UnifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnifyError::Mismatch { expected, found } => {
                write!(f, "type mismatch: expected {}, found {}", expected, found)
            }
            UnifyError::OccursCheck { var, ty } => {
                write!(f, "infinite type: {} occurs in {}", var, ty)
            }
        }
    }
}

impl std::error::Error for UnifyError {}

/// Instantiates a polymorphic type with fresh type variables.
pub fn instantiate(ctx: &mut TypeContext, ty: &Type) -> Type {
    match ty {
        Type::Var(var) => {
            if var.bound.is_some() {
                ctx.fresh_var()
            } else {
                ty.clone()
            }
        }
        Type::List(inner) => Type::list(instantiate(ctx, inner)),
        Type::Dict(k, v) => Type::dict(instantiate(ctx, k), instantiate(ctx, v)),
        Type::Set(inner) => Type::set(instantiate(ctx, inner)),
        Type::Tuple(elems) => Type::Tuple(elems.iter().map(|t| instantiate(ctx, t)).collect()),
        Type::Optional(inner) => Type::optional(instantiate(ctx, inner)),
        Type::Union(types) => Type::union(types.iter().map(|t| instantiate(ctx, t)).collect()),
        Type::Callable {
            params,
            returns,
            is_async,
        } => Type::Callable {
            params: params
                .iter()
                .map(|p| FuncParam {
                    ty: instantiate(ctx, &p.ty),
                    ..p.clone()
                })
                .collect(),
            returns: std::sync::Arc::new(instantiate(ctx, returns)),
            is_async: *is_async,
        },
        Type::Generic { base, args } => Type::Generic {
            base: std::sync::Arc::new(instantiate(ctx, base)),
            args: args.iter().map(|t| instantiate(ctx, t)).collect(),
        },
        Type::Ref { inner, mutable } => Type::Ref {
            inner: std::sync::Arc::new(instantiate(ctx, inner)),
            mutable: *mutable,
        },
        Type::Owned(inner) => Type::Owned(std::sync::Arc::new(instantiate(ctx, inner))),
        _ => ty.clone(),
    }
}

/// Generalizes a type over free type variables.
pub fn generalize(ctx: &TypeContext, ty: &Type) -> Type {
    // For now, just resolve the type
    ctx.resolve_deep(ty)
}

