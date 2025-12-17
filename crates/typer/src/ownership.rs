//! Ownership analysis for the type system.
//!
//! This module determines whether types are Copy, Clone, or require
//! ownership tracking.

use crate::types::*;
use roast_common::Symbol;
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// Trait that a type may implement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeTrait {
    /// Type can be copied bitwise (no ownership tracking needed).
    Copy,
    /// Type can be cloned (explicit copy).
    Clone,
    /// Type can be sent between threads.
    Send,
    /// Type can be shared between threads.
    Sync,
    /// Type has a known size at compile time.
    Sized,
    /// Type can be dropped.
    Drop,
}

/// Ownership mode for a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum OwnershipMode {
    /// Value is owned (default for non-Copy types).
    #[default]
    Owned,
    /// Value is a shared (immutable) reference.
    SharedRef,
    /// Value is a mutable reference.
    MutRef,
    /// Value is borrowed (Roast-specific, allows implicit reborrow).
    Borrowed,
    /// Value is being moved.
    Moving,
}

impl OwnershipMode {
    pub fn is_ref(&self) -> bool {
        matches!(self, OwnershipMode::SharedRef | OwnershipMode::MutRef | OwnershipMode::Borrowed)
    }

    pub fn is_mutable(&self) -> bool {
        matches!(self, OwnershipMode::MutRef | OwnershipMode::Owned | OwnershipMode::Moving)
    }
}

/// Result of ownership analysis for a type.
#[derive(Clone, Debug)]
pub struct OwnershipInfo {
    /// Traits the type implements.
    pub traits: FxHashSet<TypeTrait>,
    /// Whether the type needs drop glue.
    pub needs_drop: bool,
    /// Whether the type has interior mutability.
    pub interior_mutable: bool,
}

impl OwnershipInfo {
    pub fn new() -> Self {
        Self {
            traits: FxHashSet::default(),
            needs_drop: false,
            interior_mutable: false,
        }
    }

    pub fn is_copy(&self) -> bool {
        self.traits.contains(&TypeTrait::Copy)
    }

    pub fn is_clone(&self) -> bool {
        self.traits.contains(&TypeTrait::Clone)
    }

    pub fn is_send(&self) -> bool {
        self.traits.contains(&TypeTrait::Send)
    }

    pub fn is_sync(&self) -> bool {
        self.traits.contains(&TypeTrait::Sync)
    }
}

impl Default for OwnershipInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Analyzes ownership properties of types.
pub struct OwnershipAnalyzer {
    /// Cache of analyzed types.
    cache: rustc_hash::FxHashMap<TypeKey, OwnershipInfo>,
}

/// Key for type caching (simplified type identity).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TypeKey {
    Primitive(PrimitiveKind),
    List,
    Dict,
    Set,
    Tuple(usize),
    Class(String),
    Ref(bool), // mutable
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PrimitiveKind {
    Bool,
    Int,
    Float,
    Str,
    Bytes,
    None,
}

impl OwnershipAnalyzer {
    pub fn new() -> Self {
        Self {
            cache: rustc_hash::FxHashMap::default(),
        }
    }

    /// Analyzes a type and returns its ownership information.
    pub fn analyze(&mut self, ty: &Type) -> OwnershipInfo {
        let key = self.type_key(ty);
        if let Some(info) = self.cache.get(&key) {
            return info.clone();
        }

        let info = self.analyze_uncached(ty);
        self.cache.insert(key, info.clone());
        info
    }

    fn type_key(&self, ty: &Type) -> TypeKey {
        match ty {
            Type::Bool => TypeKey::Primitive(PrimitiveKind::Bool),
            Type::Int | Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64 | Type::Int128 |
            Type::UInt | Type::UInt8 | Type::UInt16 | Type::UInt32 | Type::UInt64 | Type::UInt128 => {
                TypeKey::Primitive(PrimitiveKind::Int)
            }
            Type::Float | Type::Float32 | Type::Float64 => TypeKey::Primitive(PrimitiveKind::Float),
            Type::Str => TypeKey::Primitive(PrimitiveKind::Str),
            Type::Bytes => TypeKey::Primitive(PrimitiveKind::Bytes),
            Type::NoneType => TypeKey::Primitive(PrimitiveKind::None),
            Type::List(_) => TypeKey::List,
            Type::Dict(_, _) => TypeKey::Dict,
            Type::Set(_) => TypeKey::Set,
            Type::Tuple(elems) => TypeKey::Tuple(elems.len()),
            Type::Class(cls) => TypeKey::Class(cls.name.clone()),
            Type::Ref { mutable, .. } => TypeKey::Ref(*mutable),
            _ => TypeKey::Other,
        }
    }

