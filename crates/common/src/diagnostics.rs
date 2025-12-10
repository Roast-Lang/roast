//! Diagnostic reporting infrastructure.

use crate::span::Span;
use codespan_reporting::diagnostic::{self, Label, Severity};
use rustc_hash::FxHashSet;
use std::fmt;
use thiserror::Error;

/// Extracts line numbers (1-indexed) where `# type: ignore` comments appear.
/// 
/// Supports various formats:
/// - `# type: ignore` - ignore all errors on this line
/// - `# type: ignore[error-code]` - ignore specific error (for future use)
/// - `x = foo()  # type: ignore` - inline ignore
pub fn extract_type_ignore_lines(source: &str) -> FxHashSet<usize> {
    let mut ignored = FxHashSet::default();
    for (idx, line) in source.lines().enumerate() {
        // Check for # type: ignore anywhere in the line
        if line.contains("# type: ignore") || line.contains("#type:ignore") {
            ignored.insert(idx + 1); // 1-indexed
        }
    }
    ignored
}

/// The kind/severity of a diagnostic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Error,
    Warning,
    Note,
    Help,
}

impl DiagnosticKind {
    /// Converts to codespan-reporting Severity.
    pub fn to_severity(self) -> Severity {
        match self {
            DiagnosticKind::Error => Severity::Error,
            DiagnosticKind::Warning => Severity::Warning,
            DiagnosticKind::Note => Severity::Note,
            DiagnosticKind::Help => Severity::Help,
        }
    }
}

/// A compiler diagnostic with optional labels and notes.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub code: Option<String>,
    pub primary_span: Option<Span>,
    pub labels: Vec<DiagnosticLabel>,
    pub notes: Vec<String>,
}

/// A label attached to a diagnostic, pointing to a span.
#[derive(Clone, Debug)]
pub struct DiagnosticLabel {
    pub span: Span,
    pub message: String,
    pub is_primary: bool,
}

impl Diagnostic {
    /// Creates a new error diagnostic.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::Error,
            message: message.into(),
            code: None,
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Creates a new warning diagnostic.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::Warning,
            message: message.into(),
            code: None,
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Creates a new note diagnostic.
    pub fn note(message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::Note,
            message: message.into(),
            code: None,
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Sets the error code.
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Sets the primary span.
    pub fn with_span(mut self, span: Span) -> Self {
        self.primary_span = Some(span);
        self
    }

    /// Adds a primary label.
    pub fn with_primary_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.primary_span = Some(span);
        self.labels.push(DiagnosticLabel {
            span,
            message: message.into(),
            is_primary: true,
        });
        self
    }

