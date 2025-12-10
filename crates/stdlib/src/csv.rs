//! CSV parsing and writing for Roast.
//!
//! Provides reading and writing of CSV files.

use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};

// =============================================================================
// Errors
// =============================================================================

#[derive(Debug, Clone)]
pub enum CsvError {
    /// Parse error.
    ParseError(String),
    /// IO error.
    IoError(String),
    /// Missing column.
    MissingColumn(String),
    /// Invalid row.
    InvalidRow(String),
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsvError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            CsvError::IoError(msg) => write!(f, "IO error: {}", msg),
            CsvError::MissingColumn(col) => write!(f, "Missing column: {}", col),
            CsvError::InvalidRow(msg) => write!(f, "Invalid row: {}", msg),
        }
    }
}

impl std::error::Error for CsvError {}

pub type CsvResult<T> = Result<T, CsvError>;

// =============================================================================
// Dialect
// =============================================================================

/// CSV dialect options.
#[derive(Clone, Debug)]
pub struct Dialect {
    /// Field delimiter.
    pub delimiter: char,
    /// Quote character.
    pub quotechar: char,
    /// Escape character.
    pub escapechar: Option<char>,
    /// Whether to double quotes for escaping.
    pub doublequote: bool,
    /// Skip initial whitespace.
    pub skipinitialspace: bool,
    /// Line terminator.
    pub lineterminator: String,
    /// Quoting mode.
    pub quoting: Quoting,
    /// Strict mode.
    pub strict: bool,
}

/// Quoting modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quoting {
    /// Quote all fields.
    All,
    /// Quote fields with special characters.
    Minimal,
    /// Quote non-numeric fields.
    NonNumeric,
    /// Never quote.
    None,
}

impl Default for Dialect {
    fn default() -> Self {
        Self {
            delimiter: ',',
            quotechar: '"',
            escapechar: None,
            doublequote: true,
            skipinitialspace: false,
            lineterminator: "\r\n".to_string(),
            quoting: Quoting::Minimal,
            strict: false,
        }
    }
}

impl Dialect {
    /// Create Excel dialect.
    pub fn excel() -> Self {
        Self::default()
    }
    
    /// Create Excel-Tab dialect.
    pub fn excel_tab() -> Self {
        Self {
            delimiter: '\t',
            ..Self::default()
        }
    }
    
    /// Create Unix dialect.
    pub fn unix() -> Self {
        Self {
            lineterminator: "\n".to_string(),
            quoting: Quoting::All,
            ..Self::default()
        }
    }
}

// =============================================================================
// Reader
// =============================================================================

/// CSV reader.
pub struct Reader<R> {
    reader: BufReader<R>,
    dialect: Dialect,
    line_num: usize,
}

impl<R: Read> Reader<R> {
    /// Create a new CSV reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            dialect: Dialect::default(),
            line_num: 0,
        }
    }
    
    /// Create with custom dialect.
    pub fn with_dialect(reader: R, dialect: Dialect) -> Self {
        Self {
            reader: BufReader::new(reader),
            dialect,
            line_num: 0,
        }
    }
    
    /// Get current line number.
    pub fn line_num(&self) -> usize {
        self.line_num
    }
    
    /// Read the next row.
    pub fn read_row(&mut self) -> CsvResult<Option<Vec<String>>> {
        let mut line = String::new();
        
        loop {
            let bytes_read = self.reader.read_line(&mut line)
                .map_err(|e| CsvError::IoError(e.to_string()))?;
            
            if bytes_read == 0 {
                if line.is_empty() {
                    return Ok(None);
                }
                break;
            }
            
            self.line_num += 1;
            
            // Check if we have a complete row (handles multiline quoted fields)
            if self.is_complete_row(&line) {
                break;
            }
        }
        
        // Trim line terminator
        let line = line.trim_end_matches(&['\r', '\n'][..]);
        
        if line.is_empty() && self.line_num > 1 {
            // Skip empty lines
            return self.read_row();
        }
        
        Ok(Some(self.parse_row(line)?))
    }
    
    fn is_complete_row(&self, s: &str) -> bool {
        // Count unescaped quotes
        let quote = self.dialect.quotechar;
        let mut in_quotes = false;
        let mut prev_was_quote = false;
        
        for c in s.chars() {
            if c == quote {
                if prev_was_quote && self.dialect.doublequote {
                    prev_was_quote = false;
                    continue;
                }
                in_quotes = !in_quotes;
                prev_was_quote = true;
            } else {
                prev_was_quote = false;
            }
        }
        
        !in_quotes
    }
    
    fn parse_row(&self, line: &str) -> CsvResult<Vec<String>> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();
        
        let delim = self.dialect.delimiter;
        let quote = self.dialect.quotechar;
        
        while let Some(c) = chars.next() {
            if in_quotes {
                if c == quote {
                    if self.dialect.doublequote && chars.peek() == Some(&quote) {
                        // Escaped quote
                        chars.next();
                        current.push(quote);
                    } else {
                        // End of quoted field
                        in_quotes = false;
                    }
                } else {
                    current.push(c);
                }
            } else {
                if c == quote {
                    in_quotes = true;
                } else if c == delim {
                    fields.push(current.clone());
                    current.clear();
                } else if self.dialect.skipinitialspace && c == ' ' && current.is_empty() {
                    // Skip leading space
                } else {
                    current.push(c);
                }
            }
        }
        
        // Add the last field
        fields.push(current);
        
        Ok(fields)
    }
    
    /// Read all rows.
    pub fn read_all(&mut self) -> CsvResult<Vec<Vec<String>>> {
        let mut rows = Vec::new();
        while let Some(row) = self.read_row()? {
            rows.push(row);
        }
        Ok(rows)
    }
}

