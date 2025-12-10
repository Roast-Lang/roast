//! Exhaustiveness checking for pattern matching.
//!
//! This module implements the "usefulness" algorithm based on:
//! - "Warnings for pattern matching" by Luc Maranget (2007)
//! - The Rust compiler's exhaustiveness checking
//!
//! The algorithm checks if a set of patterns covers all possible values of a type,
//! and reports which patterns are missing or redundant.

use crate::types::Type;
use roast_ast::{Expr, ExprKind, Pattern, PatternKind};
use roast_common::Span;
use rustc_hash::FxHashSet;
use std::fmt;

/// A constructor represents a way to build values of a type.
/// The key insight is that each type has a finite set of constructors.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constructor {
    /// Single constant value (e.g., `42`, `"hello"`)
    Single(ConstValue),
    /// Boolean true
    True,
    /// Boolean false  
    False,
    /// None literal
    None,
    /// Tuple constructor with arity
    Tuple(usize),
    /// List with specific length
    ListExact(usize),
    /// List with at least N elements (due to [x, *rest] patterns)
    ListAtLeast(usize),
    /// Dict/mapping pattern
    Dict,
    /// Class/struct constructor
    Class(String),
    /// Enum variant
    Variant(String, String), // (enum_name, variant_name)
    /// Wildcard (matches everything) - used internally
    Wildcard,
    /// Integer range (for numeric exhaustiveness)
    IntRange(Option<i128>, Option<i128>), // (min_inclusive, max_inclusive)
    /// Missing constructor (for generating witness patterns)
    Missing,
    /// Or pattern (internal - flattened before use)
    Or(Vec<Constructor>),
}

impl Constructor {
    /// Returns the arity (number of sub-patterns) of this constructor.
    pub fn arity(&self) -> usize {
        match self {
            Constructor::Tuple(n) => *n,
            Constructor::ListExact(n) => *n,
            Constructor::ListAtLeast(n) => *n,
            Constructor::Class(_) => 0, // Simplified - should query type info
            _ => 0,
        }
    }

    /// Returns true if this is a wildcard constructor.
    pub fn is_wildcard(&self) -> bool {
        matches!(self, Constructor::Wildcard)
    }
}

impl fmt::Display for Constructor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constructor::Single(v) => write!(f, "{}", v),
            Constructor::True => write!(f, "True"),
            Constructor::False => write!(f, "False"),
            Constructor::None => write!(f, "None"),
            Constructor::Tuple(n) => write!(f, "({})", ",".repeat(*n)),
            Constructor::ListExact(0) => write!(f, "[]"),
            Constructor::ListExact(n) => write!(f, "[{}]", "_, ".repeat(*n).trim_end_matches(", ")),
            Constructor::ListAtLeast(n) => write!(f, "[{}, ..]", "_, ".repeat(*n).trim_end_matches(", ")),
            Constructor::Dict => write!(f, "{{..}}"),
            Constructor::Class(name) => write!(f, "{}(..)", name),
            Constructor::Variant(enum_name, variant) => write!(f, "{}.{}", enum_name, variant),
            Constructor::Wildcard => write!(f, "_"),
            Constructor::IntRange(lo, hi) => {
                match (lo, hi) {
                    (Some(l), Some(h)) if l == h => write!(f, "{}", l),
                    (Some(l), Some(h)) => write!(f, "{}..={}", l, h),
                    (Some(l), None) => write!(f, "{}..", l),
                    (None, Some(h)) => write!(f, "..={}", h),
                    (None, None) => write!(f, ".."),
                }
            }
            Constructor::Missing => write!(f, "_"),
            Constructor::Or(ctors) => {
                let parts: Vec<_> = ctors.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
        }
    }
}

/// A constant value used in patterns.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstValue {
    Int(i128),
    Float(String), // Store as string for exact comparison
    Str(String),
    Bytes(Vec<u8>),
}

impl fmt::Display for ConstValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstValue::Int(i) => write!(f, "{}", i),
            ConstValue::Float(s) => write!(f, "{}", s),
            ConstValue::Str(s) => write!(f, "\"{}\"", s),
            ConstValue::Bytes(b) => write!(f, "b\"{:?}\"", b),
        }
    }
}

