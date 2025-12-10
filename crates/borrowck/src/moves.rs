//! Move analysis for tracking ownership transfer.
//!
//! Tracks when values are moved and ensures no use-after-move.

use crate::loans::Location;
use crate::places::{Place, PlaceId};
use roast_common::Span;
use rustc_hash::FxHashMap;
use std::fmt;

/// Unique identifier for a move path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MovePathId(pub u32);

impl fmt::Display for MovePathId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mp{}", self.0)
    }
}

/// A move path represents a place that can be moved.
#[derive(Clone, Debug)]
pub struct MovePath {
    pub id: MovePathId,
    pub place: Place,
    pub parent: Option<MovePathId>,
    pub children: Vec<MovePathId>,
}

impl MovePath {
    pub fn new(id: MovePathId, place: Place, parent: Option<MovePathId>) -> Self {
        Self {
            id,
            place,
            parent,
            children: Vec::new(),
        }
    }
}

/// Data about a move or copy.
#[derive(Clone, Debug)]
pub struct MoveInfo {
    /// The move path that was moved.
    pub path: MovePathId,
    /// Where the move occurred.
    pub location: Location,
    /// The source span.
    pub span: Span,
    /// Whether this is a copy (for Copy types).
    pub is_copy: bool,
}

/// Tracks all move data for a function.
pub struct MoveData {
    /// All move paths.
    paths: Vec<MovePath>,
    /// Map from place to move path.
    place_to_path: FxHashMap<Place, MovePathId>,
    /// All moves.
    moves: Vec<MoveInfo>,
    /// Initialization status for each path.
    init_status: FxHashMap<MovePathId, InitStatus>,
}

/// Initialization status of a move path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitStatus {
    /// The place is definitely initialized.
    Initialized,
    /// The place is definitely uninitialized (moved out).
    Uninitialized,
    /// The place might be initialized (depends on control flow).
    MaybeInitialized,
    /// The place might be uninitialized (depends on control flow).
    MaybeUninitialized,
}

impl MoveData {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            place_to_path: FxHashMap::default(),
            moves: Vec::new(),
            init_status: FxHashMap::default(),
        }
    }

    /// Gets or creates a move path for a place.
    pub fn get_or_create_path(&mut self, place: &Place) -> MovePathId {
        if let Some(&id) = self.place_to_path.get(place) {
            return id;
        }

        // Create parent path first
        let parent_id = if let Some(parent_place) = place.parent() {
            Some(self.get_or_create_path(&parent_place))
        } else {
            None
        };

        let id = MovePathId(self.paths.len() as u32);
        let path = MovePath::new(id, place.clone(), parent_id);
        self.paths.push(path);
        self.place_to_path.insert(place.clone(), id);

        // Add as child of parent
        if let Some(parent_id) = parent_id {
            self.paths[parent_id.0 as usize].children.push(id);
        }

        id
    }

    /// Gets the move path for a place, if it exists.
    pub fn get_path(&self, place: &Place) -> Option<MovePathId> {
        self.place_to_path.get(place).copied()
    }

    /// Gets a move path by ID.
    pub fn path(&self, id: MovePathId) -> Option<&MovePath> {
        self.paths.get(id.0 as usize)
    }

    /// Records a move.
    pub fn record_move(&mut self, place: &Place, location: Location, span: Span, is_copy: bool) {
        let path = self.get_or_create_path(place);
        self.moves.push(MoveInfo {
            path,
            location,
            span,
            is_copy,
        });

        if !is_copy {
            self.init_status.insert(path, InitStatus::Uninitialized);
            // Also mark children as uninitialized
            if let Some(move_path) = self.paths.get(path.0 as usize) {
                for &child in &move_path.children.clone() {
                    self.init_status.insert(child, InitStatus::Uninitialized);
                }
            }
        }
    }

    /// Records an initialization.
    pub fn record_init(&mut self, place: &Place, _location: Location) {
        let path = self.get_or_create_path(place);
        self.init_status.insert(path, InitStatus::Initialized);
    }

    /// Checks if a place is initialized at a given point.
    pub fn is_initialized(&self, place: &Place) -> InitStatus {
        if let Some(&id) = self.place_to_path.get(place) {
            self.init_status.get(&id).copied().unwrap_or(InitStatus::Initialized)
        } else {
            // If we haven't tracked it, assume initialized
            InitStatus::Initialized
        }
    }

    /// Returns all moves of a place.
    pub fn moves_of(&self, place: &Place) -> Vec<&MoveInfo> {
        if let Some(&path) = self.place_to_path.get(place) {
            self.moves.iter().filter(|m| m.path == path).collect()
        } else {
            Vec::new()
        }
    }

    /// Returns all moves.
    pub fn all_moves(&self) -> &[MoveInfo] {
        &self.moves
    }

    /// Returns all paths.
    pub fn all_paths(&self) -> &[MovePath] {
        &self.paths
    }
}

impl Default for MoveData {
    fn default() -> Self {
        Self::new()
    }
}

/// Move error kinds.
#[derive(Clone, Debug)]
pub enum MoveError {
    /// Use of moved value.
    UseAfterMove {
        place: Place,
        moved_at: Location,
        used_at: Location,
        move_span: Span,
        use_span: Span,
    },
    /// Move of borrowed value.
    MoveWhileBorrowed {
        place: Place,
        borrowed_at: Location,
        moved_at: Location,
        borrow_span: Span,
        move_span: Span,
    },
    /// Partial move.
    PartialMove {
        place: Place,
        partial_path: Place,
        span: Span,
    },
    /// Use of partially moved value.
    UseAfterPartialMove {
        place: Place,
        moved_field: Place,
        span: Span,
    },
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::UseAfterMove { place, .. } => {
                write!(f, "use of moved value: `{}`", place)
            }
            MoveError::MoveWhileBorrowed { place, .. } => {
                write!(f, "cannot move out of `{}` while borrowed", place)
            }
            MoveError::PartialMove { place, partial_path, .. } => {
                write!(f, "cannot move out of `{}` because `{}` is partially moved", place, partial_path)
            }
            MoveError::UseAfterPartialMove { place, moved_field, .. } => {
                write!(f, "use of partially moved value `{}` (field `{}` was moved)", place, moved_field)
            }
        }
    }
}

