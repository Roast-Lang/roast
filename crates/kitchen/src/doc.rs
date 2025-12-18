//! Documentation generator for Roast projects.
//!
//! Generates HTML documentation from Roast source files,
//! similar to `cargo doc` or `pydoc`.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

// =============================================================================
// Documentation Item
// =============================================================================

/// A documented item (function, class, module).
#[derive(Debug, Clone)]
pub struct DocItem {
    /// Item name.
    pub name: String,
    /// Item kind (function, class, module, etc.).
    pub kind: DocItemKind,
    /// Docstring/description.
    pub docstring: Option<String>,
    /// Signature (for functions/methods).
    pub signature: Option<String>,
    /// Source file path.
    pub source_file: PathBuf,
    /// Line number.
    pub line: usize,
    /// Child items (e.g., methods in a class).
    pub children: Vec<DocItem>,
    /// Attributes/decorators.
    pub decorators: Vec<String>,
    /// Is this public?
    pub is_public: bool,
}

/// Kind of documented item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocItemKind {
    Module,
    Class,
    Function,
    Method,
    Property,
    Constant,
    TypeAlias,
}

impl DocItemKind {
    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            DocItemKind::Module => "module",
            DocItemKind::Class => "class",
            DocItemKind::Function => "function",
            DocItemKind::Method => "method",
            DocItemKind::Property => "property",
            DocItemKind::Constant => "const",
            DocItemKind::TypeAlias => "type",
        }
    }

    /// Get emoji for display.
    pub fn emoji(&self) -> &'static str {
        match self {
            DocItemKind::Module => "📦",
            DocItemKind::Class => "🏛️",
            DocItemKind::Function => "⚡",
            DocItemKind::Method => "🔧",
            DocItemKind::Property => "📝",
            DocItemKind::Constant => "🔒",
            DocItemKind::TypeAlias => "📐",
        }
    }
}

// =============================================================================
// Documentation Generator
// =============================================================================

/// Documentation generator configuration.
#[derive(Debug, Clone)]
pub struct DocConfig {
    /// Output directory.
    pub output_dir: PathBuf,
    /// Include private items.
    pub include_private: bool,
    /// Generate HTML (vs Markdown).
    pub html: bool,
    /// Project name.
    pub project_name: String,
    /// Project version.
    pub project_version: String,
}

impl Default for DocConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("docs"),
            include_private: false,
            html: true,
            project_name: String::from("Project"),
            project_version: String::from("0.1.0"),
        }
    }
}

/// Documentation generator.
pub struct DocGenerator {
    config: DocConfig,
    items: Vec<DocItem>,
}

impl DocGenerator {
    /// Create a new generator.
    pub fn new(config: DocConfig) -> Self {
        Self {
            config,
            items: Vec::new(),
        }
    }

    /// Add a documentation item.
    pub fn add_item(&mut self, item: DocItem) {
        self.items.push(item);
    }

    /// Parse docstring from source.
    pub fn extract_docstring(source: &str) -> Option<String> {
        let trimmed = source.trim();
        
        // Triple-quoted string
        if trimmed.starts_with("\"\"\"") && trimmed.ends_with("\"\"\"") && trimmed.len() > 6 {
            return Some(trimmed[3..trimmed.len()-3].trim().to_string());
        }
        if trimmed.starts_with("'''") && trimmed.ends_with("'''") && trimmed.len() > 6 {
            return Some(trimmed[3..trimmed.len()-3].trim().to_string());
        }
        
        // Single line comment
        if trimmed.starts_with("#") {
            return Some(trimmed[1..].trim().to_string());
        }
        
        None
    }

    /// Generate documentation.
    pub fn generate(&self) -> Result<()> {
        fs::create_dir_all(&self.config.output_dir)
            .context("Failed to create output directory")?;

        if self.config.html {
            self.generate_html()?;
        } else {
            self.generate_markdown()?;
        }

        Ok(())
    }

