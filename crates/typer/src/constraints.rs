//! Constraint-based type inference.

use crate::types::*;
use roast_common::Span;
use std::collections::VecDeque;

/// A type constraint.
#[derive(Debug, Clone)]
pub struct Constraint {
    pub kind: ConstraintKind,
    pub span: Span,
}

/// Kind of type constraint.
#[derive(Debug, Clone)]
pub enum ConstraintKind {
    /// Two types must be equal.
    Equal(Type, Type),
    /// First type must be a subtype of second.
    Subtype(Type, Type),
    /// Type must be numeric.
    Numeric(Type),
    /// Type must be iterable with given element type.
    Iterable(Type, Type),
    /// Type must be callable with given signature.
    Callable {
        callee: Type,
        args: Vec<Type>,
        result: Type,
    },
    /// Type must have given attribute with given type.
    HasAttr {
        base: Type,
        attr: String,
        attr_type: Type,
    },
    /// Type must be subscriptable.
    Subscriptable {
        base: Type,
        index: Type,
        result: Type,
    },
}

/// Constraint solver.
pub struct ConstraintSolver {
    constraints: VecDeque<Constraint>,
    errors: Vec<ConstraintError>,
}

/// Constraint solving error.
#[derive(Debug, Clone)]
pub struct ConstraintError {
    pub message: String,
    pub span: Span,
}

impl ConstraintSolver {
    /// Creates a new constraint solver.
    pub fn new() -> Self {
        Self {
            constraints: VecDeque::new(),
            errors: Vec::new(),
        }
    }

    /// Adds a constraint.
    pub fn add(&mut self, constraint: Constraint) {
        self.constraints.push_back(constraint);
    }

    /// Adds an equality constraint.
    pub fn equal(&mut self, a: Type, b: Type, span: Span) {
        self.add(Constraint {
            kind: ConstraintKind::Equal(a, b),
            span,
        });
    }

    /// Adds a subtype constraint.
    pub fn subtype(&mut self, sub: Type, sup: Type, span: Span) {
        self.add(Constraint {
            kind: ConstraintKind::Subtype(sub, sup),
            span,
        });
    }

    /// Solves all constraints.
    pub fn solve(&mut self, ctx: &mut crate::context::TypeContext) -> Vec<ConstraintError> {
        while let Some(constraint) = self.constraints.pop_front() {
            if let Err(err) = self.solve_one(ctx, &constraint) {
                self.errors.push(err);
            }
        }
        std::mem::take(&mut self.errors)
    }

