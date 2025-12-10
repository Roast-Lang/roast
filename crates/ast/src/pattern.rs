//! Pattern AST nodes for match statements.

use crate::{Expr, Ident};
use roast_common::Span;

/// A pattern node for match statements.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

impl Pattern {
    pub fn new(kind: PatternKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Pattern kinds.
#[derive(Clone, Debug)]
pub enum PatternKind {
    /// Match a constant value
    MatchValue {
        value: Box<Expr>,
    },

    /// Match a single value and bind it: case x:
    MatchSingleton {
        value: Box<Expr>,
    },

    /// Match a sequence: case [a, b, *rest]:
    MatchSequence {
        patterns: Vec<Pattern>,
    },

    /// Match a mapping: case {"key": value}:
    MatchMapping {
        keys: Vec<Expr>,
        patterns: Vec<Pattern>,
        rest: Option<Ident>,
    },

    /// Match a class: case Point(x=px, y=py):
    MatchClass {
        cls: Box<Expr>,
        patterns: Vec<Pattern>,
        kwd_attrs: Vec<Ident>,
        kwd_patterns: Vec<Pattern>,
    },

    /// Match with star: case [first, *rest]:
    MatchStar {
        name: Option<Ident>,
    },

    /// Match with as: case pattern as name:
    MatchAs {
        pattern: Option<Box<Pattern>>,
        name: Option<Ident>,
    },

    /// Match with or: case pattern1 | pattern2:
    MatchOr {
        patterns: Vec<Pattern>,
    },

    /// Wildcard pattern: case _:
    Wildcard,

    /// Capture pattern: case name:
    Capture {
        name: Ident,
    },
}

impl Pattern {
    /// Returns true if this pattern is irrefutable (always matches).
    pub fn is_irrefutable(&self) -> bool {
        match &self.kind {
            PatternKind::Wildcard => true,
            PatternKind::Capture { .. } => true,
            PatternKind::MatchAs { pattern: None, .. } => true,
            PatternKind::MatchAs { pattern: Some(p), .. } => p.is_irrefutable(),
            PatternKind::MatchSequence { patterns } => patterns.iter().all(|p| p.is_irrefutable()),
            PatternKind::MatchOr { patterns } => patterns.iter().any(|p| p.is_irrefutable()),
            _ => false,
        }
    }

    /// Returns all names bound by this pattern.
    pub fn bound_names(&self) -> Vec<&Ident> {
        let mut names = Vec::new();
        self.collect_bound_names(&mut names);
        names
    }

    fn collect_bound_names<'a>(&'a self, names: &mut Vec<&'a Ident>) {
        match &self.kind {
            PatternKind::Capture { name } => names.push(name),
            PatternKind::MatchAs { pattern, name } => {
                if let Some(p) = pattern {
                    p.collect_bound_names(names);
                }
                if let Some(n) = name {
                    names.push(n);
                }
            }
            PatternKind::MatchStar { name: Some(n) } => names.push(n),
            PatternKind::MatchSequence { patterns } => {
                for p in patterns {
                    p.collect_bound_names(names);
                }
            }
            PatternKind::MatchMapping { patterns, rest, .. } => {
                for p in patterns {
                    p.collect_bound_names(names);
                }
                if let Some(r) = rest {
                    names.push(r);
                }
            }
            PatternKind::MatchClass {
                patterns,
                kwd_patterns,
                ..
            } => {
                for p in patterns {
                    p.collect_bound_names(names);
                }
                for p in kwd_patterns {
                    p.collect_bound_names(names);
                }
            }
            PatternKind::MatchOr { patterns } => {
                // In OR patterns, all branches must bind the same names
                if let Some(first) = patterns.first() {
                    first.collect_bound_names(names);
                }
            }
            _ => {}
        }
    }
}

