//! LSP server implementation.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use std::sync::Arc;

use roast_common::{DiagnosticSink, Interner, Span};
use roast_parser::parse_module;

use crate::analysis::{Analyzer, DocumentAnalysis, SymbolKind, builtin_completions};

/// The Roast language server.
pub struct RoastLanguageServer {
    client: Client,
    documents: DashMap<Url, DocumentState>,
    interner: Arc<Interner>,
}

/// State for a single document.
struct DocumentState {
    source: String,
    analysis: Option<DocumentAnalysis>,
}

impl RoastLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            interner: Arc::new(Interner::new()),
        }
    }

    /// Analyze and publish diagnostics.
    async fn analyze_document(&self, uri: Url, source: &str) {
        // Use with_source to enable # type: ignore support
        let mut diag_sink = DiagnosticSink::with_source(source);
        
        // Parse the module
        let parse_result = parse_module(source, uri.path(), &self.interner, &mut diag_sink);
        
        // Store analysis
        let analysis = Analyzer::new(&self.interner)
            .analyze(source, uri.path());
        
        self.documents.insert(uri.clone(), DocumentState {
            source: source.to_string(),
            analysis,
        });

        // Convert and publish diagnostics
        let diagnostics = self.convert_diagnostics(&diag_sink, source);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
    
    fn convert_diagnostics(&self, sink: &DiagnosticSink, source: &str) -> Vec<Diagnostic> {
        // Calculate line offsets
        let line_offsets: Vec<u32> = std::iter::once(0)
            .chain(source.chars().enumerate()
                .filter(|(_, c)| *c == '\n')
                .map(|(i, _)| (i + 1) as u32))
            .collect();
        
        sink.diagnostics()
            .iter()
            .filter_map(|d| {
                d.primary_span.map(|span| {
                    let range = self.span_to_range(span, &line_offsets);
                    Diagnostic {
                        range,
                        severity: Some(match d.kind {
                            roast_common::DiagnosticKind::Error => DiagnosticSeverity::ERROR,
                            roast_common::DiagnosticKind::Warning => DiagnosticSeverity::WARNING,
                            roast_common::DiagnosticKind::Note => DiagnosticSeverity::INFORMATION,
                            roast_common::DiagnosticKind::Help => DiagnosticSeverity::HINT,
                        }),
                        message: d.message.clone(),
                        source: Some("roast".to_string()),
                        ..Default::default()
                    }
                })
            })
            .collect()
    }
    
    fn span_to_range(&self, span: Span, line_offsets: &[u32]) -> Range {
        let start = self.offset_to_position(span.start, line_offsets);
        let end = self.offset_to_position(span.end, line_offsets);
        Range::new(start, end)
    }
    
    fn offset_to_position(&self, offset: u32, line_offsets: &[u32]) -> Position {
        let mut line = 0u32;
        let mut col = offset;
        
        for (i, &line_start) in line_offsets.iter().enumerate() {
            if offset >= line_start {
                line = i as u32;
                col = offset - line_start;
            } else {
                break;
            }
        }
        
        Position::new(line, col)
    }
    
    /// Get word at position.
    fn word_at_position(&self, source: &str, position: Position) -> Option<(String, Range)> {
        let lines: Vec<&str> = source.lines().collect();
        let line_idx = position.line as usize;
        
        if line_idx >= lines.len() {
            return None;
        }
        
        let line = lines[line_idx];
        let col = position.character as usize;
        
        if col > line.len() {
            return None;
        }
        
        // Find word boundaries
        let chars: Vec<char> = line.chars().collect();
        let mut start = col;
        let mut end = col;
        
        // Go back to start of word
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }
        
        // Go forward to end of word
        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        
        if start == end {
            return None;
        }
        
        let word: String = chars[start..end].iter().collect();
        let range = Range::new(
            Position::new(position.line, start as u32),
            Position::new(position.line, end as u32),
        );
        
        Some((word, range))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RoastLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        "@".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    ..Default::default()
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "roast-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Roast language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.analyze_document(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            self.analyze_document(uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        let mut completions = Vec::new();
        
        // Add builtin completions
        for (name, detail, kind) in builtin_completions() {
            completions.push(CompletionItem {
                label: name.clone(),
                kind: Some(kind.to_completion_kind()),
                detail: Some(detail),
                insert_text: if name.starts_with('@') {
                    Some(name[1..].to_string())
                } else {
                    Some(name)
                },
                ..Default::default()
            });
        }
        
        // Add symbols from the current document
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref analysis) = doc.analysis {
                for (name, info) in &analysis.symbols {
                    completions.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(info.kind.to_completion_kind()),
                        detail: info.doc.clone().or_else(|| Some(format!("{:?}", info.kind))),
                        documentation: info.doc.as_ref().map(|d| {
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: d.clone(),
                            })
                        }),
                        ..Default::default()
                    });
                }
            }
        }
        
        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri) {
            // Get word at position
            if let Some((word, range)) = self.word_at_position(&doc.source, position) {
                // Look up in analysis
                if let Some(ref analysis) = doc.analysis {
                    if let Some(info) = analysis.symbols.get(&word) {
                        let kind_str = format!("{:?}", info.kind);
                        let mut markdown = format!("**{}** _{}_", word, kind_str.to_lowercase());
                        
                        if let Some(ref doc) = info.doc {
                            markdown.push_str("\n\n---\n\n");
                            markdown.push_str(doc);
                        }
                        
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: markdown,
                            }),
                            range: Some(range),
                        }));
                    }
                }
                
                // Check builtins
                for (name, detail, kind) in builtin_completions() {
                    if name == word || name == format!("@{}", word) {
                        let kind_str = format!("{:?}", kind);
                        let markdown = format!(
                            "**{}** _{}_\n\n{}",
                            word,
                            kind_str.to_lowercase(),
                            detail
                        );
                        
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: markdown,
                            }),
                            range: Some(range),
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        if let Some(doc) = self.documents.get(&uri) {
            if let Some((word, _)) = self.word_at_position(&doc.source, position) {
                if let Some(ref analysis) = doc.analysis {
                    // First, check if it's a locally defined symbol
                    if let Some(info) = analysis.symbols.get(&word) {
                        let range = analysis.span_to_range(info.definition);
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: uri.clone(),
                            range,
                        })));
                    }
                    
                    // Try with parent prefix for methods (e.g., ClassName.method)
                    for (name, info) in &analysis.symbols {
                        if name.ends_with(&format!(".{}", word)) {
                            let range = analysis.span_to_range(info.definition);
                            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                uri: uri.clone(),
                                range,
                            })));
                        }
                    }
                    
                    // Check if it's an imported symbol - cross-module lookup
                    if let Some(imported) = analysis.get_imported_symbol(&word) {
                        // Resolve module path to file
                        if let Some(module_path) = crate::analysis::resolve_module_path(
                            &analysis.file_path,
                            &imported.module_path,
                        ) {
                            // Read and analyze the target module
                            if let Ok(source) = std::fs::read_to_string(&module_path) {
                                let target_analysis = crate::analysis::Analyzer::new(&self.interner)
                                    .analyze(&source, module_path.to_str().unwrap_or(""));
                                
                                if let Some(target) = target_analysis {
                                    // Find the symbol in the target module
                                    let target_name = if imported.original_name.is_empty() {
                                        &word // Module import
                                    } else {
                                        &imported.original_name
                                    };
                                    
                                    if let Some(target_info) = target.symbols.get(target_name) {
                                        let range = target.span_to_range(target_info.definition);
                                        let target_uri = Url::from_file_path(&module_path)
                                            .unwrap_or_else(|_| uri.clone());
                                        
                                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                            uri: target_uri,
                                            range,
                                        })));
                                    }
                                }
                            }
                        }
                        
                        // If we couldn't resolve, at least jump to the import statement
                        let range = analysis.span_to_range(imported.import_span);
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: uri.clone(),
                            range,
                        })));
                    }
                }
            }
        }
        
        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        let mut locations = Vec::new();
        
        if let Some(doc) = self.documents.get(&uri) {
            if let Some((word, _)) = self.word_at_position(&doc.source, position) {
                // Find all occurrences of the word
                for (line_idx, line) in doc.source.lines().enumerate() {
                    let mut col = 0;
                    while let Some(idx) = line[col..].find(&word) {
                        let start_col = col + idx;
                        let end_col = start_col + word.len();
                        
                        // Check if it's a whole word
                        let is_start = start_col == 0 || 
                            !line.chars().nth(start_col - 1).unwrap().is_alphanumeric();
                        let is_end = end_col >= line.len() ||
                            !line.chars().nth(end_col).unwrap().is_alphanumeric();
                        
                        if is_start && is_end {
                            locations.push(Location {
                                uri: uri.clone(),
                                range: Range::new(
                                    Position::new(line_idx as u32, start_col as u32),
                                    Position::new(line_idx as u32, end_col as u32),
                                ),
                            });
                        }
                        
                        col = end_col;
                    }
                }
            }
        }
        
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref analysis) = doc.analysis {
                let symbols: Vec<SymbolInformation> = analysis.symbols
                    .iter()
                    .filter(|(_, info)| info.parent.is_none()) // Top-level only
                    .map(|(name, info)| {
                        #[allow(deprecated)]
                        SymbolInformation {
                            name: name.clone(),
                            kind: info.kind.to_lsp(),
                            tags: None,
                            deprecated: None,
                            location: Location {
                                uri: uri.clone(),
                                range: analysis.span_to_range(info.definition),
                            },
                            container_name: info.parent.clone(),
                        }
                    })
                    .collect();
                
                return Ok(Some(DocumentSymbolResponse::Flat(symbols)));
            }
        }
        
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        
        if let Some(doc) = self.documents.get(&uri) {
            // Basic formatting: fix indentation
            let formatted = self.format_source(&doc.source);
            
            if formatted != doc.source {
                let lines = doc.source.lines().count();
                return Ok(Some(vec![TextEdit {
                    range: Range::new(
                        Position::new(0, 0),
                        Position::new(lines as u32, 0),
                    ),
                    new_text: formatted,
                }]));
            }
        }

        Ok(None)
    }
    
    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        if let Some(doc) = self.documents.get(&uri) {
            // Find the function being called
            let lines: Vec<&str> = doc.source.lines().collect();
            let line_idx = position.line as usize;
            
            if line_idx < lines.len() {
                let line = lines[line_idx];
                let col = position.character as usize;
                
                // Look backwards for function name
                if let Some(paren_idx) = line[..col].rfind('(') {
                    let prefix = &line[..paren_idx];
                    // Get word before paren
                    let word_start = prefix
                        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let func_name = &prefix[word_start..];
                    
                    if let Some(ref analysis) = doc.analysis {
                        if let Some(info) = analysis.symbols.get(func_name) {
                            if matches!(info.kind, SymbolKind::Function | SymbolKind::Method) {
                                // Count commas to determine active parameter
                                let after_paren = &line[paren_idx + 1..col];
                                let active_param = after_paren.matches(',').count();
                                
                                return Ok(Some(SignatureHelp {
                                    signatures: vec![SignatureInformation {
                                        label: format!("{}(...)", func_name),
                                        documentation: info.doc.as_ref().map(|d| {
                                            Documentation::MarkupContent(MarkupContent {
                                                kind: MarkupKind::Markdown,
                                                value: d.clone(),
                                            })
                                        }),
                                        parameters: None,
                                        active_parameter: Some(active_param as u32),
                                    }],
                                    active_signature: Some(0),
                                    active_parameter: Some(active_param as u32),
                                }));
                            }
                        }
                    }
                }
            }
        }
        
        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        
        if let Some(doc) = self.documents.get(&uri) {
            if let Some((old_name, _)) = self.word_at_position(&doc.source, position) {
                // Find all references and create edits
                let mut edits = Vec::new();
                
                for (line_idx, line) in doc.source.lines().enumerate() {
                    let mut col = 0;
                    while let Some(idx) = line[col..].find(&old_name) {
                        let start_col = col + idx;
                        let end_col = start_col + old_name.len();
                        
                        // Check if it's a whole word
                        let is_start = start_col == 0 || 
                            !line.chars().nth(start_col - 1).unwrap().is_alphanumeric();
                        let is_end = end_col >= line.len() ||
                            !line.chars().nth(end_col).unwrap().is_alphanumeric();
                        
                        if is_start && is_end {
                            edits.push(TextEdit {
                                range: Range::new(
                                    Position::new(line_idx as u32, start_col as u32),
                                    Position::new(line_idx as u32, end_col as u32),
                                ),
                                new_text: new_name.clone(),
                            });
                        }
                        
                        col = end_col;
                    }
                }
                
                if !edits.is_empty() {
                    let mut changes = std::collections::HashMap::new();
                    changes.insert(uri.clone(), edits);
                    
                    return Ok(Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let range = params.range;
        
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref analysis) = doc.analysis {
                let mut hints = Vec::new();
                
                for hint_info in &analysis.inlay_hints {
                    let pos = analysis.offset_to_position(hint_info.position.start);
                    
                    // Check if hint is in requested range
                    if pos.line >= range.start.line && pos.line <= range.end.line {
                        let kind = match hint_info.kind {
                            crate::analysis::InlayHintKind::Type => InlayHintKind::TYPE,
                            crate::analysis::InlayHintKind::Parameter => InlayHintKind::PARAMETER,
                            crate::analysis::InlayHintKind::ChainingHint => InlayHintKind::TYPE,
                        };
                        
                        hints.push(InlayHint {
                            position: pos,
                            label: InlayHintLabel::String(hint_info.label.clone()),
                            kind: Some(kind),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(hint_info.kind == crate::analysis::InlayHintKind::Parameter),
                            padding_right: Some(hint_info.kind == crate::analysis::InlayHintKind::Type),
                            data: None,
                        });
                    }
                }
                
                if !hints.is_empty() {
                    return Ok(Some(hints));
                }
            }
        }
        
        Ok(None)
    }
}

