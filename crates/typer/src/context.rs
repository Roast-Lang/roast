//! Type context for managing types during compilation.

use crate::types::*;
use roast_common::{Interner, Span, Symbol};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// The type context holds all type information during compilation.
pub struct TypeContext {
    /// Next type variable ID.
    next_var_id: u32,

    /// Type variable substitutions.
    substitutions: FxHashMap<u32, Type>,

    /// Named types (classes, type aliases, etc.).
    named_types: FxHashMap<Symbol, Type>,

    /// Scope stack for local bindings.
    scopes: Vec<FxHashMap<Symbol, Type>>,

    /// Persistent map of expression types, keyed by span.
    /// This survives scope popping and is used by HIR builder.
    expr_types: FxHashMap<Span, Type>,
    
    /// Module exports for star imports: module_name -> list of (exported_name, type)
    /// Populated before type checking, used to expand `from x import *`
    module_exports: FxHashMap<String, Vec<(Symbol, Type)>>,
}

impl TypeContext {
    /// Creates a new type context with built-in types.
    pub fn new() -> Self {
        Self {
            next_var_id: 0,
            substitutions: FxHashMap::default(),
            named_types: FxHashMap::default(),
            scopes: vec![FxHashMap::default()],
            expr_types: FxHashMap::default(),
            module_exports: FxHashMap::default(),
        }
    }

    /// Creates a new type context with builtins registered.
    pub fn with_builtins(interner: &Interner) -> Self {
        let mut ctx = Self::new();
        crate::builtins::register_builtins(&mut ctx, interner);
        ctx
    }

    /// Creates a fresh type variable.
    pub fn fresh_var(&mut self) -> Type {
        let id = self.next_var_id;
        self.next_var_id += 1;
        Type::Var(TypeVar {
            id,
            name: None,
            bound: None,
        })
    }

    /// Creates a fresh type variable with a name.
    pub fn fresh_named_var(&mut self, name: Symbol) -> Type {
        let id = self.next_var_id;
        self.next_var_id += 1;
        Type::Var(TypeVar {
            id,
            name: Some(name),
            bound: None,
        })
    }

    /// Creates a fresh type variable with a bound.
    pub fn fresh_bounded_var(&mut self, bound: Type) -> Type {
        let id = self.next_var_id;
        self.next_var_id += 1;
        Type::Var(TypeVar {
            id,
            name: None,
            bound: Some(Arc::new(bound)),
        })
    }

    /// Binds a type variable to a type.
    pub fn bind(&mut self, var_id: u32, ty: Type) {
        self.substitutions.insert(var_id, ty);
    }