/// A deconstructed pattern for exhaustiveness analysis.
/// This is a simplified representation that's easier to work with.
#[derive(Clone, Debug)]
pub struct DeconstructedPat {
    pub ctor: Constructor,
    pub fields: Vec<DeconstructedPat>,
    pub span: Span,
}

impl DeconstructedPat {
    pub fn new(ctor: Constructor, fields: Vec<DeconstructedPat>, span: Span) -> Self {
        Self { ctor, fields, span }
    }

    pub fn wildcard(span: Span) -> Self {
        Self {
            ctor: Constructor::Wildcard,
            fields: Vec::new(),
            span,
        }
    }

    /// Convert from AST Pattern to DeconstructedPat.
    pub fn from_ast(pattern: &Pattern) -> Vec<Self> {
        // Returns Vec because OR patterns need to be flattened into multiple patterns
        match &pattern.kind {
            PatternKind::MatchValue { value } => {
                vec![Self::from_value_expr(value, pattern.span)]
            }
            PatternKind::MatchSingleton { value } => {
                vec![Self::from_value_expr(value, pattern.span)]
            }
            PatternKind::Wildcard | PatternKind::Capture { .. } => {
                vec![Self::wildcard(pattern.span)]
            }
            PatternKind::MatchAs { pattern: sub, .. } => {
                if let Some(p) = sub {
                    Self::from_ast(p)
                } else {
                    vec![Self::wildcard(pattern.span)]
                }
            }
            PatternKind::MatchSequence { patterns } => {
                let has_star = patterns.iter().any(|p| matches!(p.kind, PatternKind::MatchStar { .. }));
                
                if has_star {
                    // Variable-length sequence
                    let min_len = patterns.iter()
                        .filter(|p| !matches!(p.kind, PatternKind::MatchStar { .. }))
                        .count();
                    
                    let mut fields = Vec::new();
                    for p in patterns {
                        if !matches!(p.kind, PatternKind::MatchStar { .. }) {
                            fields.extend(Self::from_ast(p));
                        }
                    }
                    
                    vec![Self::new(Constructor::ListAtLeast(min_len), fields, pattern.span)]
                } else {
                    // Fixed-length sequence
                    let fields: Vec<_> = patterns.iter()
                        .flat_map(Self::from_ast)
                        .collect();
                    vec![Self::new(Constructor::ListExact(patterns.len()), fields, pattern.span)]
                }
            }
            PatternKind::MatchStar { .. } => {
                // Star patterns are handled in MatchSequence
                vec![Self::wildcard(pattern.span)]
            }
            PatternKind::MatchOr { patterns } => {
                // Flatten OR patterns into multiple separate patterns
                patterns.iter()
                    .flat_map(Self::from_ast)
                    .collect()
            }
            PatternKind::MatchMapping { keys, patterns, rest } => {
                // Dict pattern - simplified handling
                let fields: Vec<_> = patterns.iter()
                    .flat_map(Self::from_ast)
                    .collect();
                vec![Self::new(Constructor::Dict, fields, pattern.span)]
            }
            PatternKind::MatchClass { cls, patterns, kwd_patterns, .. } => {
                let class_name = extract_class_name(cls);
                let mut fields: Vec<_> = patterns.iter()
                    .flat_map(Self::from_ast)
                    .collect();
                fields.extend(kwd_patterns.iter().flat_map(Self::from_ast));
                vec![Self::new(Constructor::Class(class_name), fields, pattern.span)]
            }
        }
    }

    fn from_value_expr(expr: &Expr, span: Span) -> Self {
        let ctor = match &expr.kind {
            ExprKind::IntLit { value } => {
                // BigInt - convert to i128 if possible
                let int_val = value.to_string().parse::<i128>().unwrap_or(0);
                Constructor::Single(ConstValue::Int(int_val))
            }
            ExprKind::FloatLit { value } => {
                Constructor::Single(ConstValue::Float(value.to_string()))
            }
            ExprKind::StringLit { value, .. } => {
                Constructor::Single(ConstValue::Str(value.clone()))
            }
            ExprKind::BoolLit { value: true } => Constructor::True,
            ExprKind::BoolLit { value: false } => Constructor::False,
            ExprKind::NoneLit => Constructor::None,
            ExprKind::Tuple { elts, .. } if elts.is_empty() => Constructor::Tuple(0),
            ExprKind::List { elts, .. } if elts.is_empty() => Constructor::ListExact(0),
            _ => {
                // For other expressions, create a unique value constructor
                Constructor::Single(ConstValue::Str(format!("{:?}", expr.kind)))
            }
        };
        Self::new(ctor, Vec::new(), span)
    }
}

