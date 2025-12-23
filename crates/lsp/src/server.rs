//! LSP server implementation.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use roast_common::{DiagnosticSink, Interner, Span};
use roast_parser::parse_module;

use crate::analysis::{Analyzer, DocumentAnalysis, SymbolKind, SymbolInfo, builtin_completions, resolve_module_path};

/// Workspace-wide module index for cross-file navigation.
#[derive(Default)]
struct ModuleIndex {
    /// Map from module path (e.g., "mypackage.utils") to file path
    module_to_file: HashMap<String, PathBuf>,
    /// Map from file path to exported symbols (symbol name -> SymbolInfo)
    file_exports: HashMap<PathBuf, HashMap<String, (SymbolInfo, Span)>>,
    /// Workspace roots
    workspace_roots: Vec<PathBuf>,
}

impl ModuleIndex {
    fn new() -> Self {
        Self::default()
    }
    
    /// Index a file's exports.
    fn index_file(&mut self, path: &Path, analysis: &DocumentAnalysis) {
        let exports: HashMap<String, (SymbolInfo, Span)> = analysis.symbols.iter()
            .filter(|(name, info)| {
                // Export public symbols (not starting with _) at module level
                !name.starts_with('_') && info.parent.is_none()
            })
            .map(|(name, info)| (name.clone(), (info.clone(), info.definition)))
            .collect();
        
        self.file_exports.insert(path.to_path_buf(), exports);
        
        // Also register module path
        if let Some(module_path) = self.file_to_module_path(path) {
            self.module_to_file.insert(module_path, path.to_path_buf());
        }
    }
    
    /// Convert file path to module path.
    fn file_to_module_path(&self, path: &Path) -> Option<String> {
        for root in &self.workspace_roots {
            if let Ok(rel) = path.strip_prefix(root) {
                // Remove extension and convert path separators to dots
                let mut module_path = rel
                    .with_extension("")
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, ".");
                
                // Remove __init__ suffix for package roots
                if module_path.ends_with(".__init__") {
                    module_path = module_path[..module_path.len() - 9].to_string();
                }
                
                return Some(module_path);
            }
        }
        None
    }
    
    /// Add a workspace root.
    fn add_workspace_root(&mut self, root: PathBuf) {
        if !self.workspace_roots.contains(&root) {
            self.workspace_roots.push(root);
        }
    }
    
    /// Lookup a symbol across the workspace.
    fn lookup_symbol(&self, module_path: &str, symbol_name: &str) -> Option<(PathBuf, SymbolInfo)> {
        if let Some(file_path) = self.module_to_file.get(module_path) {
            if let Some(exports) = self.file_exports.get(file_path) {
                if let Some((info, _)) = exports.get(symbol_name) {
                    return Some((file_path.clone(), info.clone()));
                }
            }
        }
        None
    }
    
    /// Get all symbols exported by a module.
    fn get_module_exports(&self, module_path: &str) -> Vec<(String, SymbolInfo)> {
        if let Some(file_path) = self.module_to_file.get(module_path) {
            if let Some(exports) = self.file_exports.get(file_path) {
                return exports.iter()
                    .map(|(name, (info, _))| (name.clone(), info.clone()))
                    .collect();
            }
        }
        Vec::new()
    }
}

/// The Roast language server.
pub struct RoastLanguageServer {
    client: Client,
    documents: DashMap<Url, DocumentState>,
    interner: Arc<Interner>,
    /// Workspace-wide module index for cross-file navigation.
    module_index: std::sync::RwLock<ModuleIndex>,
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
            module_index: std::sync::RwLock::new(ModuleIndex::new()),
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
        
