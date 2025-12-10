//! Documentation generation for Roast.
//!
//! Generates HTML documentation from Roast source files.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write, BufWriter};
use std::path::{Path, PathBuf};

// =============================================================================
// Documentation Item
// =============================================================================

/// A documented item.
#[derive(Clone, Debug)]
pub struct DocItem {
    /// Item name
    pub name: String,
    /// Item kind (function, class, method, module, etc.)
    pub kind: DocKind,
    /// Docstring content
    pub docstring: Option<String>,
    /// Type signature
    pub signature: Option<String>,
    /// Source file location
    pub source_file: Option<PathBuf>,
    /// Line number
    pub line: Option<usize>,
    /// Parent item (for methods)
    pub parent: Option<String>,
    /// Child items
    pub children: Vec<DocItem>,
    /// Decorators
    pub decorators: Vec<String>,
    /// Parameters (for functions)
    pub params: Vec<ParamDoc>,
    /// Return type (for functions)
    pub returns: Option<String>,
    /// Examples from docstring
    pub examples: Vec<String>,
    /// Tags (deprecated, abstract, etc.)
    pub tags: Vec<String>,
}

impl DocItem {
    pub fn new(name: &str, kind: DocKind) -> Self {
        Self {
            name: name.to_string(),
            kind,
            docstring: None,
            signature: None,
            source_file: None,
            line: None,
            parent: None,
            children: Vec::new(),
            decorators: Vec::new(),
            params: Vec::new(),
            returns: None,
            examples: Vec::new(),
            tags: Vec::new(),
        }
    }
    
    /// Add a child item.
    pub fn add_child(&mut self, child: DocItem) {
        self.children.push(child);
    }
    
    /// Set the docstring.
    pub fn with_docstring(mut self, doc: &str) -> Self {
        self.docstring = Some(doc.to_string());
        self.parse_docstring();
        self
    }
    
    /// Parse the docstring for examples, params, etc.
    fn parse_docstring(&mut self) {
        if let Some(ref doc) = self.docstring {
            let lines: Vec<&str> = doc.lines().collect();
            let mut in_example = false;
            let mut current_example = String::new();
            
            for line in lines {
                let trimmed = line.trim();
                
                if trimmed.starts_with(">>>") || trimmed.starts_with("```") {
                    if in_example {
                        if !current_example.is_empty() {
                            self.examples.push(std::mem::take(&mut current_example));
                        }
                    }
                    in_example = !in_example || trimmed.starts_with(">>>");
                    if in_example && trimmed.starts_with(">>>") {
                        current_example.push_str(trimmed);
                        current_example.push('\n');
                    }
                } else if in_example {
                    current_example.push_str(line);
                    current_example.push('\n');
                } else if trimmed.starts_with("Args:") || trimmed.starts_with("Parameters:") {
                    // Parse parameter section
                } else if trimmed.starts_with("Returns:") {
                    // Parse return section
                } else if trimmed.starts_with("Deprecated") {
                    self.tags.push("deprecated".to_string());
                }
            }
            
            if !current_example.is_empty() {
                self.examples.push(current_example);
            }
        }
    }
}

/// Kind of documented item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    Module,
    Class,
    Function,
    Method,
    Property,
    Variable,
    Constant,
    TypeAlias,
    Protocol,
}

impl DocKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocKind::Module => "module",
            DocKind::Class => "class",
            DocKind::Function => "function",
            DocKind::Method => "method",
            DocKind::Property => "property",
            DocKind::Variable => "variable",
            DocKind::Constant => "constant",
            DocKind::TypeAlias => "type",
            DocKind::Protocol => "protocol",
        }
    }
}

/// Parameter documentation.
#[derive(Clone, Debug)]
pub struct ParamDoc {
    pub name: String,
    pub type_: Option<String>,
    pub description: Option<String>,
    pub default: Option<String>,
}

// =============================================================================
// Documentation Generator
// =============================================================================