fn extract_class_name(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Name { id, .. } => id.name.to_string(),
        ExprKind::Attribute { value, attr, .. } => {
            format!("{}.{}", extract_class_name(value), attr.name)
        }
        _ => "Unknown".to_string()
    }
}

/// A row in the pattern matrix.
#[derive(Clone, Debug)]
struct PatternRow {
    pats: Vec<DeconstructedPat>,
    arm_idx: usize,
    guard: bool, // Whether this arm has a guard
}

/// The pattern matrix for usefulness checking.
#[derive(Clone, Debug)]
struct PatMatrix {
    rows: Vec<PatternRow>,
    num_columns: usize,
}

impl PatMatrix {
    fn new(num_columns: usize) -> Self {
        Self { rows: Vec::new(), num_columns }
    }

    fn push(&mut self, row: PatternRow) {
        debug_assert!(row.pats.len() == self.num_columns || self.num_columns == 0);
        if self.num_columns == 0 {
            self.num_columns = row.pats.len();
        }
        self.rows.push(row);
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Returns all constructors used in the first column.
    fn head_constructors(&self) -> Vec<Constructor> {
        let mut ctors = Vec::new();
        for row in &self.rows {
            if let Some(first) = row.pats.first() {
                if !first.ctor.is_wildcard() && !ctors.contains(&first.ctor) {
                    ctors.push(first.ctor.clone());
                }
            }
        }
        ctors
    }

    /// Specialize the matrix by the given constructor.
    fn specialize(&self, ctor: &Constructor) -> PatMatrix {
        let arity = ctor.arity();
        let mut new_matrix = PatMatrix::new(0);

        for row in &self.rows {
            if let Some(first) = row.pats.first() {
                let specialized_row = specialize_row(&row.pats, first, ctor, row.arm_idx, row.guard);
                if let Some(new_row) = specialized_row {
                    new_matrix.push(new_row);
                }
            }
        }

        new_matrix
    }
}

/// Specialize a single row by a constructor.
fn specialize_row(
    pats: &[DeconstructedPat],
    first: &DeconstructedPat,
    ctor: &Constructor,
    arm_idx: usize,
    guard: bool,
) -> Option<PatternRow> {
    let rest = &pats[1..];
    let arity = ctor.arity();

    match &first.ctor {
        Constructor::Wildcard => {
            // Wildcard matches any constructor - expand to the arity
            let mut new_pats = Vec::with_capacity(arity + rest.len());
            for _ in 0..arity {
                new_pats.push(DeconstructedPat::wildcard(first.span));
            }
            new_pats.extend(rest.iter().cloned());
            Some(PatternRow { pats: new_pats, arm_idx, guard })
        }
        c if constructors_match(c, ctor) => {
            // Constructors match - use the fields
            let mut new_pats = Vec::with_capacity(first.fields.len() + rest.len());
            new_pats.extend(first.fields.iter().cloned());
            // Pad with wildcards if needed
            while new_pats.len() < arity {
                new_pats.push(DeconstructedPat::wildcard(first.span));
            }
            new_pats.extend(rest.iter().cloned());
            Some(PatternRow { pats: new_pats, arm_idx, guard })
        }
        Constructor::Or(or_ctors) => {
            // Check if any constructor in the OR matches
            for or_ctor in or_ctors {
                if constructors_match(or_ctor, ctor) {
                    let mut new_pats = Vec::with_capacity(arity + rest.len());
                    for _ in 0..arity {
                        new_pats.push(DeconstructedPat::wildcard(first.span));
                    }
                    new_pats.extend(rest.iter().cloned());
                    return Some(PatternRow { pats: new_pats, arm_idx, guard });
                }
            }
            None
        }
        _ => None, // Constructors don't match
    }
}

/// Check if two constructors match (for specialization purposes).
fn constructors_match(c1: &Constructor, c2: &Constructor) -> bool {
    match (c1, c2) {
        (Constructor::Wildcard, _) | (_, Constructor::Wildcard) => true,
        (Constructor::True, Constructor::True) => true,
        (Constructor::False, Constructor::False) => true,
        (Constructor::None, Constructor::None) => true,
        (Constructor::Single(v1), Constructor::Single(v2)) => v1 == v2,
        (Constructor::Tuple(n1), Constructor::Tuple(n2)) => n1 == n2,
        (Constructor::ListExact(n1), Constructor::ListExact(n2)) => n1 == n2,
        (Constructor::ListAtLeast(n1), Constructor::ListExact(n2)) => n2 >= n1,
        (Constructor::ListExact(n1), Constructor::ListAtLeast(n2)) => n1 >= n2,
        (Constructor::ListAtLeast(n1), Constructor::ListAtLeast(n2)) => true,
        (Constructor::Dict, Constructor::Dict) => true,
        (Constructor::Class(c1), Constructor::Class(c2)) => c1 == c2,
        (Constructor::Variant(e1, v1), Constructor::Variant(e2, v2)) => e1 == e2 && v1 == v2,
        _ => false,
    }
}

/// Information about whether all constructors of a type are covered.
#[derive(Clone, Debug)]
enum ConstructorSet {
    /// All constructors are known and can be enumerated
    Finite(Vec<Constructor>),
    /// The type has infinitely many values (int, str)
    Infinite,
    /// Boolean type (exactly True and False)
    Bool,
    /// Optional type (Some(T) or None)
    Optional,
    /// List type (can have any length)
    List,
    /// Unknown type
    Unknown,
}

impl ConstructorSet {
    /// Get all constructors for this type that are NOT covered by the given constructors.
    fn missing_constructors(&self, covered: &[Constructor]) -> Vec<Constructor> {
        match self {
            ConstructorSet::Bool => {
                let has_true = covered.iter().any(|c| matches!(c, Constructor::True));
                let has_false = covered.iter().any(|c| matches!(c, Constructor::False));
                let mut missing = Vec::new();
                if !has_true { missing.push(Constructor::True); }
                if !has_false { missing.push(Constructor::False); }
                missing
            }
            ConstructorSet::Optional => {
                let has_none = covered.iter().any(|c| matches!(c, Constructor::None));
                // We can't know about Some without type info
                let mut missing = Vec::new();
                if !has_none { missing.push(Constructor::None); }
                missing
            }
            ConstructorSet::Finite(all) => {
                all.iter()
                    .filter(|c| !covered.iter().any(|cov| constructors_match(c, cov)))
                    .cloned()
                    .collect()
            }
            ConstructorSet::List => {
                // Check for ListExact patterns - need wildcard or ListAtLeast(0)
                let has_wildcard_list = covered.iter().any(|c| matches!(c, Constructor::ListAtLeast(0)));
                if has_wildcard_list {
                    Vec::new()
                } else {
                    // Find what lengths are covered
                    let covered_lens: FxHashSet<usize> = covered.iter()
                        .filter_map(|c| match c {
                            Constructor::ListExact(n) => Some(*n),
                            _ => None,
                        })
                        .collect();
                    
                    // If there's a ListAtLeast, check if it covers remaining
                    let min_at_least = covered.iter()
                        .filter_map(|c| match c {
                            Constructor::ListAtLeast(n) => Some(*n),
                            _ => None,
                        })
                        .min();
                    
                    if let Some(min) = min_at_least {
                        // All lengths >= min are covered, check below
                        (0..min)
                            .filter(|n| !covered_lens.contains(n))
                            .map(|n| Constructor::ListExact(n))
                            .collect()
                    } else {
                        // Need a catch-all
                        vec![Constructor::Missing]
                    }
                }
            }
            ConstructorSet::Infinite | ConstructorSet::Unknown => {
                // Infinite types need a wildcard
                let has_wildcard = covered.iter().any(|c| c.is_wildcard());
                if has_wildcard {
                    Vec::new()
                } else {
                    vec![Constructor::Missing]
                }
            }
        }
    }