    fn analyze_uncached(&mut self, ty: &Type) -> OwnershipInfo {
        let mut info = OwnershipInfo::new();

        match ty {
            // Primitive types are Copy
            Type::Bool | Type::Int | Type::Int8 | Type::Int16 | Type::Int32 |
            Type::Int64 | Type::Int128 | Type::UInt | Type::UInt8 | Type::UInt16 |
            Type::UInt32 | Type::UInt64 | Type::UInt128 | Type::Float |
            Type::Float32 | Type::Float64 | Type::Complex | Type::Complex64 |
            Type::Complex128 | Type::NoneType => {
                info.traits.insert(TypeTrait::Copy);
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Send);
                info.traits.insert(TypeTrait::Sync);
                info.traits.insert(TypeTrait::Sized);
            }

            // Strings are Clone but not Copy
            Type::Str | Type::Bytes => {
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Send);
                info.traits.insert(TypeTrait::Sync);
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
            }

            // References are Copy
            Type::Ref { inner, mutable } => {
                info.traits.insert(TypeTrait::Copy);
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Sized);
                
                // Inherit Send/Sync from inner type
                let inner_info = self.analyze(inner);
                if inner_info.is_sync() {
                    info.traits.insert(TypeTrait::Send);
                }
                if inner_info.is_sync() && !*mutable {
                    info.traits.insert(TypeTrait::Sync);
                }
            }

            // Reference-counted types are Copy (just copying the pointer)
            // This enables shared ownership: ref1 = ref2 = data
            Type::Rc(inner) => {
                info.traits.insert(TypeTrait::Copy);
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Sized);
                
                // Inherit Send/Sync from inner type
                let inner_info = self.analyze(inner);
                if inner_info.is_send() {
                    info.traits.insert(TypeTrait::Send);
                }
                if inner_info.is_sync() {
                    info.traits.insert(TypeTrait::Sync);
                }
            }

            // Collections need drop
            Type::List(elem) => {
                let elem_info = self.analyze(elem);
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
                
                if elem_info.is_send() {
                    info.traits.insert(TypeTrait::Send);
                }
                if elem_info.is_sync() {
                    info.traits.insert(TypeTrait::Sync);
                }
            }

            Type::Dict(key, val) => {
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
                
                let key_info = self.analyze(key);
                let val_info = self.analyze(val);
                if key_info.is_send() && val_info.is_send() {
                    info.traits.insert(TypeTrait::Send);
                }
                if key_info.is_sync() && val_info.is_sync() {
                    info.traits.insert(TypeTrait::Sync);
                }
            }

            Type::Set(elem) => {
                let elem_info = self.analyze(elem);
                info.traits.insert(TypeTrait::Clone);
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
                
                if elem_info.is_send() {
                    info.traits.insert(TypeTrait::Send);
                }
                if elem_info.is_sync() {
                    info.traits.insert(TypeTrait::Sync);
                }
            }

            // Tuples are Copy if all elements are Copy
            Type::Tuple(elems) => {
                info.traits.insert(TypeTrait::Sized);
                
                let elem_infos: Vec<_> = elems.iter().map(|e| self.analyze(e)).collect();
                
                if elem_infos.iter().all(|i| i.is_copy()) {
                    info.traits.insert(TypeTrait::Copy);
                }
                if elem_infos.iter().all(|i| i.is_clone()) {
                    info.traits.insert(TypeTrait::Clone);
                }
                if elem_infos.iter().all(|i| i.is_send()) {
                    info.traits.insert(TypeTrait::Send);
                }
                if elem_infos.iter().all(|i| i.is_sync()) {
                    info.traits.insert(TypeTrait::Sync);
                }
                if elem_infos.iter().any(|i| i.needs_drop) {
                    info.needs_drop = true;
                }
            }

            // Optionals inherit from inner
            Type::Optional(inner) => {
                let inner_info = self.analyze(inner);
                info.traits = inner_info.traits.clone();
                info.needs_drop = inner_info.needs_drop;
            }

            // Unions are complex
            Type::Union(types) => {
                info.traits.insert(TypeTrait::Sized);
                
                let type_infos: Vec<_> = types.iter().map(|t| self.analyze(t)).collect();
                
                if type_infos.iter().all(|i| i.is_copy()) {
                    info.traits.insert(TypeTrait::Copy);
                }
                if type_infos.iter().all(|i| i.is_clone()) {
                    info.traits.insert(TypeTrait::Clone);
                }
                if type_infos.iter().all(|i| i.is_send()) {
                    info.traits.insert(TypeTrait::Send);
                }
                if type_infos.iter().all(|i| i.is_sync()) {
                    info.traits.insert(TypeTrait::Sync);
                }
                if type_infos.iter().any(|i| i.needs_drop) {
                    info.needs_drop = true;
                }
            }

            // Functions are not Copy (they might capture)
            Type::Callable { .. } => {
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
            }

            // Classes need analysis of fields
            Type::Class(cls) => {
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
                
                // Would need to check if class has __copy__ or __clone__
                // For now, assume classes are Clone
                info.traits.insert(TypeTrait::Clone);
            }

            // Any type - assume worst case
            Type::Any => {
                info.traits.insert(TypeTrait::Sized);
                info.needs_drop = true;
            }

            // Error/Unknown - be conservative
            Type::Error | Type::Unknown | Type::Never => {
                info.traits.insert(TypeTrait::Sized);
            }

            _ => {
                info.traits.insert(TypeTrait::Sized);
            }
        }

        info
    }

    /// Checks if a type is Copy.
    pub fn is_copy(&mut self, ty: &Type) -> bool {
        self.analyze(ty).is_copy()
    }

    /// Checks if a type is Clone.
    pub fn is_clone(&mut self, ty: &Type) -> bool {
        self.analyze(ty).is_clone()
    }

    /// Checks if a type needs drop.
    pub fn needs_drop(&mut self, ty: &Type) -> bool {
        self.analyze(ty).needs_drop
    }

    /// Checks if a type is Send.
    pub fn is_send(&mut self, ty: &Type) -> bool {
        self.analyze(ty).is_send()
    }

    /// Checks if a type is Sync.
    pub fn is_sync(&mut self, ty: &Type) -> bool {
        self.analyze(ty).is_sync()
    }
}