/// Iterator over CSV rows.
pub struct RowIterator<R> {
    reader: Reader<R>,
}

impl<R: Read> Iterator for RowIterator<R> {
    type Item = CsvResult<Vec<String>>;
    
    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_row() {
            Ok(Some(row)) => Some(Ok(row)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<R: Read> IntoIterator for Reader<R> {
    type Item = CsvResult<Vec<String>>;
    type IntoIter = RowIterator<R>;
    
    fn into_iter(self) -> Self::IntoIter {
        RowIterator { reader: self }
    }
}

// =============================================================================
// DictReader
// =============================================================================

/// CSV reader that returns dictionaries.
pub struct DictReader<R> {
    reader: Reader<R>,
    fieldnames: Option<Vec<String>>,
    restkey: String,
    restval: String,
}

impl<R: Read> DictReader<R> {
    /// Create a new DictReader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: Reader::new(reader),
            fieldnames: None,
            restkey: "_extra".to_string(),
            restval: "".to_string(),
        }
    }
    
    /// Create with custom dialect.
    pub fn with_dialect(reader: R, dialect: Dialect) -> Self {
        Self {
            reader: Reader::with_dialect(reader, dialect),
            fieldnames: None,
            restkey: "_extra".to_string(),
            restval: "".to_string(),
        }
    }
    
    /// Set field names explicitly.
    pub fn with_fieldnames(mut self, names: Vec<String>) -> Self {
        self.fieldnames = Some(names);
        self
    }
    
    /// Get field names (reading from first row if needed).
    pub fn fieldnames(&mut self) -> CsvResult<&[String]> {
        if self.fieldnames.is_none() {
            if let Some(row) = self.reader.read_row()? {
                self.fieldnames = Some(row);
            }
        }
        
        self.fieldnames.as_deref()
            .ok_or_else(|| CsvError::ParseError("No field names".into()))
    }
    
    /// Read the next row as a dictionary.
    pub fn read_row(&mut self) -> CsvResult<Option<HashMap<String, String>>> {
        // Ensure we have field names
        let names = self.fieldnames()?.to_vec();
        
        let row = match self.reader.read_row()? {
            Some(r) => r,
            None => return Ok(None),
        };
        
        let mut result = HashMap::new();
        
        for (i, value) in row.iter().enumerate() {
            if i < names.len() {
                result.insert(names[i].clone(), value.clone());
            } else {
                // Extra field
                let key = format!("{}_{}", self.restkey, i - names.len());
                result.insert(key, value.clone());
            }
        }
        
        // Fill missing fields
        for name in &names {
            if !result.contains_key(name) {
                result.insert(name.clone(), self.restval.clone());
            }
        }
        
        Ok(Some(result))
    }
    
    /// Read all rows as dictionaries.
    pub fn read_all(&mut self) -> CsvResult<Vec<HashMap<String, String>>> {
        let mut rows = Vec::new();
        while let Some(row) = self.read_row()? {
            rows.push(row);
        }
        Ok(rows)
    }
}

// =============================================================================
// Writer
// =============================================================================

/// CSV writer.
pub struct Writer<W> {
    writer: W,
    dialect: Dialect,
}

impl<W: Write> Writer<W> {
    /// Create a new CSV writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            dialect: Dialect::default(),
        }
    }
    
    /// Create with custom dialect.
    pub fn with_dialect(writer: W, dialect: Dialect) -> Self {
        Self { writer, dialect }
    }
    
    /// Write a row.
    pub fn write_row(&mut self, row: &[impl AsRef<str>]) -> CsvResult<()> {
        let formatted: Vec<_> = row.iter()
            .map(|f| self.format_field(f.as_ref()))
            .collect();
        
        let line = formatted.join(&self.dialect.delimiter.to_string());
        
        writeln!(self.writer, "{}", line)
            .map_err(|e| CsvError::IoError(e.to_string()))?;
        
        Ok(())
    }
    
    fn format_field(&self, field: &str) -> String {
        let needs_quote = match self.dialect.quoting {
            Quoting::All => true,
            Quoting::None => false,
            Quoting::NonNumeric => field.parse::<f64>().is_err(),
            Quoting::Minimal => {
                field.contains(self.dialect.delimiter)
                    || field.contains(self.dialect.quotechar)
                    || field.contains('\n')
                    || field.contains('\r')
            }
        };
        
        if needs_quote {
            let escaped = if self.dialect.doublequote {
                field.replace(
                    self.dialect.quotechar,
                    &format!("{}{}", self.dialect.quotechar, self.dialect.quotechar),
                )
            } else if let Some(esc) = self.dialect.escapechar {
                field.replace(
                    self.dialect.quotechar,
                    &format!("{}{}", esc, self.dialect.quotechar),
                )
            } else {
                field.to_string()
            };
            
            format!("{}{}{}", self.dialect.quotechar, escaped, self.dialect.quotechar)
        } else {
            field.to_string()
        }
    }
    
    /// Write multiple rows.
    pub fn write_rows(&mut self, rows: &[Vec<impl AsRef<str>>]) -> CsvResult<()> {
        for row in rows {
            self.write_row(row)?;
        }
        Ok(())
    }
    
    /// Flush the writer.
    pub fn flush(&mut self) -> CsvResult<()> {
        self.writer.flush()
            .map_err(|e| CsvError::IoError(e.to_string()))
    }
}