/// Generate documentation from Roast source files.
pub struct DocGenerator {
    /// Output directory
    output_dir: PathBuf,
    /// Project name
    project_name: String,
    /// Project version
    project_version: String,
    /// Include private items
    include_private: bool,
    /// Theme
    theme: DocTheme,
    /// Collected items
    items: Vec<DocItem>,
    /// Cross-references
    references: HashMap<String, String>,
}

/// Documentation theme.
#[derive(Clone, Copy, Debug, Default)]
pub enum DocTheme {
    #[default]
    Light,
    Dark,
    Auto,
}

impl DocGenerator {
    /// Create a new documentation generator.
    pub fn new(output_dir: PathBuf, project_name: &str, version: &str) -> Self {
        Self {
            output_dir,
            project_name: project_name.to_string(),
            project_version: version.to_string(),
            include_private: false,
            theme: DocTheme::default(),
            items: Vec::new(),
            references: HashMap::new(),
        }
    }
    
    /// Include private items in documentation.
    pub fn with_private(mut self, include: bool) -> Self {
        self.include_private = include;
        self
    }
    
    /// Set the theme.
    pub fn with_theme(mut self, theme: DocTheme) -> Self {
        self.theme = theme;
        self
    }
    
    /// Add a documentation item.
    pub fn add_item(&mut self, item: DocItem) {
        // Skip private items unless configured
        if !self.include_private && item.name.starts_with('_') && !item.name.starts_with("__") {
            return;
        }
        
        // Add to cross-references
        let path = if let Some(ref parent) = item.parent {
            format!("{}.{}", parent, item.name)
        } else {
            item.name.clone()
        };
        self.references.insert(path.clone(), format!("{}.html", path.replace('.', "/")));
        
        self.items.push(item);
    }
    
    /// Generate the documentation.
    pub fn generate(&self) -> io::Result<()> {
        // Create output directory
        fs::create_dir_all(&self.output_dir)?;
        
        // Generate index
        self.generate_index()?;
        
        // Generate CSS
        self.generate_css()?;
        
        // Generate JS
        self.generate_js()?;
        
        // Generate pages for each item
        for item in &self.items {
            self.generate_item_page(item)?;
        }
        
        // Generate search index
        self.generate_search_index()?;
        
        Ok(())
    }
    
