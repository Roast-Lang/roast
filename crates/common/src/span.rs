//! Source location and span types.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

/// Unique identifier for a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

impl FileId {
    /// Creates a new file ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying ID value.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// A span representing a range in the source code.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// The file this span belongs to.
    pub file: FileId,
    /// Start byte offset (inclusive).
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
}

impl Span {
    /// Creates a new span.
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "span start must be <= end");
        Self { file, start, end }
    }

    /// Creates a dummy span for generated code.
    pub fn dummy() -> Self {
        Self {
            file: FileId(u32::MAX),
            start: 0,
            end: 0,
        }
    }

    /// Returns true if this is a dummy span.
    pub fn is_dummy(&self) -> bool {
        self.file.0 == u32::MAX
    }

    /// Returns the length of this span in bytes.
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Returns true if this span is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merges two spans, taking the minimum start and maximum end.
    /// Panics if the spans are from different files.
    pub fn merge(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file, "cannot merge spans from different files");
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Creates a span that covers from self to other (inclusive).
    pub fn to(self, other: Span) -> Span {
        self.merge(other)
    }

    /// Extends this span to include another span.
    pub fn extend(&mut self, other: Span) {
        *self = self.merge(other);
    }

    /// Returns a zero-length span at the start of this span.
    pub fn shrink_to_start(self) -> Span {
        Span {
            file: self.file,
            start: self.start,
            end: self.start,
        }
    }

    /// Returns a zero-length span at the end of this span.
    pub fn shrink_to_end(self) -> Span {
        Span {
            file: self.file,
            start: self.end,
            end: self.end,
        }
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl Default for Span {
    fn default() -> Self {
        Self::dummy()
    }
}

/// A value with an associated source span.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Creates a new spanned value.
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }

    /// Creates a spanned value with a dummy span.
    pub fn dummy(node: T) -> Self {
        Self {
            node,
            span: Span::dummy(),
        }
    }

    /// Maps the inner value while preserving the span.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        Spanned {
            node: f(self.node),
            span: self.span,
        }
    }

    /// Returns a reference to the inner value.
    pub fn as_ref(&self) -> Spanned<&T> {
        Spanned {
            node: &self.node,
            span: self.span,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} @ {:?}", self.node, self.span)
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

/// Represents a source file loaded into the compiler.
#[derive(Clone, Debug)]
pub struct SourceFile {
    /// Unique identifier for this file.
    pub id: FileId,
    /// Path or name of the file.
    pub name: String,
    /// The source code.
    pub source: Arc<str>,
    /// Byte offsets of each line start.
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Creates a new source file.
    pub fn new(id: FileId, name: String, source: String) -> Self {
        let line_starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i as u32 + 1))
            .collect();

        Self {
            id,
            name,
            source: source.into(),
            line_starts,
        }
    }

    /// Returns the source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the line number (0-indexed) for a byte offset.
    pub fn line_index(&self, offset: u32) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next_line) => next_line.saturating_sub(1),
        }
    }

    /// Returns the column number (0-indexed) for a byte offset.
    pub fn column_index(&self, offset: u32) -> usize {
        let line = self.line_index(offset);
        let line_start = self.line_starts[line];
        (offset - line_start) as usize
    }

    /// Returns (line, column) for a byte offset (both 0-indexed).
    pub fn line_col(&self, offset: u32) -> (usize, usize) {
        let line = self.line_index(offset);
        let col = (offset - self.line_starts[line]) as usize;
        (line, col)
    }

    /// Returns the byte offset of the start of a line.
    pub fn line_start(&self, line: usize) -> Option<u32> {
        self.line_starts.get(line).copied()
    }

    /// Returns the number of lines in the file.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Extracts the text for a given span.
    pub fn span_text(&self, span: Span) -> &str {
        &self.source[span.start as usize..span.end as usize]
    }

    /// Returns the text of a specific line.
    pub fn line_text(&self, line: usize) -> Option<&str> {
        let start = *self.line_starts.get(line)? as usize;
        let end = self
            .line_starts
            .get(line + 1)
            .map(|&e| e as usize)
            .unwrap_or(self.source.len());
        Some(self.source[start..end].trim_end_matches(['\r', '\n']))
    }
}

