//! Python to Roast migration tool.
//!
//! Provides automated conversion of Python code to Roast,
//! with type inference and ownership annotations.

use std::path::Path;
use std::fs;

/// Migration configuration.
#[derive(Clone, Debug)]
pub struct MigrateConfig {
    /// Add type annotations.
    pub add_types: bool,
    /// Add ownership annotations.
    pub add_ownership: bool,
    /// Preserve comments.
    pub preserve_comments: bool,
    /// Convert Python-specific idioms.
    pub convert_idioms: bool,
    /// Target Roast edition.
    pub edition: String,
    /// Maximum line width for formatting.
    pub line_width: usize,
}

impl Default for MigrateConfig {
    fn default() -> Self {
        Self {
            add_types: true,
            add_ownership: false,
            preserve_comments: true,
            convert_idioms: true,
            edition: "2024".into(),
            line_width: 100,
        }
    }
}

/// Migration result.
#[derive(Debug)]
pub struct MigrateResult {
    /// Converted source code.
    pub source: String,
    /// Warnings during conversion.
    pub warnings: Vec<MigrateWarning>,
    /// Statistics.
    pub stats: MigrateStats,
}

/// Migration warning.
#[derive(Debug, Clone)]
pub struct MigrateWarning {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Migration statistics.
#[derive(Debug, Default)]
pub struct MigrateStats {
    pub lines_processed: usize,
    pub functions_converted: usize,
    pub classes_converted: usize,
    pub types_inferred: usize,
    pub ownership_annotations_added: usize,
    pub idioms_converted: usize,
}

/// The Python to Roast migrator.
pub struct Migrator {
    config: MigrateConfig,
}

impl Migrator {
    pub fn new(config: MigrateConfig) -> Self {
        Self { config }
    }

    /// Migrates a Python file to Roast.
    pub fn migrate_file(&self, path: &Path) -> Result<MigrateResult, std::io::Error> {
        let source = fs::read_to_string(path)?;
        Ok(self.migrate(&source))
    }

    /// Migrates Python source code to Roast.
    pub fn migrate(&self, source: &str) -> MigrateResult {
        let mut result = MigrateResult {
            source: String::new(),
            warnings: Vec::new(),
            stats: MigrateStats::default(),
        };

        let mut output = String::new();
        let lines: Vec<&str> = source.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            result.stats.lines_processed += 1;

            // Handle multi-line statements
            let (converted, consumed, warnings) = self.convert_line(line, &lines[i..]);
            output.push_str(&converted);
            output.push('\n');

            for mut warning in warnings {
                warning.line = i + 1;
                result.warnings.push(warning);
            }

            i += consumed.max(1);
        }

        result.source = output;
        result
    }

    /// Converts a single line (or multi-line statement).
    fn convert_line(&self, line: &str, remaining: &[&str]) -> (String, usize, Vec<MigrateWarning>) {
        let mut warnings = Vec::new();
        let trimmed = line.trim();

        // Empty line or comment
        if trimmed.is_empty() {
            return (String::new(), 1, warnings);
        }

        if trimmed.starts_with('#') {
            if self.config.preserve_comments {
                return (line.to_string(), 1, warnings);
            } else {
                return (String::new(), 1, warnings);
            }
        }

        // Import statement
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            return (self.convert_import(line), 1, warnings);
        }

        // Function definition
        if trimmed.starts_with("def ") {
            let (converted, consumed) = self.convert_function(remaining);
            return (converted, consumed, warnings);
        }

        // Class definition
        if trimmed.starts_with("class ") {
            let (converted, consumed) = self.convert_class(remaining);
            return (converted, consumed, warnings);
        }

        // Async function
        if trimmed.starts_with("async def ") {
            let (converted, consumed) = self.convert_async_function(remaining);
            return (converted, consumed, warnings);
        }

        // Decorator
        if trimmed.starts_with('@') {
            return (line.to_string(), 1, warnings);
        }

        // Assignment with type inference
        if let Some(converted) = self.convert_assignment(trimmed) {
            return (self.preserve_indent(line) + &converted, 1, warnings);
        }

        // Python-specific idioms
        if self.config.convert_idioms {
            if let Some(converted) = self.convert_idiom(trimmed) {
                return (self.preserve_indent(line) + &converted, 1, warnings);
            }
        }

