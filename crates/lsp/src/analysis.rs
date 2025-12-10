//! Code analysis for LSP features.
//!
//! Provides symbol indexing, type information, and code navigation.

use std::collections::HashMap;
use roast_common::{Span, Interner, DiagnosticSink, Symbol};
use roast_ast::*;
use roast_ast::{BinOp, UnaryOp};
use roast_typer::{Type, TypeContext, TypeChecker};
use roast_parser::parse_module;
use tower_lsp::lsp_types::{Position, Range, Location, Url};

/// Information for an inlay hint.
#[derive(Debug, Clone)]
pub struct InlayHintInfo {
    /// Position where the hint should appear.
    pub position: Span,
    /// The hint label (e.g., ": int").
    pub label: String,
    /// Kind of hint.
    pub kind: InlayHintKind,
}

/// Kind of inlay hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlayHintKind {
    /// Type annotation hint (e.g., `: int`).
    Type,
    /// Parameter name hint (e.g., `name =`).
    Parameter,
    /// Chained method hint.
    ChainingHint,
}

/// Represents a symbol in the codebase.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Symbol name.
    pub name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Type information.
    pub ty: Option<Type>,
    /// Definition location.
    pub definition: Span,
    /// Documentation.
    pub doc: Option<String>,
    /// Parent scope (for methods/nested items).
    pub parent: Option<String>,
}

/// Kind of symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Variable,
    Parameter,
    Field,
    Module,
    Constant,
    TypeAlias,
}

impl SymbolKind {
    pub fn to_lsp(self) -> tower_lsp::lsp_types::SymbolKind {
        use tower_lsp::lsp_types::SymbolKind as LspSymbolKind;
        match self {
            SymbolKind::Function => LspSymbolKind::FUNCTION,
            SymbolKind::Method => LspSymbolKind::METHOD,
            SymbolKind::Class => LspSymbolKind::CLASS,
            SymbolKind::Variable => LspSymbolKind::VARIABLE,
            SymbolKind::Parameter => LspSymbolKind::VARIABLE,
            SymbolKind::Field => LspSymbolKind::FIELD,
            SymbolKind::Module => LspSymbolKind::MODULE,
            SymbolKind::Constant => LspSymbolKind::CONSTANT,
            SymbolKind::TypeAlias => LspSymbolKind::TYPE_PARAMETER,
        }
    }
    
    pub fn to_completion_kind(self) -> tower_lsp::lsp_types::CompletionItemKind {
        use tower_lsp::lsp_types::CompletionItemKind;
        match self {
            SymbolKind::Function => CompletionItemKind::FUNCTION,
            SymbolKind::Method => CompletionItemKind::METHOD,
            SymbolKind::Class => CompletionItemKind::CLASS,
            SymbolKind::Variable => CompletionItemKind::VARIABLE,
            SymbolKind::Parameter => CompletionItemKind::VARIABLE,
            SymbolKind::Field => CompletionItemKind::FIELD,
            SymbolKind::Module => CompletionItemKind::MODULE,
            SymbolKind::Constant => CompletionItemKind::CONSTANT,
            SymbolKind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
        }
    }
}

/// Document analysis result.
#[derive(Debug, Default)]
pub struct DocumentAnalysis {
    /// All symbols in the document.
    pub symbols: HashMap<String, SymbolInfo>,
    /// Symbol at each position (for quick lookup).
    pub position_index: Vec<(Span, String)>,
    /// Import statements.
    pub imports: Vec<ImportInfo>,
    /// Imported symbol names mapped to their module.
    pub imported_symbols: HashMap<String, ImportedSymbol>,
    /// Line offsets for position calculation.
    pub line_offsets: Vec<u32>,
    /// Inlay hints for the document.
    pub inlay_hints: Vec<InlayHintInfo>,
    /// File path of this document.
    pub file_path: String,
}

/// Import information.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module: String,
    pub names: Vec<(String, Option<String>)>, // (name, alias)
    pub span: Span,
}

/// Information about an imported symbol.
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    /// The module path this symbol is imported from.
    pub module_path: String,
    /// The original name in the source module.
    pub original_name: String,
    /// The local alias (if any).
    pub alias: Option<String>,
    /// Span of the import statement.
    pub import_span: Span,
}

impl DocumentAnalysis {
    /// Find symbol at position.
    pub fn symbol_at(&self, position: Position) -> Option<&SymbolInfo> {
        let offset = self.position_to_offset(position)?;
        
        for (span, name) in &self.position_index {
            if offset >= span.start && offset < span.end {
                return self.symbols.get(name);
            }
        }
        None
    }
    