impl RoastLanguageServer {
    /// Basic source formatting.
    fn format_source(&self, source: &str) -> String {
        let mut result = String::new();
        let mut indent_level = 0i32;
        
        for line in source.lines() {
            let trimmed = line.trim();
            
            // Decrease indent for dedent keywords
            if trimmed.starts_with("elif ") || 
               trimmed.starts_with("else:") ||
               trimmed.starts_with("except") ||
               trimmed.starts_with("finally:") ||
               trimmed.starts_with("case ") {
                indent_level = (indent_level - 1).max(0);
            }
            
            // Check for outdent
            if trimmed == "pass" || 
               trimmed.starts_with("return") ||
               trimmed.starts_with("break") ||
               trimmed.starts_with("continue") ||
               trimmed.starts_with("raise") {
                // Don't change indent
            }
            
            // Add properly indented line
            if !trimmed.is_empty() {
                for _ in 0..indent_level {
                    result.push_str("    ");
                }
                result.push_str(trimmed);
            }
            result.push('\n');
            
            // Increase indent after block starters
            if trimmed.ends_with(':') && 
               (trimmed.starts_with("def ") ||
                trimmed.starts_with("class ") ||
                trimmed.starts_with("if ") ||
                trimmed.starts_with("elif ") ||
                trimmed.starts_with("else:") ||
                trimmed.starts_with("for ") ||
                trimmed.starts_with("while ") ||
                trimmed.starts_with("try:") ||
                trimmed.starts_with("except") ||
                trimmed.starts_with("finally:") ||
                trimmed.starts_with("with ") ||
                trimmed.starts_with("match ") ||
                trimmed.starts_with("case ")) {
                indent_level += 1;
            }
        }
        
        result
    }
}