        // Default: return as-is
        (line.to_string(), 1, warnings)
    }

    /// Preserves the indentation of a line.
    fn preserve_indent(&self, line: &str) -> String {
        let indent_len = line.len() - line.trim_start().len();
        line[..indent_len].to_string()
    }

    /// Converts import statements.
    fn convert_import(&self, line: &str) -> String {
        let trimmed = line.trim();
        
        // from x import y -> from x import y (same in Roast)
        // import x -> import x
        
        // Handle Python standard library imports
        let converted = trimmed
            .replace("from typing import ", "from typing import ")
            .replace("from collections import ", "from collections import ")
            .replace("from dataclasses import ", "from dataclasses import ");

        self.preserve_indent(line) + &converted
    }

    /// Converts function definitions.
    fn convert_function(&self, lines: &[&str]) -> (String, usize) {
        let mut result = String::new();
        let mut consumed = 0;

        for (i, line) in lines.iter().enumerate() {
            consumed = i + 1;
            let trimmed = line.trim();

            if i == 0 {
                // First line: def statement
                let converted = self.add_function_types(trimmed);
                result.push_str(&self.preserve_indent(line));
                result.push_str(&converted);
                result.push('\n');
            } else if trimmed.is_empty() && i > 0 {
                // Empty line might end the function
                result.push('\n');
                if i + 1 < lines.len() && !lines[i + 1].starts_with(char::is_whitespace) {
                    break;
                }
            } else if !line.starts_with(char::is_whitespace) && !trimmed.is_empty() {
                // Non-indented line ends the function
                consumed = i;
                break;
            } else {
                // Body line
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline
        if result.ends_with('\n') {
            result.pop();
        }

        (result, consumed)
    }

    /// Adds type annotations to a function definition.
    fn add_function_types(&self, line: &str) -> String {
        if !self.config.add_types {
            return line.to_string();
        }

        // Parse function signature
        let line = line.trim();
        if !line.starts_with("def ") {
            return line.to_string();
        }

        // Check if already has return type
        if line.contains("->") {
            return line.to_string();
        }

        // Add return type annotation
        if line.ends_with(':') {
            let without_colon = &line[..line.len() - 1];
            format!("{} -> None:", without_colon)
        } else {
            line.to_string()
        }
    }

    /// Converts async function definitions.
    fn convert_async_function(&self, lines: &[&str]) -> (String, usize) {
        // Same as regular function, async keyword is preserved
        self.convert_function(lines)
    }

    /// Converts class definitions.
    fn convert_class(&self, lines: &[&str]) -> (String, usize) {
        let mut result = String::new();
        let mut consumed = 0;

        for (i, line) in lines.iter().enumerate() {
            consumed = i + 1;
            let trimmed = line.trim();

            if i == 0 {
                // Class definition line
                result.push_str(line);
                result.push('\n');
            } else if trimmed.is_empty() && i > 0 {
                result.push('\n');
                if i + 1 < lines.len() && !lines[i + 1].starts_with(char::is_whitespace) {
                    break;
                }
            } else if !line.starts_with(char::is_whitespace) && !trimmed.is_empty() {
                consumed = i;
                break;
            } else {
                // Method or attribute
                if trimmed.starts_with("def ") {
                    let converted = self.add_function_types(trimmed);
                    result.push_str(&self.preserve_indent(line));
                    result.push_str(&converted);
                    result.push('\n');
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        if result.ends_with('\n') {
            result.pop();
        }

        (result, consumed)
    }

    /// Converts assignment statements with type inference.
    fn convert_assignment(&self, line: &str) -> Option<String> {
        if !self.config.add_types {
            return None;
        }

        // Skip if already has type annotation
        if line.contains(':') && line.contains('=') {
            return None;
        }

        // Pattern: x = value
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return None;
        }

        let var = parts[0].trim();
        let value = parts[1].trim();

        // Skip complex assignments
        if var.contains('[') || var.contains('.') || var.contains(',') {
            return None;
        }

        // Infer type from value
        if let Some(ty) = self.infer_type(value) {
            Some(format!("{}: {} = {}", var, ty, value))
        } else {
            None
        }
    }

    /// Infers type from a value expression.
    fn infer_type(&self, value: &str) -> Option<String> {
        let value = value.trim();

        // Integer literal
        if value.parse::<i64>().is_ok() {
            return Some("int".into());
        }

        // Float literal
        if value.parse::<f64>().is_ok() && value.contains('.') {
            return Some("float".into());
        }

        // String literal
        if (value.starts_with('"') && value.ends_with('"')) ||
           (value.starts_with('\'') && value.ends_with('\'')) {
            return Some("str".into());
        }

        // F-string
        if value.starts_with("f\"") || value.starts_with("f'") {
            return Some("str".into());
        }

        // List literal
        if value.starts_with('[') && value.ends_with(']') {
            return Some("list".into());
        }

        // Dict literal
        if value.starts_with('{') && value.ends_with('}') && value.contains(':') {
            return Some("dict".into());
        }

        // Set literal
        if value.starts_with('{') && value.ends_with('}') && !value.contains(':') {
            return Some("set".into());
        }

        // Tuple literal
        if value.starts_with('(') && value.ends_with(')') && value.contains(',') {
            return Some("tuple".into());
        }

        // Boolean
        if value == "True" || value == "False" {
            return Some("bool".into());
        }

        // None
        if value == "None" {
            return Some("None".into());
        }

        // List/dict/set constructors
        if value.starts_with("list(") {
            return Some("list".into());
        }
        if value.starts_with("dict(") {
            return Some("dict".into());
        }
        if value.starts_with("set(") {
            return Some("set".into());
        }

        None
    }

    /// Converts Python-specific idioms to Roast equivalents.
    fn convert_idiom(&self, line: &str) -> Option<String> {
        // isinstance -> type check
        if line.contains("isinstance(") {
            // isinstance(x, int) -> (would need more context)
            return None;
        }

        // print without parentheses (Python 2)
        if line.starts_with("print ") && !line.starts_with("print(") {
            let args = line.strip_prefix("print ")?.trim();
            return Some(format!("print({})", args));
        }

        // xrange -> range (Python 2)
        if line.contains("xrange(") {
            return Some(line.replace("xrange(", "range("));
        }

        // raw_input -> input (Python 2)
        if line.contains("raw_input(") {
            return Some(line.replace("raw_input(", "input("));
        }

        // unicode -> str (Python 2)
        if line.contains("unicode(") {
            return Some(line.replace("unicode(", "str("));
        }

        // basestring -> str (Python 2)
        if line.contains("basestring") {
            return Some(line.replace("basestring", "str"));
        }

        None
    }

    /// Migrates a directory of Python files.
    pub fn migrate_directory(
        &self,
        src: &Path,
        dest: &Path,
    ) -> Result<Vec<(String, MigrateResult)>, std::io::Error> {
        let mut results = Vec::new();

        if !dest.exists() {
            fs::create_dir_all(dest)?;
        }

        for entry in walkdir::WalkDir::new(src)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            if path.extension().map_or(false, |e| e == "py") {
                let rel_path = path.strip_prefix(src).unwrap_or(path);
                let dest_path = dest.join(rel_path).with_extension("roast");

                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let result = self.migrate_file(path)?;
                fs::write(&dest_path, &result.source)?;

                results.push((path.to_string_lossy().to_string(), result));
            }
        }

        Ok(results)
    }
}

impl Default for Migrator {
    fn default() -> Self {
        Self::new(MigrateConfig::default())
    }
}

/// Quick migration function.
pub fn migrate(source: &str) -> String {
    Migrator::default().migrate(source).source
}

/// Quick migration with config.
pub fn migrate_with_config(source: &str, config: MigrateConfig) -> MigrateResult {
    Migrator::new(config).migrate(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_migration() {
        let py = "x = 42\ny = 'hello'";
        let result = migrate(py);
        assert!(result.contains("x: int = 42"));
        assert!(result.contains("y: str = 'hello'"));
    }

    #[test]
    fn test_function_migration() {
        let py = "def add(a, b):\n    return a + b";
        let result = migrate(py);
        assert!(result.contains("-> None:"));
    }

    #[test]
    fn test_python2_idioms() {
        let py = "print 'hello'\nxrange(10)";
        let migrator = Migrator::new(MigrateConfig {
            convert_idioms: true,
            add_types: false,
            ..Default::default()
        });
        let result = migrator.migrate(py);
        assert!(result.source.contains("print('hello')"));
        assert!(result.source.contains("range(10)"));
    }
}