        // Index file exports for cross-module navigation
        if let Some(ref analysis) = analysis {
            if let Ok(file_path) = uri.to_file_path() {
                if let Ok(mut index) = self.module_index.write() {
                    index.index_file(&file_path, analysis);
                }
            }
        }
        
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
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Set up workspace roots for cross-module navigation
        if let Ok(mut index) = self.module_index.write() {
            // Add workspace folders
            if let Some(folders) = params.workspace_folders {
                for folder in folders {
                    if let Ok(path) = folder.uri.to_file_path() {
                        index.add_workspace_root(path);
                    }
                }
            }
            // Fallback to root_uri
            if let Some(ref root_uri) = params.root_uri {
                if let Ok(path) = root_uri.to_file_path() {
                    index.add_workspace_root(path);
                }
            }
        }
        
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
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    ..Default::default()
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: vec![
                                SemanticTokenType::NAMESPACE,
                                SemanticTokenType::TYPE,
                                SemanticTokenType::CLASS,
                                SemanticTokenType::ENUM,
                                SemanticTokenType::INTERFACE,
                                SemanticTokenType::STRUCT,
                                SemanticTokenType::TYPE_PARAMETER,
                                SemanticTokenType::PARAMETER,
                                SemanticTokenType::VARIABLE,
                                SemanticTokenType::PROPERTY,
                                SemanticTokenType::ENUM_MEMBER,
                                SemanticTokenType::FUNCTION,
                                SemanticTokenType::METHOD,
                                SemanticTokenType::MACRO,
                                SemanticTokenType::KEYWORD,
                                SemanticTokenType::COMMENT,
                                SemanticTokenType::STRING,
                                SemanticTokenType::NUMBER,
                                SemanticTokenType::OPERATOR,
                                SemanticTokenType::DECORATOR,
                            ],
                            token_modifiers: vec![
                                SemanticTokenModifier::DECLARATION,
                                SemanticTokenModifier::DEFINITION,
                                SemanticTokenModifier::READONLY,
                                SemanticTokenModifier::ASYNC,
                            ],
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: Some(false),
                        ..Default::default()
                    })
                ),
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
        
        // First, find the word at the cursor position
        let word = if let Some(doc) = self.documents.get(&uri) {
            if let Some((w, _)) = self.word_at_position(&doc.source, position) {
                w
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };
        
        // Search across ALL open documents for references
        for entry in self.documents.iter() {
            let doc_uri = entry.key();
            let doc = entry.value();
            for (line_idx, line) in doc.source.lines().enumerate() {
                let mut col = 0;
                while let Some(idx) = line[col..].find(&word) {
                    let start_col = col + idx;
                    let end_col = start_col + word.len();
                    
                    // Check if it's a whole word (not part of larger identifier)
                    let is_start = start_col == 0 || 
                        !line.chars().nth(start_col - 1).map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
                    let is_end = end_col >= line.len() ||
                        !line.chars().nth(end_col).map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
                    
                    if is_start && is_end {
                        locations.push(Location {
                            uri: doc_uri.clone(),
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
        
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let diagnostics = &params.context.diagnostics;
        
        let mut actions = Vec::new();
        
        // Generate quick fixes based on diagnostics
        for diag in diagnostics {
            let message = &diag.message;
            
            // Fix: "undefined name" -> suggest import or define
            if message.contains("undefined") {
                if let Some(name) = message.split('\'').nth(1) {
                    // Suggest adding an import at the top of the file
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Add import for '{}'", name),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        edit: Some(WorkspaceEdit {
                            changes: Some({
                                let mut changes = std::collections::HashMap::new();
                                changes.insert(uri.clone(), vec![TextEdit {
                                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                                    new_text: format!("from module import {}\n", name),
                                }]);
                                changes
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
            
            // Fix: "unused import" -> remove line
            if message.contains("unused import") || message.contains("imported but unused") {
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: "Remove unused import".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some({
                            let mut changes = std::collections::HashMap::new();
                            changes.insert(uri.clone(), vec![TextEdit {
                                range: Range::new(
                                    Position::new(diag.range.start.line, 0),
                                    Position::new(diag.range.end.line + 1, 0),
                                ),
                                new_text: String::new(),
                            }]);
                            changes
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
            
            // Fix: missing colon in function/class def
            if message.contains("expected ':'") {
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: "Add missing colon".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some({
                            let mut changes = std::collections::HashMap::new();
                            changes.insert(uri.clone(), vec![TextEdit {
                                range: Range::new(diag.range.end, diag.range.end),
                                new_text: ":".to_string(),
                            }]);
                            changes
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
        }
        
        // Add general code actions (not diagnostic-based)
        if let Some(doc) = self.documents.get(&uri) {
            // Extract function/wrap in try-except
            if let Some((word, word_range)) = self.word_at_position(&doc.source, range.start) {
                // Add "Extract to variable" action
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Extract '{}' to variable", word),
                    kind: Some(CodeActionKind::REFACTOR_EXTRACT),
                    edit: Some(WorkspaceEdit {
                        changes: Some({
                            let mut changes = std::collections::HashMap::new();
                            changes.insert(uri.clone(), vec![
                                TextEdit {
                                    range: Range::new(
                                        Position::new(word_range.start.line, 0),
                                        Position::new(word_range.start.line, 0),
                                    ),
                                    new_text: format!("extracted_var = {}\n", word),
                                },
                                TextEdit {
                                    range: word_range,
                                    new_text: "extracted_var".to_string(),
                                },
                            ]);
                            changes
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
        }
        
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
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

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_lowercase();
        let mut results = Vec::new();
        
        // Search across all open documents
        for entry in self.documents.iter() {
            let uri = entry.key();
            if let Some(ref analysis) = entry.value().analysis {
                for (name, info) in &analysis.symbols {
                    // Only include top-level symbols that match the query
                    if info.parent.is_none() && name.to_lowercase().contains(&query) {
                        #[allow(deprecated)]
                        results.push(SymbolInformation {
                            name: name.clone(),
                            kind: info.kind.to_lsp(),
                            tags: None,
                            deprecated: None,
                            location: Location {
                                uri: uri.clone(),
                                range: analysis.span_to_range(info.definition),
                            },
                            container_name: None,
                        });
                    }
                }
            }
        }
        
        // Also search the module index for symbols not in open documents
        if let Ok(index) = self.module_index.read() {
            for (file_path, exports) in &index.file_exports {
                // Skip files already in open documents
                if let Ok(uri) = Url::from_file_path(file_path) {
                    if self.documents.contains_key(&uri) {
                        continue;
                    }
                    
                    for (name, (info, _)) in exports {
                        if name.to_lowercase().contains(&query) {
                            #[allow(deprecated)]
                            results.push(SymbolInformation {
                                name: name.clone(),
                                kind: info.kind.to_lsp(),
                                tags: None,
                                deprecated: None,
                                location: Location {
                                    uri: uri.clone(),
                                    range: Range::default(), // Would need to re-analyze for exact range
                                },
                                container_name: None,
                            });
                        }
                    }
                }
            }
        }
        
        if results.is_empty() {
            Ok(None)
        } else {
            Ok(Some(results))
        }
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

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        
        if let Some(doc) = self.documents.get(&uri) {
            let mut tokens = Vec::new();
            let mut prev_line = 0u32;
            let mut prev_col = 0u32;
            
            // Simple keyword-based semantic analysis
            let keywords = [
                "def", "class", "if", "else", "elif", "for", "while", "return", "import",
                "from", "as", "try", "except", "finally", "raise", "with", "async", "await",
                "pass", "break", "continue", "and", "or", "not", "in", "is", "lambda",
                "True", "False", "None", "self", "nonlocal", "global", "yield", "assert",
            ];
            
            for (line_idx, line) in doc.source.lines().enumerate() {
                let line_u32 = line_idx as u32;
                
                // Find comments
                if let Some(comment_start) = line.find('#') {
                    let delta_line = line_u32 - prev_line;
                    let delta_col = if delta_line == 0 { comment_start as u32 - prev_col } else { comment_start as u32 };
                    tokens.push(SemanticToken {
                        delta_line,
                        delta_start: delta_col,
                        length: (line.len() - comment_start) as u32,
                        token_type: 15, // COMMENT
                        token_modifiers_bitset: 0,
                    });
                    prev_line = line_u32;
                    prev_col = comment_start as u32;
                }
                
                // Find strings
                for (i, _) in line.match_indices('"') {
                    if let Some(end) = line[i+1..].find('"') {
                        let delta_line = line_u32 - prev_line;
                        let delta_col = if delta_line == 0 { i as u32 - prev_col } else { i as u32 };
                        tokens.push(SemanticToken {
                            delta_line,
                            delta_start: delta_col,
                            length: (end + 2) as u32,
                            token_type: 16, // STRING
                            token_modifiers_bitset: 0,
                        });
                        prev_line = line_u32;
                        prev_col = i as u32;
                        break; // Simple string handling
                    }
                }
                
                // Find keywords
                for keyword in &keywords {
                    for (i, _) in line.match_indices(keyword) {
                        // Check word boundaries
                        let before_ok = i == 0 || !line.chars().nth(i - 1).map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
                        let after_pos = i + keyword.len();
                        let after_ok = after_pos >= line.len() || !line.chars().nth(after_pos).map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
                        
                        if before_ok && after_ok {
                            let delta_line = line_u32 - prev_line;
                            let delta_col = if delta_line == 0 { i as u32 - prev_col } else { i as u32 };
                            tokens.push(SemanticToken {
                                delta_line,
                                delta_start: delta_col,
                                length: keyword.len() as u32,
                                token_type: 14, // KEYWORD
                                token_modifiers_bitset: 0,
                            });
                            prev_line = line_u32;
                            prev_col = i as u32;
                        }
                    }
                }
                
                // Find decorators (@something)
                if line.trim_start().starts_with('@') {
                    if let Some(at_pos) = line.find('@') {
                        let word_end = line[at_pos+1..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - at_pos - 1);
                        let delta_line = line_u32 - prev_line;
                        let delta_col = if delta_line == 0 { at_pos as u32 - prev_col } else { at_pos as u32 };
                        tokens.push(SemanticToken {
                            delta_line,
                            delta_start: delta_col,
                            length: (word_end + 1) as u32,
                            token_type: 19, // DECORATOR
                            token_modifiers_bitset: 0,
                        });
                        prev_line = line_u32;
                        prev_col = at_pos as u32;
                    }
                }
            }
            
            // Sort tokens by position and compute proper deltas
            tokens.sort_by(|a, b| {
                (a.delta_line, a.delta_start).cmp(&(b.delta_line, b.delta_start))
            });
            
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })));
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