    /// Check if the covered constructors are exhaustive.
    fn is_exhaustive(&self, covered: &[Constructor]) -> bool {
        self.missing_constructors(covered).is_empty()
    }
}

/// Determine the constructor set for a type.
fn constructor_set_for_type(ty: &Type) -> ConstructorSet {
    match ty {
        Type::Bool => ConstructorSet::Bool,
        Type::Optional(_) => ConstructorSet::Optional,
        Type::List(_) | Type::Tuple(_) => ConstructorSet::List,
        Type::Int | Type::Float | Type::Str | Type::Bytes => ConstructorSet::Infinite,
        Type::Class(cls) => {
            // Would need class info to know variants
            ConstructorSet::Unknown
        }
        Type::Union(types) => {
            // For unions, we need to cover all variants
            let ctors: Vec<Constructor> = types.iter()
                .filter_map(|t| type_to_constructor(t))
                .collect();
            if ctors.is_empty() {
                ConstructorSet::Unknown
            } else {
                ConstructorSet::Finite(ctors)
            }
        }
        _ => ConstructorSet::Unknown,
    }
}

/// Convert a type to its constructor (for union types).
fn type_to_constructor(ty: &Type) -> Option<Constructor> {
    match ty {
        Type::NoneType => Some(Constructor::None),
        Type::Bool => None, // Bool has two constructors
        Type::Class(cls) => Some(Constructor::Class(cls.name.clone())),
        _ => None,
    }
}

/// Result of exhaustiveness checking.
#[derive(Debug, Clone)]
pub enum ExhaustivenessResult {
    /// All values are covered.
    Exhaustive,
    /// Some patterns are missing.
    NonExhaustive(Vec<WitnessPattern>),
    /// Some patterns are unreachable (redundant).
    Redundant(Vec<usize>),
}

/// A witness pattern showing a missing case.
#[derive(Debug, Clone)]
pub struct WitnessPattern {
    pub ctor: Constructor,
    pub fields: Vec<WitnessPattern>,
}

impl WitnessPattern {
    fn wildcard() -> Self {
        Self { ctor: Constructor::Wildcard, fields: Vec::new() }
    }

