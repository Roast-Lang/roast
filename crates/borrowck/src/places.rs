//! Place representation for borrow checking.
//!
//! A place is a path that identifies a memory location.

use roast_common::Symbol;
use smallvec::SmallVec;
use std::fmt;

/// A place identifier (variable or temporary).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaceId(pub u32);

/// A place represents a path to a memory location.
///
/// Examples:
/// - `x` - local variable
/// - `x.field` - field access
/// - `x[i]` - index access
/// - `*x` - dereference
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    /// The base local variable.
    pub local: PlaceId,
    /// Projections from the base.
    pub projection: SmallVec<[PlaceElem; 4]>,
}

impl Place {
    /// Creates a place for a local variable.
    pub fn local(id: PlaceId) -> Self {
        Self {
            local: id,
            projection: SmallVec::new(),
        }
    }

    /// Creates a place with projections.
    pub fn with_projection(local: PlaceId, projection: SmallVec<[PlaceElem; 4]>) -> Self {
        Self { local, projection }
    }

    /// Adds a field projection.
    pub fn field(mut self, field: u32, symbol: Option<Symbol>) -> Self {
        self.projection.push(PlaceElem::Field(field, symbol));
        self
    }

    /// Adds an index projection.
    pub fn index(mut self, index: PlaceId) -> Self {
        self.projection.push(PlaceElem::Index(index));
        self
    }

    /// Adds a deref projection.
    pub fn deref(mut self) -> Self {
        self.projection.push(PlaceElem::Deref);
        self
    }

    /// Returns true if this place is a local variable (no projections).
    pub fn is_local(&self) -> bool {
        self.projection.is_empty()
    }

    /// Returns true if this place involves a dereference.
    pub fn has_deref(&self) -> bool {
        self.projection.iter().any(|p| matches!(p, PlaceElem::Deref))
    }

    /// Returns true if this place is a prefix of another.
    /// For example, `x` is a prefix of `x.f`, and `x.f` is a prefix of `x.f.g`.
    pub fn is_prefix_of(&self, other: &Place) -> bool {
        if self.local != other.local {
            return false;
        }
        if self.projection.len() > other.projection.len() {
            return false;
        }
        self.projection
            .iter()
            .zip(other.projection.iter())
            .all(|(a, b)| a == b)
    }

    /// Returns true if two places may overlap.
    pub fn may_overlap(&self, other: &Place) -> bool {
        self.is_prefix_of(other) || other.is_prefix_of(self)
    }

    /// Returns the parent place (without the last projection).
    pub fn parent(&self) -> Option<Place> {
        if self.projection.is_empty() {
            None
        } else {
            let mut projection = self.projection.clone();
            projection.pop();
            Some(Place {
                local: self.local,
                projection,
            })
        }
    }

    /// Returns the depth of this place (number of projections).
    pub fn depth(&self) -> usize {
        self.projection.len()
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "_{}", self.local.0)?;
        for proj in &self.projection {
            match proj {
                PlaceElem::Field(idx, Some(sym)) => write!(f, ".{}", sym.as_raw())?,
                PlaceElem::Field(idx, None) => write!(f, ".{}", idx)?,
                PlaceElem::Index(local) => write!(f, "[_{}]", local.0)?,
                PlaceElem::ConstIndex { offset, from_end, .. } => {
                    if *from_end {
                        write!(f, "[-{}]", offset)?;
                    } else {
                        write!(f, "[{}]", offset)?;
                    }
                }
                PlaceElem::Subslice { from, to, from_end } => {
                    if *from_end {
                        write!(f, "[{}..-{}]", from, to)?;
                    } else {
                        write!(f, "[{}..{}]", from, to)?;
                    }
                }
                PlaceElem::Deref => write!(f, ".*")?,
                PlaceElem::Downcast(idx) => write!(f, " as variant#{}", idx)?,
            }
        }
        Ok(())
    }
}

/// A projection element in a place.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlaceElem {
    /// Field access: `place.field`
    Field(u32, Option<Symbol>),
    /// Index access: `place[index]`
    Index(PlaceId),
    /// Constant index: `place[N]`
    ConstIndex {
        offset: u32,
        min_length: u32,
        from_end: bool,
    },
    /// Subslice: `place[from..to]`
    Subslice {
        from: u32,
        to: u32,
        from_end: bool,
    },
    /// Dereference: `*place`
    Deref,
    /// Downcast to variant: `place as Variant`
    Downcast(u32),
}

/// A set of places for tracking what's accessed.
#[derive(Clone, Debug, Default)]
pub struct PlaceSet {
    places: Vec<Place>,
}

impl PlaceSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, place: Place) {
        if !self.contains(&place) {
            self.places.push(place);
        }
    }

    pub fn contains(&self, place: &Place) -> bool {
        self.places.contains(place)
    }

    pub fn contains_prefix_of(&self, place: &Place) -> bool {
        self.places.iter().any(|p| p.is_prefix_of(place))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Place> {
        self.places.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Removes places that are prefixes of the given place.
    pub fn remove_prefixes_of(&mut self, place: &Place) {
        self.places.retain(|p| !p.is_prefix_of(place));
    }
}

impl FromIterator<Place> for PlaceSet {
    fn from_iter<T: IntoIterator<Item = Place>>(iter: T) -> Self {
        Self {
            places: iter.into_iter().collect(),
        }
    }
}