// =============================================================================
// DictWriter
// =============================================================================

/// CSV writer that accepts dictionaries.
pub struct DictWriter<W> {
    writer: Writer<W>,
    fieldnames: Vec<String>,
    extrasaction: ExtrasAction,
    wrote_header: bool,
}

/// Action for extra keys.
#[derive(Clone, Copy, Debug)]
pub enum ExtrasAction {
    /// Raise error for extra keys.
    Raise,
    /// Ignore extra keys.
    Ignore,
}

impl<W: Write> DictWriter<W> {
    /// Create a new DictWriter.
    pub fn new(writer: W, fieldnames: Vec<String>) -> Self {
        Self {
            writer: Writer::new(writer),
            fieldnames,
            extrasaction: ExtrasAction::Raise,
            wrote_header: false,
        }
    }
    
    /// Create with custom dialect.
    pub fn with_dialect(writer: W, fieldnames: Vec<String>, dialect: Dialect) -> Self {
        Self {
            writer: Writer::with_dialect(writer, dialect),
            fieldnames,
            extrasaction: ExtrasAction::Raise,
            wrote_header: false,
        }
    }
    
    /// Set extras action.
    pub fn extrasaction(mut self, action: ExtrasAction) -> Self {
        self.extrasaction = action;
        self
    }
    
    /// Write the header row.
    pub fn write_header(&mut self) -> CsvResult<()> {
        self.writer.write_row(&self.fieldnames)?;
        self.wrote_header = true;
        Ok(())
    }
    
    /// Write a row from a dictionary.
    pub fn write_row(&mut self, row: &HashMap<String, String>) -> CsvResult<()> {
        // Check for extra keys
        if matches!(self.extrasaction, ExtrasAction::Raise) {
            for key in row.keys() {
                if !self.fieldnames.contains(key) {
                    return Err(CsvError::InvalidRow(format!("Extra key: {}", key)));
                }
            }
        }
        
        let values: Vec<String> = self.fieldnames.iter()
            .map(|name| row.get(name).cloned().unwrap_or_default())
            .collect();
        
        self.writer.write_row(&values)
    }
    