    fn from_ctor(ctor: Constructor, arity: usize) -> Self {
        Self {
            ctor,
            fields: (0..arity).map(|_| Self::wildcard()).collect(),
        }
    }
}

impl fmt::Display for WitnessPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.fields.is_empty() {
            write!(f, "{}", self.ctor)
        } else {
            write!(f, "{}(", self.ctor)?;
            for (i, field) in self.fields.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", field)?;
            }
            write!(f, ")")
        }
    }
}

/// Check if a pattern vector is useful against a matrix.
/// Returns Some(witness) if useful, None if not.
fn compute_usefulness(
    matrix: &PatMatrix,
    vector: &[DeconstructedPat],
    ty: &Type,
) -> Option<WitnessPattern> {
    // Base case: empty matrix means the vector is useful
    if matrix.is_empty() {
        return Some(WitnessPattern::wildcard());
    }

    // Base case: empty vector with non-empty matrix means not useful
    if vector.is_empty() || matrix.num_columns == 0 {
        return None;
    }

    let first = &vector[0];
    let rest = &vector[1..];

    match &first.ctor {
        Constructor::Wildcard => {
            // Collect constructors in the first column of the matrix
            let head_ctors = matrix.head_constructors();
            let ctor_set = constructor_set_for_type(ty);

            // Check if there are missing constructors
            let missing = ctor_set.missing_constructors(&head_ctors);

            if !missing.is_empty() {
                // There are missing constructors - the wildcard covers them
                // We can use any missing constructor as a witness
                let witness_ctor = missing.into_iter().next().unwrap();
                let arity = witness_ctor.arity();
                
                // Specialize matrix for this constructor
                let specialized = matrix.specialize(&witness_ctor);
                
                // Build witness for the rest
                let mut witness_fields: Vec<DeconstructedPat> = (0..arity)
                    .map(|_| DeconstructedPat::wildcard(first.span))
                    .collect();
                witness_fields.extend(rest.iter().cloned());
                
                if let Some(sub_witness) = compute_usefulness(&specialized, &witness_fields, ty) {
                    return Some(WitnessPattern::from_ctor(witness_ctor, arity));
                }
            }

            // Check if wildcard is useful against any of the head constructors
            for ctor in &head_ctors {
                let specialized = matrix.specialize(ctor);
                let arity = ctor.arity();
                
                let mut witness_fields: Vec<DeconstructedPat> = (0..arity)
                    .map(|_| DeconstructedPat::wildcard(first.span))
                    .collect();
                witness_fields.extend(rest.iter().cloned());
                
                if let Some(sub_witness) = compute_usefulness(&specialized, &witness_fields, ty) {
                    return Some(WitnessPattern::from_ctor(ctor.clone(), arity));
                }
            }

            None
        }
        ctor => {
            // Non-wildcard constructor - specialize and recurse
            let specialized = matrix.specialize(ctor);
            let arity = ctor.arity();
            
            let mut new_vector: Vec<DeconstructedPat> = first.fields.clone();
            while new_vector.len() < arity {
                new_vector.push(DeconstructedPat::wildcard(first.span));
            }
            new_vector.extend(rest.iter().cloned());
            
            compute_usefulness(&specialized, &new_vector, ty)
        }
    }
}