    /// Resolves a type variable to its bound type, if any.
    pub fn resolve(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(var) => {
                if let Some(bound) = self.substitutions.get(&var.id) {
                    self.resolve(bound)
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Fully resolves all type variables in a type.
    pub fn resolve_deep(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(var) => {
                if let Some(bound) = self.substitutions.get(&var.id) {
                    self.resolve_deep(bound)
                } else {
                    ty.clone()
                }
            }
            Type::List(inner) => Type::list(self.resolve_deep(inner)),
            Type::Dict(k, v) => Type::dict(self.resolve_deep(k), self.resolve_deep(v)),
            Type::Set(inner) => Type::set(self.resolve_deep(inner)),
            Type::Tuple(elems) => {
                Type::Tuple(elems.iter().map(|t| self.resolve_deep(t)).collect())
            }
            Type::Optional(inner) => Type::optional(self.resolve_deep(inner)),
            Type::Union(types) => {
                Type::union(types.iter().map(|t| self.resolve_deep(t)).collect())
            }
            Type::Callable {
                params,
                returns,
                is_async,
            } => Type::Callable {
                params: params
                    .iter()
                    .map(|p| FuncParam {
                        ty: self.resolve_deep(&p.ty),
                        ..p.clone()
                    })
                    .collect(),
                returns: Arc::new(self.resolve_deep(returns)),
                is_async: *is_async,
            },
            Type::Generic { base, args } => Type::Generic {
                base: Arc::new(self.resolve_deep(base)),
                args: args.iter().map(|t| self.resolve_deep(t)).collect(),
            },
            Type::Ref { inner, mutable } => Type::Ref {
                inner: Arc::new(self.resolve_deep(inner)),
                mutable: *mutable,
            },
            Type::Owned(inner) => Type::Owned(Arc::new(self.resolve_deep(inner))),
            _ => ty.clone(),
        }
    }

    /// Pushes a new scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    /// Pops the current scope.
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Defines a variable in the current scope.
    pub fn define(&mut self, name: Symbol, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Looks up a variable in all scopes.
    pub fn lookup(&self, name: Symbol) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(&name) {
                return Some(ty);
            }
        }
        None
    }

    /// Returns an iterator over all names defined in any scope.
    /// Used for suggesting similar names in error messages.
    pub fn all_names(&self) -> impl Iterator<Item = &Symbol> {
        self.scopes.iter()
            .flat_map(|scope| scope.keys())
            .chain(self.named_types.keys())
    }

    /// Registers a named type.
    pub fn register_type(&mut self, name: Symbol, ty: Type) {
        self.named_types.insert(name, ty);
    }

    /// Looks up a named type.
    pub fn lookup_type(&self, name: Symbol) -> Option<&Type> {
        self.named_types.get(&name)
    }

    /// Records the type of an expression at a given span.
    pub fn record_expr_type(&mut self, span: Span, ty: Type) {
        self.expr_types.insert(span, ty);
    }

    /// Looks up the type of an expression by span.
    pub fn lookup_expr_type(&self, span: Span) -> Option<&Type> {
        self.expr_types.get(&span)
    }

    /// Registers the exported names from a module for star imports.
    pub fn register_module_exports(&mut self, module_name: &str, exports: Vec<(Symbol, Type)>) {
        self.module_exports.insert(module_name.to_string(), exports);
    }

    /// Looks up the exported names from a module for star imports.
    pub fn lookup_module_exports(&self, module_name: &str) -> Option<&Vec<(Symbol, Type)>> {
        self.module_exports.get(module_name)
    }


    /// Checks if two types are equal (after resolution).
    pub fn types_equal(&self, a: &Type, b: &Type) -> bool {
        let a = self.resolve(a);
        let b = self.resolve(b);
        a == b
    }

    /// Checks if a type is a subtype of another.
    pub fn is_subtype(&self, sub: &Type, sup: &Type) -> bool {
        let sub = self.resolve(sub);
        let sup = self.resolve(sup);

        // Any is a supertype of everything
        if matches!(sup, Type::Any) {
            return true;
        }

        // Everything is a subtype of Any
        if matches!(sub, Type::Any) {
            return true;
        }

        // Never is a subtype of everything
        if matches!(sub, Type::Never) {
            return true;
        }

        // Error types are compatible with everything (for error recovery)
        if matches!(sub, Type::Error) || matches!(sup, Type::Error) {
            return true;
        }

        // Unknown types are compatible with everything (inference placeholder)
        if matches!(sub, Type::Unknown) || matches!(sup, Type::Unknown) {
            return true;
        }

        // Type variables (unbound) are compatible with any type
        // This allows `[]` (List[T]) to be assigned to `list[int]`
        if matches!(sub, Type::Var(_)) || matches!(sup, Type::Var(_)) {
            return true;
        }

        match (&sub, &sup) {
            // Same type
            (a, b) if a == b => true,

            // Class types: compare by name only (two Point types are the same if named Point)
            (Type::Class(a), Type::Class(b)) => a.name == b.name,

            // None is a subtype of Optional[T]
            (Type::NoneType, Type::Optional(_)) => true,

            // T is a subtype of Optional[T]
            (t, Type::Optional(inner)) => self.is_subtype(t, inner),

            // Union subtyping
            (Type::Union(subs), _) => subs.iter().all(|s| self.is_subtype(s, &sup)),
            (_, Type::Union(sups)) => sups.iter().any(|s| self.is_subtype(&sub, s)),

            // List covariance
            (Type::List(a), Type::List(b)) => self.is_subtype(a, b),

            // Set covariance
            (Type::Set(a), Type::Set(b)) => self.is_subtype(a, b),

            // Rc covariance - rc[T] is subtype of rc[U] if T is subtype of U
            (Type::Rc(a), Type::Rc(b)) => self.is_subtype(a, b),

            // Dict covariance (simplified)
            (Type::Dict(k1, v1), Type::Dict(k2, v2)) => {
                self.is_subtype(k1, k2) && self.is_subtype(v1, v2)
            }

            // Tuple subtyping
            (Type::Tuple(a), Type::Tuple(b)) if a.len() == b.len() => {
                a.iter().zip(b.iter()).all(|(a, b)| self.is_subtype(a, b))
            }

            // Function subtyping (contravariant params, covariant return)
            (
                Type::Callable {
                    params: params1,
                    returns: ret1,
                    ..
                },
                Type::Callable {
                    params: params2,
                    returns: ret2,
                    ..
                },
            ) => {
                params1.len() == params2.len()
                    && params1
                        .iter()
                        .zip(params2.iter())
                        .all(|(p1, p2)| self.is_subtype(&p2.ty, &p1.ty))
                    && self.is_subtype(ret1, ret2)
            }

            // Numeric widening (small to large)
            (Type::Int8, Type::Int16 | Type::Int32 | Type::Int64 | Type::Int128 | Type::Int) => {
                true
            }
            (Type::Int16, Type::Int32 | Type::Int64 | Type::Int128 | Type::Int) => true,
            (Type::Int32, Type::Int64 | Type::Int128 | Type::Int) => true,
            (Type::Int64, Type::Int128 | Type::Int) => true,
            (Type::Float32, Type::Float64 | Type::Float) => true,
            (Type::Float64, Type::Float) => true,

            // Numeric narrowing for literals (int can be assigned to any int type)
            // This allows: x: i8 = 127
            (Type::Int, Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64 | Type::Int128) => true,
            (Type::Int128, Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64) => true,

            // Float narrowing for literals
            (Type::Float, Type::Float32 | Type::Float64) => true,
            (Type::Float64, Type::Float32) => true,

            // Unsigned integers
            (Type::UInt8, Type::UInt16 | Type::UInt32 | Type::UInt64 | Type::UInt128 | Type::UInt) => true,
            (Type::UInt16, Type::UInt32 | Type::UInt64 | Type::UInt128 | Type::UInt) => true,
            (Type::UInt32, Type::UInt64 | Type::UInt128 | Type::UInt) => true,
            (Type::UInt64, Type::UInt128 | Type::UInt) => true,
            (Type::UInt, Type::UInt8 | Type::UInt16 | Type::UInt32 | Type::UInt64 | Type::UInt128) => true,

            // Int to UInt (for positive literals)
            (Type::Int, Type::UInt8 | Type::UInt16 | Type::UInt32 | Type::UInt64 | Type::UInt128 | Type::UInt) => true,

            // Reference subtyping
            (Type::Ref { inner: a, .. }, Type::Ref { inner: b, .. }) => self.is_subtype(a, b),

            // Owned subtyping: owned T is a subtype of T (and vice versa for compatibility)
            (Type::Owned(inner), other) | (other, Type::Owned(inner)) => {
                self.is_subtype(inner, other) || self.is_subtype(other, inner)
            }

            _ => false,
        }
    }

    /// Finds the least upper bound (join) of two types.
    pub fn join(&self, a: &Type, b: &Type) -> Type {
        let a = self.resolve(a);
        let b = self.resolve(b);

        if self.is_subtype(&a, &b) {
            b
        } else if self.is_subtype(&b, &a) {
            a
        } else {
            // Create a union type
            Type::Union(vec![a, b])
        }
    }

    /// Finds the greatest lower bound (meet) of two types.
    pub fn meet(&self, a: &Type, b: &Type) -> Option<Type> {
        let a = self.resolve(a);
        let b = self.resolve(b);

        if self.is_subtype(&a, &b) {
            Some(a)
        } else if self.is_subtype(&b, &a) {
            Some(b)
        } else {
            // No common subtype
            None
        }
    }
}

impl Default for TypeContext {
    fn default() -> Self {
        Self::new()
    }
}