    /// Check if a name is an imported symbol.
    pub fn get_imported_symbol(&self, name: &str) -> Option<&ImportedSymbol> {
        self.imported_symbols.get(name)
    }
    
    /// Find all symbols in scope at position.
    pub fn symbols_in_scope(&self, _position: Position) -> Vec<&SymbolInfo> {
        // For now, return all symbols
        self.symbols.values().collect()
    }
    
    /// Convert position to byte offset.
    pub fn position_to_offset(&self, position: Position) -> Option<u32> {
        let line = position.line as usize;
        if line >= self.line_offsets.len() {
            return None;
        }
        
        Some(self.line_offsets[line] + position.character)
    }
    
    /// Convert byte offset to position.
    pub fn offset_to_position(&self, offset: u32) -> Position {
        let mut line = 0u32;
        let mut col = offset;
        
        for (i, &line_start) in self.line_offsets.iter().enumerate() {
            if offset >= line_start {
                line = i as u32;
                col = offset - line_start;
            } else {
                break;
            }
        }
        
        Position::new(line, col)
    }
    
    /// Convert span to range.
    pub fn span_to_range(&self, span: Span) -> Range {
        Range::new(
            self.offset_to_position(span.start),
            self.offset_to_position(span.end),
        )
    }
}

/// Resolve a module path to a file path.
/// 
/// This handles both relative and absolute imports:
/// - `from .sibling import foo` -> sibling.roast in same directory
/// - `from ..parent import foo` -> parent.roast in parent directory  
/// - `from mypackage.module import foo` -> mypackage/module.roast
pub fn resolve_module_path(current_file: &str, module_path: &str) -> Option<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    
    let current = Path::new(current_file);
    let current_dir = current.parent()?;
    
    // Handle relative imports (starting with .)
    if module_path.starts_with('.') {
        let mut path = current_dir.to_path_buf();
        let mut remaining = module_path;
        
        // Count leading dots for parent directory traversal
        while remaining.starts_with("..") {
            path = path.parent()?.to_path_buf();
            remaining = &remaining[1..]; // Skip one dot (keeping one for next iteration)
        }
        
        // Skip leading dot(s)
        remaining = remaining.trim_start_matches('.');
        
        // Convert remaining path
        if !remaining.is_empty() {
            for part in remaining.split('.') {
                path.push(part);
            }
        }
        
        // Try .roast extension first, then .py
        let roast_path = path.with_extension("roast");
        if roast_path.exists() {
            return Some(roast_path);
        }
        
        let py_path = path.with_extension("py");
        if py_path.exists() {
            return Some(py_path);
        }
        
        // Try as directory with __init__.roast
        path.push("__init__.roast");
        if path.exists() {
            return Some(path);
        }
        
        return None;
    }
    
    // Absolute import - look in various locations
    let parts: Vec<&str> = module_path.split('.').collect();
    
    // Try relative to current file's directory
    let mut path = current_dir.to_path_buf();
    for part in &parts {
        path.push(part);
    }
    
    // Try .roast extension
    let roast_path = path.with_extension("roast");
    if roast_path.exists() {
        return Some(roast_path);
    }
    
    // Try as directory with __init__.roast
    let init_path = path.join("__init__.roast");
    if init_path.exists() {
        return Some(init_path);
    }
    
    // Try workspace root (parent directories looking for roast.toml or pyproject.toml)
    let mut search_dir = current_dir;
    while let Some(parent) = search_dir.parent() {
        // Check for project root indicators
        if parent.join("roast.toml").exists() || parent.join("pyproject.toml").exists() {
            let mut project_path = parent.to_path_buf();
            for part in &parts {
                project_path.push(part);
            }
            
            let roast_path = project_path.with_extension("roast");
            if roast_path.exists() {
                return Some(roast_path);
            }
            
            let init_path = project_path.join("__init__.roast");
            if init_path.exists() {
                return Some(init_path);
            }
            
            break;
        }
        search_dir = parent;
    }
    
    None
}

/// Analyzes a document and extracts symbols.
pub struct Analyzer<'a> {
    interner: &'a Interner,
    analysis: DocumentAnalysis,
    current_class: Option<String>,
    current_function: Option<String>,
    /// Track inferred types for variables.
    inferred_types: HashMap<String, String>,
}