/// Check if the given patterns are exhaustive for the given type.
pub fn check_exhaustiveness(ty: &Type, patterns: &[&Pattern]) -> ExhaustivenessResult {
    // Convert AST patterns to deconstructed patterns
    let mut matrix = PatMatrix::new(1);
    let mut has_guard = false;

    for (i, pattern) in patterns.iter().enumerate() {
        // Check for guards in the match statement (simplified - patterns don't carry guard info)
        // We assume no guards for now, but guards with fallback wildcards are handled
        
        // Flatten OR patterns
        let deconstructed = DeconstructedPat::from_ast(pattern);
        for pat in deconstructed {
            matrix.push(PatternRow {
                pats: vec![pat],
                arm_idx: i,
                guard: false,
            });
        }
    }

    // Check usefulness of wildcard against the matrix
    let wildcard = vec![DeconstructedPat::wildcard(Span::default())];
    
    if let Some(witness) = compute_usefulness(&matrix, &wildcard, ty) {
        // Match is not exhaustive
        ExhaustivenessResult::NonExhaustive(vec![witness])
    } else {
        ExhaustivenessResult::Exhaustive
    }
}

/// Check for redundant patterns (patterns that are never matched).
pub fn check_redundancy(ty: &Type, patterns: &[&Pattern]) -> Vec<usize> {
    let mut redundant = Vec::new();
    let mut matrix = PatMatrix::new(1);

    for (i, pattern) in patterns.iter().enumerate() {
        let deconstructed = DeconstructedPat::from_ast(pattern);
        
        // Check if this pattern is useful against current matrix
        let first = deconstructed.first();
        if let Some(first_pat) = first {
            let first_vec = vec![first_pat.clone()];
            let is_useful = compute_usefulness(&matrix, &first_vec, ty).is_some();
            
            if !is_useful {
                redundant.push(i);
            }
        }
        
        // Add all variations to matrix
        for pat in deconstructed {
            matrix.push(PatternRow {
                pats: vec![pat],
                arm_idx: i,
                guard: false,
            });
        }
    }

    redundant
}

/// Combined check that returns both exhaustiveness and redundancy information.
pub fn analyze_patterns(ty: &Type, patterns: &[&Pattern]) -> MatchAnalysis {
    let exhaustiveness = check_exhaustiveness(ty, patterns);
    let redundant = check_redundancy(ty, patterns);
    
    MatchAnalysis {
        exhaustiveness,
        redundant_arms: redundant,
    }
}

/// Complete analysis result for a match expression.
#[derive(Debug)]
pub struct MatchAnalysis {
    pub exhaustiveness: ExhaustivenessResult,
    pub redundant_arms: Vec<usize>,
}

impl MatchAnalysis {
    /// Returns true if the match is well-formed (exhaustive with no redundant arms).
    pub fn is_well_formed(&self) -> bool {
        matches!(self.exhaustiveness, ExhaustivenessResult::Exhaustive) 
            && self.redundant_arms.is_empty()
    }

    /// Generate diagnostic messages.
    pub fn diagnostics(&self) -> Vec<String> {
        let mut msgs = Vec::new();
        
        match &self.exhaustiveness {
            ExhaustivenessResult::Exhaustive => {}
            ExhaustivenessResult::NonExhaustive(witnesses) => {
                let witness_strs: Vec<_> = witnesses.iter()
                    .map(|w| w.to_string())
                    .collect();
                msgs.push(format!(
                    "non-exhaustive patterns: {} not covered",
                    witness_strs.join(", ")
                ));
            }
            ExhaustivenessResult::Redundant(indices) => {
                for idx in indices {
                    msgs.push(format!("arm {} is redundant", idx + 1));
                }
            }
        }
        
        for idx in &self.redundant_arms {
            msgs.push(format!("warning: arm {} is unreachable", idx + 1));
        }
        
        msgs
    }
}
