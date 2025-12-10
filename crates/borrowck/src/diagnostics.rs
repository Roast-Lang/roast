//! Borrow checker diagnostics.

use crate::loans::{Loan, LoanKind, Location};
use crate::moves::MoveError;
use crate::places::Place;
use crate::regions::RegionError;
use roast_common::{Diagnostic, DiagnosticKind, Span};
use std::fmt;

/// Borrow checker error.
#[derive(Clone, Debug)]
pub enum BorrowError {
    /// Cannot borrow as mutable because already borrowed.
    ConflictingBorrow {
        place: Place,
        new_kind: LoanKind,
        existing: LoanInfo,
        new_span: Span,
    },

    /// Cannot use borrowed value.
    UseWhileBorrowed {
        place: Place,
        borrow: LoanInfo,
        use_span: Span,
    },

    /// Cannot move out of borrowed content.
    MoveWhileBorrowed {
        place: Place,
        borrow: LoanInfo,
        move_span: Span,
    },

    /// Cannot assign to borrowed value.
    AssignWhileBorrowed {
        place: Place,
        borrow: LoanInfo,
        assign_span: Span,
    },

    /// Use of moved value.
    UseAfterMove {
        place: Place,
        moved_at: Span,
        used_at: Span,
    },

    /// Borrow may not live long enough.
    BorrowTooShort {
        place: Place,
        borrow_span: Span,
        drop_span: Span,
    },

    /// Cannot return reference to local.
    ReturnLocalRef {
        place: Place,
        borrow_span: Span,
        return_span: Span,
    },

    /// Cannot borrow as mutable more than once.
    DoubleMutableBorrow {
        place: Place,
        first_borrow: Span,
        second_borrow: Span,
    },

    /// Cannot use partially moved value.
    PartiallyMoved {
        place: Place,
        moved_field: Place,
        span: Span,
    },

    /// Move out of non-movable type.
    CannotMove {
        place: Place,
        reason: String,
        span: Span,
    },

    /// Region error.
    Region(RegionError),

    /// Move error.
    Move(MoveError),
}

/// Information about an existing loan (for error messages).
#[derive(Clone, Debug)]
pub struct LoanInfo {
    pub kind: LoanKind,
    pub span: Span,
}

impl BorrowError {
    /// Creates a conflicting borrow error.
    pub fn conflicting_borrow(
        place: Place,
        new_kind: LoanKind,
        existing_kind: LoanKind,
        existing_span: Span,
        new_span: Span,
    ) -> Self {
        BorrowError::ConflictingBorrow {
            place,
            new_kind,
            existing: LoanInfo {
                kind: existing_kind,
                span: existing_span,
            },
            new_span,
        }
    }

    /// Creates a use-while-borrowed error.
    pub fn use_while_borrowed(
        place: Place,
        borrow_kind: LoanKind,
        borrow_span: Span,
        use_span: Span,
    ) -> Self {
        BorrowError::UseWhileBorrowed {
            place,
            borrow: LoanInfo {
                kind: borrow_kind,
                span: borrow_span,
            },
            use_span,
        }
    }

    /// Creates a move-while-borrowed error.
    pub fn move_while_borrowed(
        place: Place,
        borrow_kind: LoanKind,
        borrow_span: Span,
        move_span: Span,
    ) -> Self {
        BorrowError::MoveWhileBorrowed {
            place,
            borrow: LoanInfo {
                kind: borrow_kind,
                span: borrow_span,
            },
            move_span,
        }
    }

    /// Creates a use-after-move error.
    pub fn use_after_move(place: Place, moved_at: Span, used_at: Span) -> Self {
        BorrowError::UseAfterMove {
            place,
            moved_at,
            used_at,
        }
    }