    fn generate_index(&self) -> io::Result<()> {
        let path = self.output_dir.join("index.html");
        let mut file = BufWriter::new(File::create(path)?);
        
        writeln!(file, "<!DOCTYPE html>")?;
        writeln!(file, "<html lang=\"en\">")?;
        writeln!(file, "<head>")?;
        writeln!(file, "  <meta charset=\"UTF-8\">")?;
        writeln!(file, "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">")?;
        writeln!(file, "  <title>{} - Documentation</title>", self.project_name)?;
        writeln!(file, "  <link rel=\"stylesheet\" href=\"style.css\">")?;
        writeln!(file, "</head>")?;
        writeln!(file, "<body>")?;
        
        // Header
        writeln!(file, "  <header>")?;
        writeln!(file, "    <h1>{}</h1>", self.project_name)?;
        writeln!(file, "    <span class=\"version\">v{}</span>", self.project_version)?;
        writeln!(file, "    <input type=\"search\" id=\"search\" placeholder=\"Search...\">")?;
        writeln!(file, "  </header>")?;
        
        // Navigation
        writeln!(file, "  <nav>")?;
        writeln!(file, "    <h2>Modules</h2>")?;
        writeln!(file, "    <ul>")?;
        for item in self.items.iter().filter(|i| i.kind == DocKind::Module) {
            writeln!(file, "      <li><a href=\"{}.html\">{}</a></li>", item.name, item.name)?;
        }
        writeln!(file, "    </ul>")?;
        
        writeln!(file, "    <h2>Classes</h2>")?;
        writeln!(file, "    <ul>")?;
        for item in self.items.iter().filter(|i| i.kind == DocKind::Class) {
            writeln!(file, "      <li><a href=\"{}.html\">{}</a></li>", item.name, item.name)?;
        }
        writeln!(file, "    </ul>")?;
        
        writeln!(file, "    <h2>Functions</h2>")?;
        writeln!(file, "    <ul>")?;
        for item in self.items.iter().filter(|i| i.kind == DocKind::Function) {
            writeln!(file, "      <li><a href=\"{}.html\">{}</a></li>", item.name, item.name)?;
        }
        writeln!(file, "    </ul>")?;
        writeln!(file, "  </nav>")?;
        
        // Main content
        writeln!(file, "  <main>")?;
        writeln!(file, "    <h2>Welcome to {} documentation</h2>", self.project_name)?;
        writeln!(file, "    <p>Version {}</p>", self.project_version)?;
        
        // Quick stats
        let module_count = self.items.iter().filter(|i| i.kind == DocKind::Module).count();
        let class_count = self.items.iter().filter(|i| i.kind == DocKind::Class).count();
        let func_count = self.items.iter().filter(|i| i.kind == DocKind::Function).count();
        
        writeln!(file, "    <div class=\"stats\">")?;
        writeln!(file, "      <div class=\"stat\"><span class=\"num\">{}</span><span>Modules</span></div>", module_count)?;
        writeln!(file, "      <div class=\"stat\"><span class=\"num\">{}</span><span>Classes</span></div>", class_count)?;
        writeln!(file, "      <div class=\"stat\"><span class=\"num\">{}</span><span>Functions</span></div>", func_count)?;
        writeln!(file, "    </div>")?;
        
        writeln!(file, "  </main>")?;
        
        writeln!(file, "  <script src=\"search.js\"></script>")?;
        writeln!(file, "</body>")?;
        writeln!(file, "</html>")?;
        
        Ok(())
    }
    
    fn generate_item_page(&self, item: &DocItem) -> io::Result<()> {
        let path = self.output_dir.join(format!("{}.html", item.name));
        let mut file = BufWriter::new(File::create(path)?);
        
        writeln!(file, "<!DOCTYPE html>")?;
        writeln!(file, "<html lang=\"en\">")?;
        writeln!(file, "<head>")?;
        writeln!(file, "  <meta charset=\"UTF-8\">")?;
        writeln!(file, "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">")?;
        writeln!(file, "  <title>{} - {}</title>", item.name, self.project_name)?;
        writeln!(file, "  <link rel=\"stylesheet\" href=\"style.css\">")?;
        writeln!(file, "</head>")?;
        writeln!(file, "<body>")?;
        
        // Header
        writeln!(file, "  <header>")?;
        writeln!(file, "    <a href=\"index.html\">{}</a>", self.project_name)?;
        writeln!(file, "    <input type=\"search\" id=\"search\" placeholder=\"Search...\">")?;
        writeln!(file, "  </header>")?;
        
        // Content
        writeln!(file, "  <main>")?;
        
        // Title with kind badge
        writeln!(file, "    <h1>")?;
        writeln!(file, "      <span class=\"badge {}\">{}</span>", item.kind.as_str(), item.kind.as_str())?;
        writeln!(file, "      {}", item.name)?;
        writeln!(file, "    </h1>")?;
        
        // Tags
        if !item.tags.is_empty() {
            writeln!(file, "    <div class=\"tags\">")?;
            for tag in &item.tags {
                writeln!(file, "      <span class=\"tag {}\">{}</span>", tag, tag)?;
            }
            writeln!(file, "    </div>")?;
        }
        
        // Signature
        if let Some(ref sig) = item.signature {
            writeln!(file, "    <pre class=\"signature\"><code>{}</code></pre>", html_escape(sig))?;
        }
        
        // Source link
        if let (Some(ref src), Some(line)) = (&item.source_file, item.line) {
            writeln!(file, "    <div class=\"source\">")?;
            writeln!(file, "      <a href=\"#\">Source: {}:{}</a>", src.display(), line)?;
            writeln!(file, "    </div>")?;
        }
        
        // Docstring
        if let Some(ref doc) = item.docstring {
            writeln!(file, "    <div class=\"docstring\">")?;
            writeln!(file, "      {}", markdown_to_html(doc))?;
            writeln!(file, "    </div>")?;
        }
        
        // Parameters
        if !item.params.is_empty() {
            writeln!(file, "    <h2>Parameters</h2>")?;
            writeln!(file, "    <table class=\"params\">")?;
            writeln!(file, "      <thead><tr><th>Name</th><th>Type</th><th>Description</th></tr></thead>")?;
            writeln!(file, "      <tbody>")?;
            for param in &item.params {
                writeln!(file, "        <tr>")?;
                writeln!(file, "          <td><code>{}</code></td>", param.name)?;
                writeln!(file, "          <td><code>{}</code></td>", param.type_.as_deref().unwrap_or("-"))?;
                writeln!(file, "          <td>{}</td>", param.description.as_deref().unwrap_or(""))?;
                writeln!(file, "        </tr>")?;
            }
            writeln!(file, "      </tbody>")?;
            writeln!(file, "    </table>")?;
        }
        
        // Returns
        if let Some(ref returns) = item.returns {
            writeln!(file, "    <h2>Returns</h2>")?;
            writeln!(file, "    <p><code>{}</code></p>", returns)?;
        }
        
        // Examples
        if !item.examples.is_empty() {
            writeln!(file, "    <h2>Examples</h2>")?;
            for example in &item.examples {
                writeln!(file, "    <pre class=\"example\"><code>{}</code></pre>", html_escape(example))?;
            }
        }
        
        // Children
        if !item.children.is_empty() {
            let methods: Vec<_> = item.children.iter().filter(|c| c.kind == DocKind::Method).collect();
            let properties: Vec<_> = item.children.iter().filter(|c| c.kind == DocKind::Property).collect();
            
            if !methods.is_empty() {
                writeln!(file, "    <h2>Methods</h2>")?;
                writeln!(file, "    <ul class=\"members\">")?;
                for method in methods {
                    writeln!(file, "      <li>")?;
                    writeln!(file, "        <code>{}</code>", method.signature.as_deref().unwrap_or(&method.name))?;
                    if let Some(ref doc) = method.docstring {
                        let first_line = doc.lines().next().unwrap_or("");
                        writeln!(file, "        <p>{}</p>", first_line)?;
                    }
                    writeln!(file, "      </li>")?;
                }
                writeln!(file, "    </ul>")?;
            }
            
            if !properties.is_empty() {
                writeln!(file, "    <h2>Properties</h2>")?;
                writeln!(file, "    <ul class=\"members\">")?;
                for prop in properties {
                    writeln!(file, "      <li>")?;
                    writeln!(file, "        <code>{}</code>", prop.name)?;
                    if let Some(ref doc) = prop.docstring {
                        let first_line = doc.lines().next().unwrap_or("");
                        writeln!(file, "        <p>{}</p>", first_line)?;
                    }
                    writeln!(file, "      </li>")?;
                }
                writeln!(file, "    </ul>")?;
            }
        }
        
        writeln!(file, "  </main>")?;
        
        writeln!(file, "  <script src=\"search.js\"></script>")?;
        writeln!(file, "</body>")?;
        writeln!(file, "</html>")?;
        
        Ok(())
    }
    
    fn generate_css(&self) -> io::Result<()> {
        let path = self.output_dir.join("style.css");
        let mut file = File::create(path)?;
        
        let css = r#"
:root {
    --bg: #ffffff;
    --fg: #1a1a1a;
    --accent: #e85d04;
    --code-bg: #f4f4f4;
    --border: #e0e0e0;
    --nav-bg: #fafafa;
}

@media (prefers-color-scheme: dark) {
    :root {
        --bg: #1a1a1a;
        --fg: #e0e0e0;
        --accent: #ff8c42;
        --code-bg: #2d2d2d;
        --border: #404040;
        --nav-bg: #252525;
    }
}

* { box-sizing: border-box; margin: 0; padding: 0; }

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg);
    color: var(--fg);
    line-height: 1.6;
}

header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1rem 2rem;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 100;
}

header h1 { font-size: 1.5rem; }
header .version { color: var(--accent); font-size: 0.9rem; }
header a { color: var(--fg); text-decoration: none; font-weight: bold; }

#search {
    margin-left: auto;
    padding: 0.5rem 1rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--code-bg);
    color: var(--fg);
    width: 200px;
}