    fn solve_one(
        &mut self,
        ctx: &mut crate::context::TypeContext,
        constraint: &Constraint,
    ) -> Result<(), ConstraintError> {
        match &constraint.kind {
            ConstraintKind::Equal(a, b) => {
                if let Err(err) = crate::infer::unify(ctx, a, b) {
                    return Err(ConstraintError {
                        message: err.to_string(),
                        span: constraint.span,
                    });
                }
            }
            ConstraintKind::Subtype(sub, sup) => {
                if !ctx.is_subtype(sub, sup) {
                    return Err(ConstraintError {
                        message: format!("{} is not a subtype of {}", sub, sup),
                        span: constraint.span,
                    });
                }
            }
            ConstraintKind::Numeric(ty) => {
                let resolved = ctx.resolve(ty);
                if !resolved.is_numeric() && !matches!(resolved, Type::Var(_) | Type::Any | Type::Error) {
                    return Err(ConstraintError {
                        message: format!("{} is not numeric", ty),
                        span: constraint.span,
                    });
                }
            }
            ConstraintKind::Iterable(iter_ty, elem_ty) => {
                let resolved = ctx.resolve(iter_ty);
                let expected_elem = match &resolved {
                    Type::List(e) | Type::Set(e) => Some((**e).clone()),
                    Type::Dict(k, _) => Some((**k).clone()),
                    Type::Tuple(elems) if !elems.is_empty() => Some(Type::union(elems.clone())),
                    Type::Str => Some(Type::Str),
                    Type::Bytes => Some(Type::Int),
                    Type::Any => Some(Type::Any),
                    Type::Var(_) => None, // Defer
                    _ => {
                        return Err(ConstraintError {
                            message: format!("{} is not iterable", iter_ty),
                            span: constraint.span,
                        });
                    }
                };
                if let Some(expected) = expected_elem {
                    if let Err(err) = crate::infer::unify(ctx, elem_ty, &expected) {
                        return Err(ConstraintError {
                            message: err.to_string(),
                            span: constraint.span,
                        });
                    }
                }
            }
            ConstraintKind::Callable {
                callee,
                args,
                result,
            } => {
                let resolved = ctx.resolve(callee);
                match &resolved {
                    Type::Callable { params, returns, .. } => {
                        if args.len() > params.len() {
                            return Err(ConstraintError {
                                message: format!(
                                    "too many arguments: expected {}, got {}",
                                    params.len(),
                                    args.len()
                                ),
                                span: constraint.span,
                            });
                        }
                        for (arg, param) in args.iter().zip(params.iter()) {
                            if let Err(err) = crate::infer::unify(ctx, arg, &param.ty) {
                                return Err(ConstraintError {
                                    message: err.to_string(),
                                    span: constraint.span,
                                });
                            }
                        }
                        if let Err(err) = crate::infer::unify(ctx, result, returns) {
                            return Err(ConstraintError {
                                message: err.to_string(),
                                span: constraint.span,
                            });
                        }
                    }
                    Type::Any => {}
                    Type::Var(_) => {
                        // Defer - re-add constraint
                        self.constraints.push_back(constraint.clone());
                    }
                    _ => {
                        return Err(ConstraintError {
                            message: format!("{} is not callable", callee),
                            span: constraint.span,
                        });
                    }
                }
            }
            ConstraintKind::HasAttr { base, attr, attr_type } => {
                let resolved = ctx.resolve(base);
                match &resolved {
                    Type::Class(cls) => {
                        let found = cls.members.iter().chain(cls.methods.iter())
                            .find(|(name, _)| name == attr);
                        if let Some((_, ty)) = found {
                            if let Err(err) = crate::infer::unify(ctx, attr_type, ty) {
                                return Err(ConstraintError {
                                    message: err.to_string(),
                                    span: constraint.span,
                                });
                            }
                        } else {
                            return Err(ConstraintError {
                                message: format!("{} has no attribute '{}'", base, attr),
                                span: constraint.span,
                            });
                        }
                    }
                    Type::Any => {}
                    Type::Var(_) => {
                        self.constraints.push_back(constraint.clone());
                    }
                    _ => {
                        return Err(ConstraintError {
                            message: format!("{} has no attribute '{}'", base, attr),
                            span: constraint.span,
                        });
                    }
                }
            }
            ConstraintKind::Subscriptable { base, index: _, result } => {
                let resolved = ctx.resolve(base);
                let elem_type = match &resolved {
                    Type::List(e) | Type::Set(e) => (**e).clone(),
                    Type::Dict(_, v) => (**v).clone(),
                    Type::Tuple(elems) if !elems.is_empty() => Type::union(elems.clone()),
                    Type::Str => Type::Str,
                    Type::Bytes => Type::Int,
                    Type::Any => Type::Any,
                    Type::Var(_) => {
                        self.constraints.push_back(constraint.clone());
                        return Ok(());
                    }
                    // Allow classes with __getitem__ to be subscriptable
                    Type::Class(cls) => {
                        // Check if class has __getitem__ method
                        if cls.methods.iter().any(|(name, _)| name == "__getitem__") {
                            // Return type is Any since we don't know the return type of __getitem__
                            Type::Any
                        } else {
                            return Err(ConstraintError {
                                message: format!("{} is not subscriptable", base),
                                span: constraint.span,
                            });
                        }
                    }
                    _ => {
                        return Err(ConstraintError {
                            message: format!("{} is not subscriptable", base),
                            span: constraint.span,
                        });
                    }
                };
                if let Err(err) = crate::infer::unify(ctx, result, &elem_type) {
                    return Err(ConstraintError {
                        message: err.to_string(),
                        span: constraint.span,
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for ConstraintSolver {
    fn default() -> Self {
        Self::new()
    }
}