    /// Converts to a diagnostic.
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            BorrowError::ConflictingBorrow {
                place,
                new_kind,
                existing,
                new_span,
            } => {
                let msg = format!(
                    "cannot borrow `{}` as {} because it is already borrowed as {}",
                    place, new_kind, existing.kind
                );
                Diagnostic::error(msg)
                    .with_primary_label(*new_span, format!("{} borrow occurs here", new_kind))
                    .with_secondary_label(
                        existing.span,
                        format!("{} borrow occurs here", existing.kind),
                    )
            }
            BorrowError::UseWhileBorrowed {
                place,
                borrow,
                use_span,
            } => {
                let msg = format!("cannot use `{}` because it is borrowed", place);
                Diagnostic::error(msg)
                    .with_primary_label(*use_span, "use occurs here")
                    .with_secondary_label(borrow.span, format!("{} borrow occurs here", borrow.kind))
            }
            BorrowError::MoveWhileBorrowed {
                place,
                borrow,
                move_span,
            } => {
                let msg = format!("cannot move out of `{}` because it is borrowed", place);
                Diagnostic::error(msg)
                    .with_primary_label(*move_span, "move occurs here")
                    .with_secondary_label(borrow.span, format!("{} borrow occurs here", borrow.kind))
            }
            BorrowError::AssignWhileBorrowed {
                place,
                borrow,
                assign_span,
            } => {
                let msg = format!("cannot assign to `{}` because it is borrowed", place);
                Diagnostic::error(msg)
                    .with_primary_label(*assign_span, "assignment occurs here")
                    .with_secondary_label(borrow.span, format!("{} borrow occurs here", borrow.kind))
            }
            BorrowError::UseAfterMove {
                place,
                moved_at,
                used_at,
            } => {
                let msg = format!("use of moved value: `{}`", place);
                Diagnostic::error(msg)
                    .with_primary_label(*used_at, "value used here after move")
                    .with_secondary_label(*moved_at, "value moved here")
                    .with_note("consider cloning the value if you need to use it again")
            }
            BorrowError::BorrowTooShort {
                place,
                borrow_span,
                drop_span,
            } => {
                let msg = format!("borrowed value does not live long enough: `{}`", place);
                Diagnostic::error(msg)
                    .with_primary_label(*drop_span, "value dropped here while still borrowed")
                    .with_secondary_label(*borrow_span, "borrow occurs here")
            }
            BorrowError::ReturnLocalRef {
                place,
                borrow_span,
                return_span,
            } => {
                let msg = format!("cannot return reference to local variable `{}`", place);
                Diagnostic::error(msg)
                    .with_primary_label(*return_span, "returns a reference to local")
                    .with_secondary_label(*borrow_span, "local variable created here")
            }
            BorrowError::DoubleMutableBorrow {
                place,
                first_borrow,
                second_borrow,
            } => {
                let msg = format!("cannot borrow `{}` as mutable more than once", place);
                Diagnostic::error(msg)
                    .with_primary_label(*second_borrow, "second mutable borrow occurs here")
                    .with_secondary_label(*first_borrow, "first mutable borrow occurs here")
            }
            BorrowError::PartiallyMoved {
                place,
                moved_field,
                span,
            } => {
                let msg = format!(
                    "use of partially moved value: `{}`",
                    place
                );
                Diagnostic::error(msg)
                    .with_primary_label(*span, "value used here")
                    .with_note(format!("field `{}` was moved", moved_field))
            }
            BorrowError::CannotMove {
                place,
                reason,
                span,
            } => {
                let msg = format!("cannot move out of `{}`", place);
                Diagnostic::error(msg)
                    .with_primary_label(*span, "cannot move")
                    .with_note(reason.clone())
            }
            BorrowError::Region(e) => {
                Diagnostic::error(format!("region error: {}", e))
            }
            BorrowError::Move(e) => {
                Diagnostic::error(format!("move error: {}", e))
            }
        }
    }

    /// Returns the primary span for this error.
    pub fn span(&self) -> Span {
        match self {
            BorrowError::ConflictingBorrow { new_span, .. } => *new_span,
            BorrowError::UseWhileBorrowed { use_span, .. } => *use_span,
            BorrowError::MoveWhileBorrowed { move_span, .. } => *move_span,
            BorrowError::AssignWhileBorrowed { assign_span, .. } => *assign_span,
            BorrowError::UseAfterMove { used_at, .. } => *used_at,
            BorrowError::BorrowTooShort { drop_span, .. } => *drop_span,
            BorrowError::ReturnLocalRef { return_span, .. } => *return_span,
            BorrowError::DoubleMutableBorrow { second_borrow, .. } => *second_borrow,
            BorrowError::PartiallyMoved { span, .. } => *span,
            BorrowError::CannotMove { span, .. } => *span,
            BorrowError::Region(_) | BorrowError::Move(_) => Span::dummy(),
        }
    }
}

impl fmt::Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BorrowError::ConflictingBorrow { place, new_kind, existing, .. } => {
                write!(
                    f,
                    "cannot borrow `{}` as {} because it is already borrowed as {}",
                    place, new_kind, existing.kind
                )
            }
            BorrowError::UseWhileBorrowed { place, .. } => {
                write!(f, "cannot use `{}` because it is borrowed", place)
            }
            BorrowError::MoveWhileBorrowed { place, .. } => {
                write!(f, "cannot move out of `{}` because it is borrowed", place)
            }
            BorrowError::AssignWhileBorrowed { place, .. } => {
                write!(f, "cannot assign to `{}` because it is borrowed", place)
            }
            BorrowError::UseAfterMove { place, .. } => {
                write!(f, "use of moved value: `{}`", place)
            }
            BorrowError::BorrowTooShort { place, .. } => {
                write!(f, "borrowed value does not live long enough: `{}`", place)
            }
            BorrowError::ReturnLocalRef { place, .. } => {
                write!(f, "cannot return reference to local variable `{}`", place)
            }
            BorrowError::DoubleMutableBorrow { place, .. } => {
                write!(f, "cannot borrow `{}` as mutable more than once", place)
            }
            BorrowError::PartiallyMoved { place, .. } => {
                write!(f, "use of partially moved value: `{}`", place)
            }
            BorrowError::CannotMove { place, reason, .. } => {
                write!(f, "cannot move out of `{}`: {}", place, reason)
            }
            BorrowError::Region(e) => write!(f, "{}", e),
            BorrowError::Move(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for BorrowError {}