nav {
    position: fixed;
    left: 0;
    top: 60px;
    width: 250px;
    height: calc(100vh - 60px);
    overflow-y: auto;
    padding: 1rem;
    background: var(--nav-bg);
    border-right: 1px solid var(--border);
}

nav h2 { font-size: 0.9rem; margin: 1rem 0 0.5rem; color: var(--accent); }
nav ul { list-style: none; }
nav a { color: var(--fg); text-decoration: none; display: block; padding: 0.25rem 0; }
nav a:hover { color: var(--accent); }

main {
    margin-left: 250px;
    padding: 2rem;
    max-width: 900px;
}

main h1 { margin-bottom: 1rem; display: flex; align-items: center; gap: 0.5rem; }
main h2 { margin: 2rem 0 1rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem; }

.badge {
    font-size: 0.75rem;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    background: var(--accent);
    color: white;
    text-transform: uppercase;
}

.tag {
    font-size: 0.75rem;
    padding: 0.2rem 0.5rem;
    border-radius: 3px;
    margin-right: 0.5rem;
}
.tag.deprecated { background: #dc3545; color: white; }

.signature {
    background: var(--code-bg);
    padding: 1rem;
    border-radius: 4px;
    overflow-x: auto;
    margin: 1rem 0;
}

.docstring {
    margin: 1.5rem 0;
}

.docstring p { margin: 0.5rem 0; }
.docstring code { background: var(--code-bg); padding: 0.1rem 0.3rem; border-radius: 3px; }

table.params {
    width: 100%;
    border-collapse: collapse;
    margin: 1rem 0;
}

table.params th, table.params td {
    border: 1px solid var(--border);
    padding: 0.5rem;
    text-align: left;
}

table.params th { background: var(--code-bg); }

pre.example {
    background: var(--code-bg);
    padding: 1rem;
    border-radius: 4px;
    overflow-x: auto;
    margin: 1rem 0;
}

ul.members { margin: 1rem 0; }
ul.members li { 
    border-bottom: 1px solid var(--border); 
    padding: 1rem 0;
}
ul.members code { font-weight: bold; }
ul.members p { color: #666; margin-top: 0.5rem; }

.stats {
    display: flex;
    gap: 2rem;
    margin: 2rem 0;
}

.stat {
    text-align: center;
}

.stat .num {
    display: block;
    font-size: 2rem;
    font-weight: bold;
    color: var(--accent);
}

.source {
    margin: 0.5rem 0;
    font-size: 0.9rem;
}

.source a { color: #666; }
"#;
        
        file.write_all(css.as_bytes())?;
        Ok(())
    }
    
    fn generate_js(&self) -> io::Result<()> {
        let path = self.output_dir.join("search.js");
        let mut file = File::create(path)?;
        
        let js = r#"
document.addEventListener('DOMContentLoaded', function() {
    const searchInput = document.getElementById('search');
    
    searchInput.addEventListener('input', function() {
        const query = this.value.toLowerCase();
        if (query.length < 2) return;
        
        // Simple client-side search
        fetch('search-index.json')
            .then(r => r.json())
            .then(data => {
                const results = data.filter(item => 
                    item.name.toLowerCase().includes(query) ||
                    (item.doc && item.doc.toLowerCase().includes(query))
                );
                console.log('Found:', results.length, 'results');
            });
    });
});
"#;
        
        file.write_all(js.as_bytes())?;
        Ok(())
    }
    
    fn generate_search_index(&self) -> io::Result<()> {
        let path = self.output_dir.join("search-index.json");
        let mut file = File::create(path)?;
        
        write!(file, "[")?;
        let mut first = true;
        for item in &self.items {
            if !first { write!(file, ",")?; }
            first = false;
            
            write!(file, r#"{{"name":"{}","kind":"{}""#, 
                json_escape(&item.name),
                item.kind.as_str()
            )?;
            
            if let Some(ref doc) = item.docstring {
                let first_line = doc.lines().next().unwrap_or("");
                write!(file, r#","doc":"{}""#, json_escape(first_line))?;
            }
            
            write!(file, "}}")?;
        }
        write!(file, "]")?;
        
        Ok(())
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn markdown_to_html(md: &str) -> String {
    // Simple markdown to HTML conversion
    let mut html = String::new();
    let mut in_code = false;
    let mut in_list = false;
    
    for line in md.lines() {
        let trimmed = line.trim();
        
        if trimmed.starts_with("```") {
            if in_code {
                html.push_str("</code></pre>");
            } else {
                html.push_str("<pre><code>");
            }
            in_code = !in_code;
            continue;
        }
        
        if in_code {
            html.push_str(&html_escape(line));
            html.push('\n');
            continue;
        }
        
        if trimmed.starts_with("# ") {
            html.push_str(&format!("<h1>{}</h1>", &trimmed[2..]));
        } else if trimmed.starts_with("## ") {
            html.push_str(&format!("<h2>{}</h2>", &trimmed[3..]));
        } else if trimmed.starts_with("### ") {
            html.push_str(&format!("<h3>{}</h3>", &trimmed[4..]));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            if !in_list {
                html.push_str("<ul>");
                in_list = true;
            }
            html.push_str(&format!("<li>{}</li>", &trimmed[2..]));
        } else if trimmed.is_empty() {
            if in_list {
                html.push_str("</ul>");
                in_list = false;
            }
            html.push_str("<br>");
        } else {
            html.push_str(&format!("<p>{}</p>", process_inline_markdown(trimmed)));
        }
    }
    
    if in_list {
        html.push_str("</ul>");
    }
    
    html
}

fn process_inline_markdown(text: &str) -> String {
    let mut result = text.to_string();
    
    // Bold: **text** or __text__
    while let Some(start) = result.find("**") {
        if let Some(end) = result[start + 2..].find("**") {
            let bold_text = &result[start + 2..start + 2 + end];
            result = format!(
                "{}<strong>{}</strong>{}",
                &result[..start],
                bold_text,
                &result[start + 4 + end..]
            );
        } else {
            break;
        }
    }
    
    // Italic: *text* or _text_
    while let Some(start) = result.find('*') {
        if let Some(end) = result[start + 1..].find('*') {
            let italic_text = &result[start + 1..start + 1 + end];
            result = format!(
                "{}<em>{}</em>{}",
                &result[..start],
                italic_text,
                &result[start + 2 + end..]
            );
        } else {
            break;
        }
    }
    
    // Code: `text`
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let code_text = &result[start + 1..start + 1 + end];
            result = format!(
                "{}<code>{}</code>{}",
                &result[..start],
                html_escape(code_text),
                &result[start + 2 + end..]
            );
        } else {
            break;
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_doc_item() {
        let item = DocItem::new("my_function", DocKind::Function)
            .with_docstring("This is a function.\n\n>>> my_function()\n42");
        
        assert_eq!(item.name, "my_function");
        assert!(item.docstring.is_some());
        assert_eq!(item.examples.len(), 1);
    }
    
    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }
    
    #[test]
    fn test_markdown_to_html() {
        let md = "# Title\n\nHello **world**!";
        let html = markdown_to_html(md);
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>world</strong>"));
    }
}