    /// Write multiple rows.
    pub fn write_rows(&mut self, rows: &[HashMap<String, String>]) -> CsvResult<()> {
        for row in rows {
            self.write_row(row)?;
        }
        Ok(())
    }
    
    /// Flush the writer.
    pub fn flush(&mut self) -> CsvResult<()> {
        self.writer.flush()
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Read CSV from a string.
pub fn from_str(s: &str) -> CsvResult<Vec<Vec<String>>> {
    let cursor = std::io::Cursor::new(s);
    let mut reader = Reader::new(cursor);
    reader.read_all()
}

/// Read CSV with header from a string.
pub fn from_str_with_header(s: &str) -> CsvResult<Vec<HashMap<String, String>>> {
    let cursor = std::io::Cursor::new(s);
    let mut reader = DictReader::new(cursor);
    reader.read_all()
}

/// Write CSV to a string.
pub fn to_string(rows: &[Vec<impl AsRef<str>>]) -> CsvResult<String> {
    let mut buffer = Vec::new();
    {
        let mut writer = Writer::new(&mut buffer);
        writer.write_rows(rows)?;
    }
    String::from_utf8(buffer)
        .map_err(|e| CsvError::IoError(e.to_string()))
}

/// Write CSV with header to a string.
pub fn to_string_with_header(
    fieldnames: &[&str],
    rows: &[HashMap<String, String>],
) -> CsvResult<String> {
    let mut buffer = Vec::new();
    {
        let names: Vec<String> = fieldnames.iter().map(|s| s.to_string()).collect();
        let mut writer = DictWriter::new(&mut buffer, names);
        writer.write_header()?;
        writer.write_rows(rows)?;
    }
    String::from_utf8(buffer)
        .map_err(|e| CsvError::IoError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_read_simple() {
        let data = "a,b,c\n1,2,3\n4,5,6";
        let rows = from_str(data).unwrap();
        
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["a", "b", "c"]);
        assert_eq!(rows[1], vec!["1", "2", "3"]);
        assert_eq!(rows[2], vec!["4", "5", "6"]);
    }
    
    #[test]
    fn test_read_quoted() {
        let data = r#"name,value
"John ""Johnny"" Doe",100
"Jane, Doe",200"#;
        let rows = from_str(data).unwrap();
        
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][0], r#"John "Johnny" Doe"#);
        assert_eq!(rows[2][0], "Jane, Doe");
    }
    
    #[test]
    fn test_dict_reader() {
        let data = "name,age,city\nAlice,30,NYC\nBob,25,LA";
        let rows = from_str_with_header(data).unwrap();
        
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("name").unwrap(), "Alice");
        assert_eq!(rows[0].get("age").unwrap(), "30");
        assert_eq!(rows[1].get("city").unwrap(), "LA");
    }
    
    #[test]
    fn test_write_simple() {
        let rows: Vec<Vec<&str>> = vec![
            vec!["a", "b", "c"],
            vec!["1", "2", "3"],
        ];
        let result = to_string(&rows).unwrap();
        
        assert!(result.contains("a,b,c"));
        assert!(result.contains("1,2,3"));
    }
    
    #[test]
    fn test_write_quoted() {
        let rows: Vec<Vec<&str>> = vec![
            vec!["name", "value"],
            vec!["John, Doe", "100"],
        ];
        let result = to_string(&rows).unwrap();
        
        assert!(result.contains("\"John, Doe\""));
    }
    
    #[test]
    fn test_dict_writer() {
        let mut rows = Vec::new();
        
        let mut row1 = HashMap::new();
        row1.insert("name".to_string(), "Alice".to_string());
        row1.insert("age".to_string(), "30".to_string());
        rows.push(row1);
        
        let result = to_string_with_header(&["name", "age"], &rows).unwrap();
        
        assert!(result.contains("name,age"));
        assert!(result.contains("Alice,30"));
    }
    
    #[test]
    fn test_custom_delimiter() {
        let data = "a\tb\tc\n1\t2\t3";
        let cursor = std::io::Cursor::new(data);
        let mut reader = Reader::with_dialect(cursor, Dialect::excel_tab());
        let rows = reader.read_all().unwrap();
        
        assert_eq!(rows[0], vec!["a", "b", "c"]);
        assert_eq!(rows[1], vec!["1", "2", "3"]);
    }
}