impl<'a> Analyzer<'a> {
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            analysis: DocumentAnalysis::default(),
            current_class: None,
            current_function: None,
            inferred_types: HashMap::new(),
        }
    }
    
    /// Analyze a source file.
    pub fn analyze(mut self, source: &str, uri: &str) -> Option<DocumentAnalysis> {
        // Store file path
        self.analysis.file_path = uri.to_string();
        
        // Compute line offsets
        self.analysis.line_offsets = vec![0];
        for (i, c) in source.chars().enumerate() {
            if c == '\n' {
                self.analysis.line_offsets.push((i + 1) as u32);
            }
        }
        
        // Parse the module - use with_source to enable # type: ignore support
        let mut diagnostics = DiagnosticSink::with_source(source);
        let module = parse_module(source, uri, self.interner, &mut diagnostics).ok()?;
        
        // Visit the AST
        self.visit_module(&module);
        
        Some(self.analysis)
    }
    
    fn visit_module(&mut self, module: &Module) {
        for stmt in &module.body {
            self.visit_stmt(stmt);
        }
    }
    
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::FunctionDef { name, args, returns, body, decorators, .. } => {
                let func_name = self.symbol_to_string(&name.name);
                let kind = if self.current_class.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                
                // Build type signature
                let type_str = self.build_function_signature(args, returns.as_deref());
                
                // Extract docstring
                let doc = self.extract_docstring(body);
                
                let info = SymbolInfo {
                    name: func_name.clone(),
                    kind,
                    ty: None, // Would need full type checking
                    definition: name.span,
                    doc,
                    parent: self.current_class.clone(),
                };
                
                self.analysis.symbols.insert(func_name.clone(), info);
                self.analysis.position_index.push((name.span, func_name.clone()));
                
                // Collect parameter type hints for parameters without annotations
                for arg in &args.args {
                    let param_name = self.symbol_to_string(&arg.name.name);
                    let info = SymbolInfo {
                        name: param_name.clone(),
                        kind: SymbolKind::Parameter,
                        ty: None,
                        definition: arg.name.span,
                        doc: None,
                        parent: Some(func_name.clone()),
                    };
                    self.analysis.symbols.insert(
                        format!("{}.{}", func_name, param_name),
                        info,
                    );
                }
                
                // Store the old function name and set new
                let old_function = self.current_function.take();
                self.current_function = Some(func_name.clone());
                
                // Visit body
                for s in body {
                    self.visit_stmt(s);
                }
                
                self.current_function = old_function;
            }
            
            StmtKind::ClassDef { name, body, .. } => {
                let class_name = self.symbol_to_string(&name.name);
                
                let doc = self.extract_docstring(body);
                
                let info = SymbolInfo {
                    name: class_name.clone(),
                    kind: SymbolKind::Class,
                    ty: None,
                    definition: name.span,
                    doc,
                    parent: None,
                };
                
                self.analysis.symbols.insert(class_name.clone(), info);
                self.analysis.position_index.push((name.span, class_name.clone()));
                
                // Visit class body
                let old_class = self.current_class.take();
                self.current_class = Some(class_name.clone());
                
                for s in body {
                    self.visit_stmt(s);
                }
                
                self.current_class = old_class;
            }
            
            StmtKind::Assign { targets, value, .. } => {
                // Infer type from value expression
                let inferred_type = self.infer_expr_type(value);
                
                for target in targets {
                    if let ExprKind::Name { id, .. } = &target.kind {
                        let var_name = self.symbol_to_string(&id.name);
                        
                        let info = SymbolInfo {
                            name: var_name.clone(),
                            kind: SymbolKind::Variable,
                            ty: None,
                            definition: id.span,
                            doc: None,
                            parent: self.current_class.clone(),
                        };
                        
                        self.analysis.symbols.insert(var_name.clone(), info);
                        self.analysis.position_index.push((id.span, var_name.clone()));
                        
                        // Add inlay hint for variable type
                        if let Some(ref ty) = inferred_type {
                            self.analysis.inlay_hints.push(InlayHintInfo {
                                position: Span::new(id.span.file, id.span.end, id.span.end),
                                label: format!(": {}", ty),
                                kind: InlayHintKind::Type,
                            });
                            self.inferred_types.insert(var_name, ty.clone());
                        }
                    }
                }
                
                // Visit expression for function call parameter hints
                self.visit_expr(value);
            }
            
            StmtKind::AnnAssign { target, .. } => {
                if let ExprKind::Name { id, .. } = &target.kind {
                    let var_name = self.symbol_to_string(&id.name);
                    
                    let info = SymbolInfo {
                        name: var_name.clone(),
                        kind: if self.current_class.is_some() {
                            SymbolKind::Field
                        } else {
                            SymbolKind::Variable
                        },
                        ty: None,
                        definition: id.span,
                        doc: None,
                        parent: self.current_class.clone(),
                    };
                    
                    self.analysis.symbols.insert(var_name.clone(), info);
                    self.analysis.position_index.push((id.span, var_name));
                }
            }
            
            StmtKind::Import { names } => {
                for alias in names {
                    let module_name = self.symbol_to_string(&alias.name.name);
                    self.analysis.imports.push(ImportInfo {
                        module: module_name.clone(),
                        names: vec![],
                        span: stmt.span,
                    });
                    
                    // Track the module itself as an imported symbol
                    let local_name = alias.asname.as_ref()
                        .map(|a| self.symbol_to_string(&a.name))
                        .unwrap_or_else(|| {
                            // For "import a.b.c", local name is "a"
                            module_name.split('.').next().unwrap_or(&module_name).to_string()
                        });
                    
                    self.analysis.imported_symbols.insert(local_name, ImportedSymbol {
                        module_path: module_name,
                        original_name: String::new(), // Module import, not specific symbol
                        alias: alias.asname.as_ref().map(|a| self.symbol_to_string(&a.name)),
                        import_span: stmt.span,
                    });
                }
            }
            
            StmtKind::ImportFrom { module, names, .. } => {
                let module_name = module
                    .as_ref()
                    .map(|m| self.symbol_to_string(&m.name))
                    .unwrap_or_default();
                
                let imported_names: Vec<_> = names.iter()
                    .map(|a| {
                        let name = self.symbol_to_string(&a.name.name);
                        let alias = a.asname.as_ref()
                            .map(|i| self.symbol_to_string(&i.name));
                        (name, alias)
                    })
                    .collect();
                
                // Track each imported name
                for (name, alias) in &imported_names {
                    let local_name = alias.as_ref().unwrap_or(name).clone();
                    self.analysis.imported_symbols.insert(local_name, ImportedSymbol {
                        module_path: module_name.clone(),
                        original_name: name.clone(),
                        alias: alias.clone(),
                        import_span: stmt.span,
                    });
                }
                
                self.analysis.imports.push(ImportInfo {
                    module: module_name,
                    names: imported_names,
                    span: stmt.span,
                });
            }
            
            StmtKind::TypeAlias { name, .. } => {
                let alias_name = self.symbol_to_string(&name.name);
                
                let info = SymbolInfo {
                    name: alias_name.clone(),
                    kind: SymbolKind::TypeAlias,
                    ty: None,
                    definition: name.span,
                    doc: None,
                    parent: None,
                };
                
                self.analysis.symbols.insert(alias_name.clone(), info);
                self.analysis.position_index.push((name.span, alias_name));
            }
            
            // Recurse into compound statements
            StmtKind::If { body, orelse, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
                for s in orelse {
                    self.visit_stmt(s);
                }
            }
            
            StmtKind::For { body, orelse, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
                for s in orelse {
                    self.visit_stmt(s);
                }
            }
            
            StmtKind::While { body, orelse, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
                for s in orelse {
                    self.visit_stmt(s);
                }
            }
            
            StmtKind::Try { body, handlers, orelse, finalbody } => {
                for s in body {
                    self.visit_stmt(s);
                }
                for handler in handlers {
                    for s in &handler.body {
                        self.visit_stmt(s);
                    }
                }
                for s in orelse {
                    self.visit_stmt(s);
                }
                for s in finalbody {
                    self.visit_stmt(s);
                }
            }
            
            StmtKind::With { body, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
            }
            
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    for s in &case.body {
                        self.visit_stmt(s);
                    }
                }
            }
            
            _ => {}
        }
    }
    
    fn symbol_to_string(&self, sym: &Symbol) -> String {
        // Get string from interner
        self.interner.resolve(*sym)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("<sym:{}>", sym.as_raw()))
    }
    
    fn build_function_signature(&self, args: &Arguments, returns: Option<&TypeExpr>) -> String {
        let mut sig = String::from("(");
        
        for (i, arg) in args.args.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            sig.push_str(&self.symbol_to_string(&arg.name.name));
            if let Some(ann) = &arg.annotation {
                sig.push_str(": ");
                sig.push_str(&self.type_expr_to_string(ann));
            }
        }
        
        sig.push(')');
        
        if let Some(ret) = returns {
            sig.push_str(" -> ");
            sig.push_str(&self.type_expr_to_string(ret));
        }
        
        sig
    }
    
    fn type_expr_to_string(&self, ty: &TypeExpr) -> String {
        match &ty.kind {
            TypeExprKind::Name { name } => self.symbol_to_string(&name.name),
            TypeExprKind::Subscript { value, slice } => {
                format!("{}[{}]", 
                    self.type_expr_to_string(value),
                    self.type_expr_to_string(slice))
            }
            TypeExprKind::Tuple { elts } => {
                let types: Vec<_> = elts.iter()
                    .map(|e| self.type_expr_to_string(e))
                    .collect();
                format!("({})", types.join(", "))
            }
            TypeExprKind::Union { types } => {
                let types: Vec<_> = types.iter()
                    .map(|e| self.type_expr_to_string(e))
                    .collect();
                types.join(" | ")
            }
            TypeExprKind::Optional { inner } => {
                format!("{}?", self.type_expr_to_string(inner))
            }
            TypeExprKind::None => "None".to_string(),
            TypeExprKind::Any => "Any".to_string(),
            _ => "...".to_string(),
        }
    }
    
    fn extract_docstring(&self, body: &[Stmt]) -> Option<String> {
        if let Some(first) = body.first() {
            if let StmtKind::Expr { value } = &first.kind {
                if let ExprKind::StringLit { value, .. } = &value.kind {
                    return Some(value.trim().to_string());
                }
            }
        }
        None
    }
    
    /// Infer the type of an expression for inlay hints.
    fn infer_expr_type(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            // Literals have known types
            ExprKind::IntLit { .. } => Some("int".to_string()),
            ExprKind::FloatLit { .. } => Some("float".to_string()),
            ExprKind::StringLit { .. } => Some("str".to_string()),
            ExprKind::BytesLit { .. } => Some("bytes".to_string()),
            ExprKind::BoolLit { .. } => Some("bool".to_string()),
            ExprKind::NoneLit => Some("None".to_string()),
            
            // List/Dict/Set/Tuple literals
            ExprKind::List { elts, .. } => {
                if let Some(first) = elts.first() {
                    if let Some(elem_ty) = self.infer_expr_type(first) {
                        return Some(format!("List[{}]", elem_ty));
                    }
                }
                Some("List[Any]".to_string())
            }
            ExprKind::Dict { keys, .. } => {
                if let (Some(Some(key)), Some(val)) = (keys.first(), keys.get(1)) {
                    if let (Some(k), Some(v)) = (self.infer_expr_type(key), val.as_ref().and_then(|v| self.infer_expr_type(v))) {
                        return Some(format!("Dict[{}, {}]", k, v));
                    }
                }
                Some("Dict[Any, Any]".to_string())
            }
            ExprKind::Set { elts } => {
                if let Some(first) = elts.first() {
                    if let Some(elem_ty) = self.infer_expr_type(first) {
                        return Some(format!("Set[{}]", elem_ty));
                    }
                }
                Some("Set[Any]".to_string())
            }
            ExprKind::Tuple { elts, .. } => {
                let types: Vec<String> = elts.iter()
                    .filter_map(|e| self.infer_expr_type(e))
                    .collect();
                if types.is_empty() {
                    Some("Tuple[()]".to_string())
                } else {
                    Some(format!("Tuple[{}]", types.join(", ")))
                }
            }
            
            // Variable reference - look up inferred type
            ExprKind::Name { id, .. } => {
                let name = self.symbol_to_string(&id.name);
                self.inferred_types.get(&name).cloned()
            }
            
            // Function call - try to find return type
            ExprKind::Call { func, .. } => {
                if let ExprKind::Name { id, .. } = &func.kind {
                    let func_name = self.symbol_to_string(&id.name);
                    // Check for common builtins
                    match func_name.as_str() {
                        "len" => return Some("int".to_string()),
                        "str" => return Some("str".to_string()),
                        "int" => return Some("int".to_string()),
                        "float" => return Some("float".to_string()),
                        "bool" => return Some("bool".to_string()),
                        "list" => return Some("List[Any]".to_string()),
                        "dict" => return Some("Dict[Any, Any]".to_string()),
                        "set" => return Some("Set[Any]".to_string()),
                        "tuple" => return Some("Tuple[Any, ...]".to_string()),
                        "range" => return Some("range".to_string()),
                        "enumerate" => return Some("enumerate".to_string()),
                        "zip" => return Some("zip".to_string()),
                        "sorted" => return Some("List[Any]".to_string()),
                        "reversed" => return Some("reversed".to_string()),
                        "input" => return Some("str".to_string()),
                        "open" => return Some("File".to_string()),
                        "sum" | "min" | "max" | "abs" => return Some("int | float".to_string()),
                        "round" => return Some("int".to_string()),
                        "type" => return Some("type".to_string()),
                        "isinstance" => return Some("bool".to_string()),
                        "hasattr" | "getattr" | "setattr" | "delattr" => return Some("Any".to_string()),
                        _ => {}
                    }
                }
                None
            }
            
            // Binary operations
            ExprKind::BinOp { left, op, right } => {
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mult | 
                    BinOp::Div | BinOp::Mod | BinOp::Pow |
                    BinOp::FloorDiv => {
                        let left_ty = self.infer_expr_type(left);
                        let right_ty = self.infer_expr_type(right);
                        match (left_ty.as_deref(), right_ty.as_deref()) {
                            (Some("float"), _) | (_, Some("float")) => Some("float".to_string()),
                            (Some("int"), Some("int")) => {
                                if matches!(op, BinOp::Div) {
                                    Some("float".to_string())
                                } else {
                                    Some("int".to_string())
                                }
                            }
                            (Some("str"), Some("str")) if matches!(op, BinOp::Add) => {
                                Some("str".to_string())
                            }
                            _ => None,
                        }
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor |
                    BinOp::LShift | BinOp::RShift => Some("int".to_string()),
                    BinOp::MatMult => None,
                }
            }
            
            // Comparison operations always return bool
            ExprKind::Compare { .. } => Some("bool".to_string()),
            
            // Boolean operations
            ExprKind::BoolOp { .. } => Some("bool".to_string()),
            
            // Unary operations
            ExprKind::UnaryOp { op, operand } => {
                match op {
                    UnaryOp::Not => Some("bool".to_string()),
                    UnaryOp::Invert => Some("int".to_string()),
                    UnaryOp::UAdd | UnaryOp::USub => self.infer_expr_type(operand),
                }
            }
            
            // Lambda - we don't know the return type easily
            ExprKind::Lambda { .. } => Some("Callable[..., Any]".to_string()),
            
            // Await - could infer from the inner type
            ExprKind::Await { .. } => None,
            
            // If expression
            ExprKind::IfExp { body, .. } => self.infer_expr_type(body),
            
            // Attribute access
            ExprKind::Attribute { attr, .. } => {
                // Common patterns
                let attr_name = self.symbol_to_string(&attr.name);
                match attr_name.as_str() {
                    "__len__" => Some("int".to_string()),
                    "__str__" | "__repr__" => Some("str".to_string()),
                    _ => None,
                }
            }
            
            // Subscript - would need proper type tracking
            ExprKind::Subscript { value, .. } => {
                // Try to infer element type from container
                if let Some(container_ty) = self.infer_expr_type(value) {
                    if container_ty.starts_with("List[") && container_ty.ends_with(']') {
                        let elem = &container_ty[5..container_ty.len()-1];
                        return Some(elem.to_string());
                    }
                    if container_ty == "str" {
                        return Some("str".to_string());
                    }
                }
                None
            }
            
            // Slice
            ExprKind::Slice { .. } => Some("slice".to_string()),
            
            // Starred
            ExprKind::Starred { value, .. } => self.infer_expr_type(value),
            
            // Walrus operator
            ExprKind::NamedExpr { value, .. } => self.infer_expr_type(value),
            
            // Generator/comprehension expressions
            ExprKind::ListComp { .. } => Some("List[Any]".to_string()),
            ExprKind::SetComp { .. } => Some("Set[Any]".to_string()),
            ExprKind::DictComp { .. } => Some("Dict[Any, Any]".to_string()),
            ExprKind::GeneratorExp { .. } => Some("Generator[Any, None, None]".to_string()),
            
            // F-string
            ExprKind::JoinedStr { .. } | ExprKind::FormattedValue { .. } => Some("str".to_string()),
            
            // Try expression
            ExprKind::Try { value } => {
                // The inner value should be Result[T, E] or Option[T], return T
                if let Some(inner_ty) = self.infer_expr_type(value) {
                    if inner_ty.starts_with("Result[") {
                        // Extract T from Result[T, E]
                        let inner = &inner_ty[7..];
                        if let Some(comma_idx) = inner.find(',') {
                            return Some(inner[..comma_idx].trim().to_string());
                        }
                    } else if inner_ty.starts_with("Option[") && inner_ty.ends_with(']') {
                        return Some(inner_ty[7..inner_ty.len()-1].to_string());
                    }
                }
                None
            }
            
            _ => None,
        }
    }
    
    /// Visit an expression to collect parameter name hints for function calls.
    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Call { func, args, keywords } => {
                // Try to get function info for parameter name hints
                if let ExprKind::Name { id, .. } = &func.kind {
                    let func_name = self.symbol_to_string(&id.name);
                    
                    // Look up function in symbols to get parameter names
                    if let Some(info) = self.analysis.symbols.get(&func_name).cloned() {
                        // For now, use heuristic-based parameter hints for known builtins
                        let param_names = self.get_builtin_params(&func_name);
                        
                        // Add parameter hints for positional arguments
                        for (i, arg) in args.iter().enumerate() {
                            if i < param_names.len() {
                                // Only add hint if arg is not a simple name matching param
                                let should_add = match &arg.kind {
                                    ExprKind::Name { id, .. } => {
                                        let arg_name = self.symbol_to_string(&id.name);
                                        arg_name != param_names[i]
                                    }
                                    _ => true,
                                };
                                
                                if should_add && !param_names[i].is_empty() {
                                    self.analysis.inlay_hints.push(InlayHintInfo {
                                        position: Span::new(arg.span.file, arg.span.start, arg.span.start),
                                        label: format!("{}:", param_names[i]),
                                        kind: InlayHintKind::Parameter,
                                    });
                                }
                            }
                            
                            // Recurse into argument
                            self.visit_expr(arg);
                        }
                    }
                }
                
                // Recurse into func and keyword arguments
                self.visit_expr(func);
                for arg in args {
                    self.visit_expr(arg);
                }
                for kw in keywords {
                    self.visit_expr(&kw.value);
                }
            }
            
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            
            ExprKind::UnaryOp { operand, .. } => {
                self.visit_expr(operand);
            }
            
            ExprKind::Compare { left, comparators, .. } => {
                self.visit_expr(left);
                for comp in comparators {
                    self.visit_expr(comp);
                }
            }
            
            ExprKind::BoolOp { values, .. } => {
                for val in values {
                    self.visit_expr(val);
                }
            }
            
            ExprKind::IfExp { test, body, orelse } => {
                self.visit_expr(test);
                self.visit_expr(body);
                self.visit_expr(orelse);
            }
            
            ExprKind::Lambda { body, .. } => {
                self.visit_expr(body);
            }
            
            ExprKind::Attribute { value, .. } => {
                self.visit_expr(value);
            }
            
            ExprKind::Subscript { value, slice, .. } => {
                self.visit_expr(value);
                self.visit_expr(slice);
            }
            
            ExprKind::List { elts, .. } | ExprKind::Set { elts } | ExprKind::Tuple { elts, .. } => {
                for elt in elts {
                    self.visit_expr(elt);
                }
            }
            
            ExprKind::Dict { keys, values } => {
                for key in keys.iter().flatten() {
                    self.visit_expr(key);
                }
                for val in values {
                    self.visit_expr(val);
                }
            }
            
            _ => {}
        }
    }
    
    /// Get parameter names for builtin functions.
    fn get_builtin_params(&self, func_name: &str) -> Vec<&'static str> {
        match func_name {
            "print" => vec!["*values", "sep", "end", "file", "flush"],
            "len" => vec!["obj"],
            "range" => vec!["start", "stop", "step"],
            "enumerate" => vec!["iterable", "start"],
            "zip" => vec!["*iterables"],
            "map" => vec!["func", "iterable"],
            "filter" => vec!["func", "iterable"],
            "sorted" => vec!["iterable", "key", "reverse"],
            "reversed" => vec!["sequence"],
            "sum" => vec!["iterable", "start"],
            "min" | "max" => vec!["*args", "key", "default"],
            "abs" => vec!["x"],
            "round" => vec!["number", "ndigits"],
            "isinstance" => vec!["obj", "classinfo"],
            "type" => vec!["obj"],
            "input" => vec!["prompt"],
            "open" => vec!["file", "mode", "encoding"],
            "getattr" => vec!["obj", "name", "default"],
            "setattr" => vec!["obj", "name", "value"],
            "hasattr" => vec!["obj", "name"],
            "delattr" => vec!["obj", "name"],
            _ => vec![],
        }
    }
}