    /// Adds a secondary label.
    pub fn with_secondary_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(DiagnosticLabel {
            span,
            message: message.into(),
            is_primary: false,
        });
        self
    }

    /// Adds a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Converts to a codespan-reporting Diagnostic.
    /// 
    /// The `file_id_offset` parameter adjusts the file ID to match the index
    /// used in SimpleFiles. Pass 0 if file IDs start at 0.
    pub fn to_codespan(&self) -> diagnostic::Diagnostic<usize> {
        self.to_codespan_with_offset(0)
    }

    /// Converts to a codespan-reporting Diagnostic with a file ID offset.
    /// 
    /// This is useful when the file ID in the Span doesn't match the index
    /// used in SimpleFiles.
    pub fn to_codespan_with_offset(&self, file_id_offset: i32) -> diagnostic::Diagnostic<usize> {
        let mut diag = diagnostic::Diagnostic::new(self.kind.to_severity())
            .with_message(&self.message);

        if let Some(ref code) = self.code {
            diag = diag.with_code(code);
        }

        // Collect explicit labels
        let mut labels: Vec<_> = self
            .labels
            .iter()
            .map(|l| {
                let file_id = (l.span.file.as_u32() as i32 + file_id_offset).max(0) as usize;
                let label = if l.is_primary {
                    Label::primary(file_id, l.span.start as usize..l.span.end as usize)
                } else {
                    Label::secondary(file_id, l.span.start as usize..l.span.end as usize)
                };
                label.with_message(&l.message)
            })
            .collect();

        // If no labels but we have a primary span, auto-generate a primary label
        // This enables source snippets even for simple diagnostics
        if labels.is_empty() {
            if let Some(span) = self.primary_span {
                if !span.is_dummy() {
                    let file_id = (span.file.as_u32() as i32 + file_id_offset).max(0) as usize;
                    labels.push(
                        Label::primary(file_id, span.start as usize..span.end as usize)
                            .with_message("here")
                    );
                }
            }
        }

        if !labels.is_empty() {
            diag = diag.with_labels(labels);
        }

        if !self.notes.is_empty() {
            diag = diag.with_notes(self.notes.clone());
        }

        diag
    }

    /// Returns true if this is an error.
    pub fn is_error(&self) -> bool {
        matches!(self.kind, DiagnosticKind::Error)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self.kind {
            DiagnosticKind::Error => "error",
            DiagnosticKind::Warning => "warning",
            DiagnosticKind::Note => "note",
            DiagnosticKind::Help => "help",
        };
        write!(f, "{}: {}", prefix, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// A sink for collecting diagnostics during compilation.
#[derive(Default)]
pub struct DiagnosticSink {
    diagnostics: Vec<Diagnostic>,
    error_count: usize,
    warning_count: usize,
    /// Lines where `# type: ignore` comments appear (1-indexed).
    ignored_lines: FxHashSet<usize>,
    /// Source code for line number calculation.
    source: Option<String>,
}

impl DiagnosticSink {
    /// Creates a new diagnostic sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new diagnostic sink with type ignore support.
    /// 
    /// Scans the source for `# type: ignore` comments and tracks those lines.
    pub fn with_source(source: &str) -> Self {
        let ignored_lines = extract_type_ignore_lines(source);
        Self {
            diagnostics: Vec::new(),
            error_count: 0,
            warning_count: 0,
            ignored_lines,
            source: Some(source.to_string()),
        }
    }

    /// Sets the ignored lines from source code.
    pub fn set_ignored_lines_from_source(&mut self, source: &str) {
        self.ignored_lines = extract_type_ignore_lines(source);
        self.source = Some(source.to_string());
    }

    /// Gets the line number (1-indexed) for a byte offset.
    fn offset_to_line(&self, offset: u32) -> usize {
        if let Some(ref source) = self.source {
            source[..offset.min(source.len() as u32) as usize]
                .matches('\n')
                .count() + 1
        } else {
            0
        }
    }

    /// Checks if a span is on an ignored line.
    fn is_ignored(&self, span: Option<Span>) -> bool {
        if self.ignored_lines.is_empty() {
            return false;
        }
        if let Some(span) = span {
            let line = self.offset_to_line(span.start);
            self.ignored_lines.contains(&line)
        } else {
            false
        }
    }

    /// Reports a diagnostic.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        // Check if this diagnostic should be ignored
        if self.is_ignored(diagnostic.primary_span) {
            return; // Silently ignore
        }

        match diagnostic.kind {
            DiagnosticKind::Error => self.error_count += 1,
            DiagnosticKind::Warning => self.warning_count += 1,
            _ => {}
        }
        self.diagnostics.push(diagnostic);
    }

    /// Reports an error.
    pub fn error(&mut self, message: impl Into<String>, span: Span) {
        self.report(Diagnostic::error(message).with_span(span));
    }

    /// Reports a warning.
    pub fn warning(&mut self, message: impl Into<String>, span: Span) {
        self.report(Diagnostic::warning(message).with_span(span));
    }

    /// Returns true if any errors have been reported.
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Returns the number of errors reported.
    pub fn error_count(&self) -> usize {
        self.error_count
    }

    /// Returns the number of warnings reported.
    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    /// Returns all collected diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Takes all diagnostics, leaving the sink empty.
    pub fn take(&mut self) -> Vec<Diagnostic> {
        self.error_count = 0;
        self.warning_count = 0;
        std::mem::take(&mut self.diagnostics)
    }

    /// Clears all diagnostics.
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.error_count = 0;
        self.warning_count = 0;
    }
}

/// Common error type for the Roast compiler.
#[derive(Error, Debug)]
pub enum RoastError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Type error: {0}")]
    Type(String),

    #[error("Compilation failed with {0} error(s)")]
    Compilation(usize),

    #[error("{0}")]
    Other(String),
}

impl RoastError {
    pub fn parse(msg: impl Into<String>) -> Self {
        RoastError::Parse(msg.into())
    }

    pub fn type_error(msg: impl Into<String>) -> Self {
        RoastError::Type(msg.into())
    }
}