    /// Generate HTML documentation.
    fn generate_html(&self) -> Result<()> {
        // Generate index.html
        let index_path = self.config.output_dir.join("index.html");
        let mut index = fs::File::create(&index_path)?;

        writeln!(index, "<!DOCTYPE html>")?;
        writeln!(index, "<html lang=\"en\">")?;
        writeln!(index, "<head>")?;
        writeln!(index, "    <meta charset=\"UTF-8\">")?;
        writeln!(index, "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">")?;
        writeln!(index, "    <title>{} Documentation</title>", self.config.project_name)?;
        writeln!(index, "    <style>")?;
        writeln!(index, "{}", CSS_STYLES)?;
        writeln!(index, "    </style>")?;
        writeln!(index, "</head>")?;
        writeln!(index, "<body>")?;
        
        // Header
        writeln!(index, "<header>")?;
        writeln!(index, "    <h1>🔥 {} <span class=\"version\">v{}</span></h1>", 
            self.config.project_name, self.config.project_version)?;
        writeln!(index, "</header>")?;
        
        // Navigation
        writeln!(index, "<nav>")?;
        writeln!(index, "    <h2>Modules</h2>")?;
        writeln!(index, "    <ul>")?;
        for item in &self.items {
            if item.kind == DocItemKind::Module {
                writeln!(index, "        <li><a href=\"#{}\">📦 {}</a></li>", 
                    item.name, item.name)?;
            }
        }
        writeln!(index, "    </ul>")?;
        writeln!(index, "</nav>")?;
        
        // Main content
        writeln!(index, "<main>")?;
        for item in &self.items {
            self.write_html_item(&mut index, item, 0)?;
        }
        writeln!(index, "</main>")?;
        
        // Footer
        writeln!(index, "<footer>")?;
        writeln!(index, "    <p>Generated by Roast Doc Generator</p>")?;
        writeln!(index, "</footer>")?;
        
        writeln!(index, "</body>")?;
        writeln!(index, "</html>")?;

        Ok(())
    }

    /// Write an HTML item.
    fn write_html_item<W: Write>(&self, w: &mut W, item: &DocItem, depth: usize) -> Result<()> {
        let class = match item.kind {
            DocItemKind::Module => "module",
            DocItemKind::Class => "class",
            DocItemKind::Function | DocItemKind::Method => "function",
            _ => "item",
        };

        writeln!(w, "<section class=\"{}\" id=\"{}\">", class, item.name)?;
        
        let tag = if depth == 0 { "h2" } else if depth == 1 { "h3" } else { "h4" };
        writeln!(w, "    <{} class=\"item-header\">", tag)?;
        writeln!(w, "        <span class=\"kind\">{}</span>", item.kind.emoji())?;
        writeln!(w, "        <span class=\"name\">{}</span>", item.name)?;
        if let Some(ref sig) = item.signature {
            writeln!(w, "        <code class=\"signature\">{}</code>", 
                html_escape(sig))?;
        }
        writeln!(w, "    </{}>", tag)?;

        // Decorators
        if !item.decorators.is_empty() {
            writeln!(w, "    <div class=\"decorators\">")?;
            for dec in &item.decorators {
                writeln!(w, "        <span class=\"decorator\">@{}</span>", dec)?;
            }
            writeln!(w, "    </div>")?;
        }

        // Docstring
        if let Some(ref doc) = item.docstring {
            writeln!(w, "    <div class=\"docstring\">")?;
            writeln!(w, "        <p>{}</p>", html_escape(doc))?;
            writeln!(w, "    </div>")?;
        }

        // Source link
        writeln!(w, "    <div class=\"source\">")?;
        writeln!(w, "        <a href=\"{}#L{}\">Source: {}:{}</a>", 
            item.source_file.display(), item.line,
            item.source_file.file_name().unwrap_or_default().to_string_lossy(), 
            item.line)?;
        writeln!(w, "    </div>")?;

        // Children
        if !item.children.is_empty() {
            writeln!(w, "    <div class=\"children\">")?;
            for child in &item.children {
                self.write_html_item(w, child, depth + 1)?;
            }
            writeln!(w, "    </div>")?;
        }

        writeln!(w, "</section>")?;
        Ok(())
    }

    /// Generate Markdown documentation.
    fn generate_markdown(&self) -> Result<()> {
        let readme_path = self.config.output_dir.join("README.md");
        let mut readme = fs::File::create(&readme_path)?;

        writeln!(readme, "# 🔥 {} v{}", 
            self.config.project_name, self.config.project_version)?;
        writeln!(readme)?;
        writeln!(readme, "## API Reference")?;
        writeln!(readme)?;

        for item in &self.items {
            self.write_markdown_item(&mut readme, item, 2)?;
        }

        Ok(())
    }

    /// Write a Markdown item.
    fn write_markdown_item<W: Write>(&self, w: &mut W, item: &DocItem, level: usize) -> Result<()> {
        let hashes = "#".repeat(level);
        
        writeln!(w, "{} {} {}", hashes, item.kind.emoji(), item.name)?;
        writeln!(w)?;

        if let Some(ref sig) = item.signature {
            writeln!(w, "```python")?;
            writeln!(w, "{}", sig)?;
            writeln!(w, "```")?;
            writeln!(w)?;
        }

        if let Some(ref doc) = item.docstring {
            writeln!(w, "{}", doc)?;
            writeln!(w)?;
        }

        writeln!(w, "*Source: `{}:{}`*", 
            item.source_file.display(), item.line)?;
        writeln!(w)?;

        for child in &item.children {
            self.write_markdown_item(w, child, level + 1)?;
        }

        Ok(())
    }
}

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// CSS styles for documentation.
const CSS_STYLES: &str = r#"
:root {
    --bg-primary: #1a1a2e;
    --bg-secondary: #16213e;
    --text-primary: #eee;
    --text-secondary: #aaa;
    --accent: #e94560;
    --accent-secondary: #0f3460;
}
body {
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: 250px 1fr;
    grid-template-rows: auto 1fr auto;
    min-height: 100vh;
}
header {
    grid-column: 1 / -1;
    background: var(--bg-secondary);
    padding: 1rem 2rem;
    border-bottom: 2px solid var(--accent);
}
header h1 {
    margin: 0;
    font-size: 1.5rem;
}
header .version {
    color: var(--text-secondary);
    font-weight: normal;
    font-size: 0.9rem;
}
nav {
    background: var(--bg-secondary);
    padding: 1rem;
    overflow-y: auto;
}
nav h2 {
    font-size: 1rem;
    color: var(--accent);
    margin-top: 0;
}
nav ul {
    list-style: none;
    padding: 0;
}
nav li {
    margin: 0.5rem 0;
}
nav a {
    color: var(--text-primary);
    text-decoration: none;
}
nav a:hover {
    color: var(--accent);
}
main {
    padding: 2rem;
    overflow-y: auto;
}
section {
    margin-bottom: 2rem;
    padding: 1rem;
    background: var(--bg-secondary);
    border-radius: 8px;
    border-left: 4px solid var(--accent);
}
.item-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
}
.kind {
    font-size: 1.5rem;
}
.name {
    font-weight: bold;
    color: var(--accent);
}
.signature {
    font-size: 0.9rem;
    color: var(--text-secondary);
    background: rgba(0,0,0,0.2);
    padding: 0.2rem 0.5rem;
    border-radius: 4px;
}
.decorator {
    background: var(--accent-secondary);
    color: var(--text-primary);
    padding: 0.2rem 0.5rem;
    border-radius: 4px;
    font-size: 0.8rem;
    margin-right: 0.5rem;
}
.docstring {
    margin: 1rem 0;
    padding: 1rem;
    background: rgba(0,0,0,0.2);
    border-radius: 4px;
}
.source {
    font-size: 0.8rem;
    color: var(--text-secondary);
}
.source a {
    color: var(--text-secondary);
}
.children {
    margin-top: 1rem;
    padding-left: 1rem;
    border-left: 2px solid var(--accent-secondary);
}
footer {
    grid-column: 1 / -1;
    background: var(--bg-secondary);
    padding: 1rem 2rem;
    text-align: center;
    color: var(--text-secondary);
    font-size: 0.9rem;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_docstring() {
        assert_eq!(
            DocGenerator::extract_docstring("\"\"\"Hello world\"\"\""),
            Some("Hello world".to_string())
        );
        assert_eq!(
            DocGenerator::extract_docstring("# Comment"),
            Some("Comment".to_string())
        );
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<test>"), "&lt;test&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }
}