/// Builtin completions.
pub fn builtin_completions() -> Vec<(String, String, SymbolKind)> {
    vec![
        // Keywords
        ("def".into(), "Define a function".into(), SymbolKind::Function),
        ("class".into(), "Define a class".into(), SymbolKind::Class),
        ("if".into(), "If statement".into(), SymbolKind::Variable),
        ("elif".into(), "Else if branch".into(), SymbolKind::Variable),
        ("else".into(), "Else branch".into(), SymbolKind::Variable),
        ("for".into(), "For loop".into(), SymbolKind::Variable),
        ("while".into(), "While loop".into(), SymbolKind::Variable),
        ("try".into(), "Try block".into(), SymbolKind::Variable),
        ("except".into(), "Exception handler".into(), SymbolKind::Variable),
        ("finally".into(), "Finally block".into(), SymbolKind::Variable),
        ("with".into(), "Context manager".into(), SymbolKind::Variable),
        ("match".into(), "Pattern matching".into(), SymbolKind::Variable),
        ("case".into(), "Match case".into(), SymbolKind::Variable),
        ("return".into(), "Return statement".into(), SymbolKind::Variable),
        ("yield".into(), "Yield expression".into(), SymbolKind::Variable),
        ("import".into(), "Import module".into(), SymbolKind::Module),
        ("from".into(), "From import".into(), SymbolKind::Module),
        ("async".into(), "Async function".into(), SymbolKind::Function),
        ("await".into(), "Await expression".into(), SymbolKind::Variable),
        ("raise".into(), "Raise exception".into(), SymbolKind::Variable),
        ("assert".into(), "Assert statement".into(), SymbolKind::Variable),
        ("pass".into(), "Pass statement".into(), SymbolKind::Variable),
        ("break".into(), "Break loop".into(), SymbolKind::Variable),
        ("continue".into(), "Continue loop".into(), SymbolKind::Variable),
        ("lambda".into(), "Lambda expression".into(), SymbolKind::Function),
        ("global".into(), "Global declaration".into(), SymbolKind::Variable),
        ("nonlocal".into(), "Nonlocal declaration".into(), SymbolKind::Variable),
        
        // Types
        ("int".into(), "Integer type".into(), SymbolKind::TypeAlias),
        ("float".into(), "Float type".into(), SymbolKind::TypeAlias),
        ("str".into(), "String type".into(), SymbolKind::TypeAlias),
        ("bool".into(), "Boolean type".into(), SymbolKind::TypeAlias),
        ("bytes".into(), "Bytes type".into(), SymbolKind::TypeAlias),
        ("List".into(), "List type".into(), SymbolKind::TypeAlias),
        ("Dict".into(), "Dictionary type".into(), SymbolKind::TypeAlias),
        ("Set".into(), "Set type".into(), SymbolKind::TypeAlias),
        ("Tuple".into(), "Tuple type".into(), SymbolKind::TypeAlias),
        ("Optional".into(), "Optional type".into(), SymbolKind::TypeAlias),
        ("Union".into(), "Union type".into(), SymbolKind::TypeAlias),
        ("Callable".into(), "Callable type".into(), SymbolKind::TypeAlias),
        ("Any".into(), "Any type".into(), SymbolKind::TypeAlias),
        ("None".into(), "None type".into(), SymbolKind::TypeAlias),
        
        // Builtins
        ("print".into(), "Print to stdout".into(), SymbolKind::Function),
        ("len".into(), "Get length".into(), SymbolKind::Function),
        ("range".into(), "Create range".into(), SymbolKind::Function),
        ("enumerate".into(), "Enumerate iterable".into(), SymbolKind::Function),
        ("zip".into(), "Zip iterables".into(), SymbolKind::Function),
        ("map".into(), "Map function".into(), SymbolKind::Function),
        ("filter".into(), "Filter iterable".into(), SymbolKind::Function),
        ("sorted".into(), "Sort iterable".into(), SymbolKind::Function),
        ("reversed".into(), "Reverse iterable".into(), SymbolKind::Function),
        ("sum".into(), "Sum of iterable".into(), SymbolKind::Function),
        ("min".into(), "Minimum value".into(), SymbolKind::Function),
        ("max".into(), "Maximum value".into(), SymbolKind::Function),
        ("abs".into(), "Absolute value".into(), SymbolKind::Function),
        ("round".into(), "Round number".into(), SymbolKind::Function),
        ("isinstance".into(), "Check instance type".into(), SymbolKind::Function),
        ("type".into(), "Get type".into(), SymbolKind::Function),
        ("input".into(), "Read input".into(), SymbolKind::Function),
        ("open".into(), "Open file".into(), SymbolKind::Function),
        
        // Decorators
        ("@property".into(), "Property decorator".into(), SymbolKind::Function),
        ("@staticmethod".into(), "Static method decorator".into(), SymbolKind::Function),
        ("@classmethod".into(), "Class method decorator".into(), SymbolKind::Function),
        ("@dataclass".into(), "Dataclass decorator".into(), SymbolKind::Function),
    ]
}