impl Default for OwnershipAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates ownership constraints.
pub struct OwnershipValidator {
    analyzer: OwnershipAnalyzer,
    errors: Vec<OwnershipError>,
}

/// Ownership validation error.
#[derive(Clone, Debug)]
pub enum OwnershipError {
    /// Attempted to copy a non-Copy type.
    CannotCopy {
        ty: Type,
        span: roast_common::Span,
    },
    /// Attempted to move a borrowed value.
    CannotMoveBorrowed {
        ty: Type,
        span: roast_common::Span,
    },
    /// Lifetime mismatch.
    LifetimeMismatch {
        expected: String,
        found: String,
        span: roast_common::Span,
    },
}

impl OwnershipValidator {
    pub fn new() -> Self {
        Self {
            analyzer: OwnershipAnalyzer::new(),
            errors: Vec::new(),
        }
    }

    /// Validates that a type can be copied.
    pub fn validate_copy(&mut self, ty: &Type, span: roast_common::Span) -> bool {
        if self.analyzer.is_copy(ty) {
            true
        } else {
            self.errors.push(OwnershipError::CannotCopy {
                ty: ty.clone(),
                span,
            });
            false
        }
    }

    /// Returns all validation errors.
    pub fn errors(&self) -> &[OwnershipError] {
        &self.errors
    }

    /// Clears errors.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }
}

impl Default for OwnershipValidator {
    fn default() -> Self {
        Self::new()
    }
}

