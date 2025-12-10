//! Lifetime representation and analysis.
//!
//! Lifetimes represent the scope during which a reference is valid.

use roast_common::Span;
use std::fmt;

/// Unique identifier for a lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LifetimeId(pub u32);

impl LifetimeId {
    pub const STATIC: LifetimeId = LifetimeId(0);

    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn is_static(&self) -> bool {
        *self == Self::STATIC
    }
}

impl fmt::Display for LifetimeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_static() {
            write!(f, "'static")
        } else {
            write!(f, "'{}", self.0)
        }
    }
}

/// A lifetime annotation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lifetime {
    pub id: LifetimeId,
    pub name: Option<String>,
    pub span: Span,
}

impl Lifetime {
    pub fn new(id: LifetimeId, span: Span) -> Self {
        Self {
            id,
            name: None,
            span,
        }
    }

    pub fn named(id: LifetimeId, name: String, span: Span) -> Self {
        Self {
            id,
            name: Some(name),
            span,
        }
    }

    pub fn static_lifetime() -> Self {
        Self {
            id: LifetimeId::STATIC,
            name: Some("static".to_string()),
            span: Span::dummy(),
        }
    }

    pub fn is_static(&self) -> bool {
        self.id.is_static()
    }
}

impl fmt::Display for Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref name) = self.name {
            write!(f, "'{}", name)
        } else {
            write!(f, "{}", self.id)
        }
    }
}

/// Lifetime bounds and constraints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifetimeBound {
    /// 'a: 'b - 'a outlives 'b
    Outlives(LifetimeId, LifetimeId),
    /// 'a = 'b - lifetimes are equal
    Equal(LifetimeId, LifetimeId),
}

impl LifetimeBound {
    pub fn outlives(longer: LifetimeId, shorter: LifetimeId) -> Self {
        LifetimeBound::Outlives(longer, shorter)
    }

    pub fn equal(a: LifetimeId, b: LifetimeId) -> Self {
        LifetimeBound::Equal(a, b)
    }
}

/// Lifetime context for tracking lifetimes during analysis.
pub struct LifetimeContext {
    next_id: u32,
    lifetimes: Vec<Lifetime>,
    bounds: Vec<LifetimeBound>,
}

impl LifetimeContext {
    pub fn new() -> Self {
        let mut ctx = Self {
            next_id: 1, // 0 is reserved for 'static
            lifetimes: Vec::new(),
            bounds: Vec::new(),
        };
        // Add 'static lifetime
        ctx.lifetimes.push(Lifetime::static_lifetime());
        ctx
    }

    /// Creates a fresh anonymous lifetime.
    pub fn fresh(&mut self, span: Span) -> LifetimeId {
        let id = LifetimeId::new(self.next_id);
        self.next_id += 1;
        self.lifetimes.push(Lifetime::new(id, span));
        id
    }

    /// Creates a named lifetime.
    pub fn named(&mut self, name: &str, span: Span) -> LifetimeId {
        let id = LifetimeId::new(self.next_id);
        self.next_id += 1;
        self.lifetimes.push(Lifetime::named(id, name.to_string(), span));
        id
    }

    /// Adds an outlives constraint: longer outlives shorter.
    pub fn add_outlives(&mut self, longer: LifetimeId, shorter: LifetimeId) {
        self.bounds.push(LifetimeBound::outlives(longer, shorter));
    }

    /// Adds an equality constraint.
    pub fn add_equal(&mut self, a: LifetimeId, b: LifetimeId) {
        self.bounds.push(LifetimeBound::equal(a, b));
    }

    /// Gets a lifetime by ID.
    pub fn get(&self, id: LifetimeId) -> Option<&Lifetime> {
        self.lifetimes.iter().find(|l| l.id == id)
    }

    /// Returns all bounds.
    pub fn bounds(&self) -> &[LifetimeBound] {
        &self.bounds
    }

    /// Checks if 'a outlives 'b (directly or transitively).
    pub fn outlives(&self, a: LifetimeId, b: LifetimeId) -> bool {
        // 'static outlives everything
        if a.is_static() {
            return true;
        }
        // Nothing outlives itself except 'static
        if a == b {
            return true;
        }

        // Check direct bounds
        for bound in &self.bounds {
            match bound {
                LifetimeBound::Outlives(longer, shorter) if *longer == a && *shorter == b => {
                    return true;
                }
                LifetimeBound::Equal(x, y) if (*x == a && *y == b) || (*x == b && *y == a) => {
                    return true;
                }
                _ => {}
            }
        }

        // TODO: Transitive closure
        false
    }
}

impl Default for LifetimeContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifetime variance for generic parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variance {
    /// Covariant: can substitute with longer lifetime
    Covariant,
    /// Contravariant: can substitute with shorter lifetime
    Contravariant,
    /// Invariant: must be exact
    Invariant,
    /// Bivariant: any substitution allowed (unused positions)
    Bivariant,
}

impl Variance {
    /// Combines variances: what is the variance of T in `Outer<Inner<T>>`?
    pub fn xform(self, inner: Variance) -> Variance {
        match (self, inner) {
            (Variance::Bivariant, _) | (_, Variance::Bivariant) => Variance::Bivariant,
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,
            (Variance::Covariant, Variance::Covariant) => Variance::Covariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Covariant,
            (Variance::Covariant, Variance::Contravariant) => Variance::Contravariant,
            (Variance::Contravariant, Variance::Covariant) => Variance::Contravariant,
        }
    }
}

