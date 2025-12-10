//! Type substitution for generics.

use crate::types::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// A mapping from type variables to types.
pub struct Substitution {
    map: FxHashMap<u32, Type>,
}

impl Substitution {
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
        }
    }

    pub fn insert(&mut self, var_id: u32, ty: Type) {
        self.map.insert(var_id, ty);
    }

    pub fn get(&self, var_id: u32) -> Option<&Type> {
        self.map.get(&var_id)
    }

    pub fn apply(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(var) => self.map.get(&var.id).cloned().unwrap_or_else(|| ty.clone()),
            Type::List(inner) => Type::list(self.apply(inner)),
            Type::Dict(k, v) => Type::dict(self.apply(k), self.apply(v)),
            Type::Set(inner) => Type::set(self.apply(inner)),
            Type::Tuple(elems) => Type::Tuple(elems.iter().map(|t| self.apply(t)).collect()),
            Type::Optional(inner) => Type::optional(self.apply(inner)),
            Type::Union(types) => Type::union(types.iter().map(|t| self.apply(t)).collect()),
            Type::Callable { params, returns, is_async } => Type::Callable {
                params: params.iter().map(|p| FuncParam {
                    ty: self.apply(&p.ty),
                    ..p.clone()
                }).collect(),
                returns: Arc::new(self.apply(returns)),
                is_async: *is_async,
            },
            Type::Generic { base, args } => Type::Generic {
                base: Arc::new(self.apply(base)),
                args: args.iter().map(|t| self.apply(t)).collect(),
            },
            Type::Ref { inner, mutable } => Type::Ref {
                inner: Arc::new(self.apply(inner)),
                mutable: *mutable,
            },
            Type::Owned(inner) => Type::Owned(Arc::new(self.apply(inner))),
            _ => ty.clone(),
        }
    }

    pub fn compose(&self, other: &Substitution) -> Substitution {
        let mut result = Substitution::new();
        for (&var_id, ty) in &self.map {
            result.insert(var_id, other.apply(ty));
        }
        for (&var_id, ty) in &other.map {
            if !result.map.contains_key(&var_id) {
                result.insert(var_id, ty.clone());
            }
        }
        result
    }
}

impl Default for Substitution {
    fn default() -> Self {
        Self::new()
    }
}

