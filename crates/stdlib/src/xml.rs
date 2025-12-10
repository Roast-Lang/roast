//! XML parsing and generation for Roast.
//!
//! Provides a simple XML parser and builder.

use std::collections::HashMap;
use std::io::{Read, Write};

// =============================================================================
// XML Types
// =============================================================================

/// An XML element.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// Tag name
    pub tag: String,
    /// Attributes
    pub attributes: HashMap<String, String>,
    /// Child nodes
    pub children: Vec<Node>,
    /// Text content (for simple elements)
    pub text: Option<String>,
}

impl Element {
    /// Create a new element with the given tag.
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text: None,
        }
    }
    
    /// Set an attribute.
    pub fn with_attr(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Add a child element.
    pub fn with_child(mut self, child: Element) -> Self {
        self.children.push(Node::Element(child));
        self
    }
    
    /// Set text content.
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }
    
    /// Get an attribute value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }
    
    /// Set an attribute.
    pub fn set(&mut self, key: &str, value: &str) {
        self.attributes.insert(key.to_string(), value.to_string());
    }
    
    /// Add a child node.
    pub fn append(&mut self, child: Node) {
        self.children.push(child);
    }
    
    /// Add a child element.
    pub fn append_element(&mut self, child: Element) {
        self.children.push(Node::Element(child));
    }
    
    /// Add text content.
    pub fn append_text(&mut self, text: &str) {
        self.children.push(Node::Text(text.to_string()));
    }
    
    /// Find first child element with given tag.
    pub fn find(&self, tag: &str) -> Option<&Element> {
        for child in &self.children {
            if let Node::Element(e) = child {
                if e.tag == tag {
                    return Some(e);
                }
            }
        }
        None
    }
    
    /// Find all child elements with given tag.
    pub fn find_all(&self, tag: &str) -> Vec<&Element> {
        self.children
            .iter()
            .filter_map(|n| {
                if let Node::Element(e) = n {
                    if e.tag == tag {
                        return Some(e);
                    }
                }
                None
            })
            .collect()
    }
    
    /// Get all text content (concatenated).
    pub fn text_content(&self) -> String {
        let mut result = String::new();
        if let Some(ref t) = self.text {
            result.push_str(t);
        }
        for child in &self.children {
            match child {
                Node::Text(t) => result.push_str(t),
                Node::Element(e) => result.push_str(&e.text_content()),
                _ => {}
            }
        }
        result
    }
    
    /// Iterate over child elements.
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| {
            if let Node::Element(e) = n {
                Some(e)
            } else {
                None
            }
        })
    }
}

/// An XML node.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// Element node
    Element(Element),
    /// Text node
    Text(String),
    /// Comment node
    Comment(String),
    /// CDATA section
    CData(String),
    /// Processing instruction
    ProcessingInstruction(String, String),
}

/// An XML document.
#[derive(Clone, Debug)]
pub struct Document {
    /// XML declaration
    pub declaration: Option<Declaration>,
    /// Root element
    pub root: Option<Element>,
    /// Comments before root
    pub prologue: Vec<Node>,
}

impl Document {
    /// Create a new empty document.
    pub fn new() -> Self {
        Self {
            declaration: Some(Declaration::default()),
            root: None,
            prologue: Vec::new(),
        }
    }
    
    /// Create a document with a root element.
    pub fn with_root(root: Element) -> Self {
        Self {
            declaration: Some(Declaration::default()),
            root: Some(root),
            prologue: Vec::new(),
        }
    }
    
    /// Set the root element.
    pub fn set_root(&mut self, root: Element) {
        self.root = Some(root);
    }
    
    /// Get the root element.
    pub fn root(&self) -> Option<&Element> {
        self.root.as_ref()
    }
    
    /// Get mutable root element.
    pub fn root_mut(&mut self) -> Option<&mut Element> {
        self.root.as_mut()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// XML declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    pub version: String,
    pub encoding: Option<String>,
    pub standalone: Option<bool>,
}

impl Default for Declaration {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            encoding: Some("UTF-8".to_string()),
            standalone: None,
        }
    }
}

// =============================================================================
// XML Parser
// =============================================================================

/// XML parsing error.
#[derive(Clone, Debug)]
pub enum ParseError {
    UnexpectedEof,
    InvalidSyntax(String),
    UnmatchedTag { expected: String, found: String },
    InvalidCharacter(char),
    InvalidEntity(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEof => write!(f, "Unexpected end of input"),
            ParseError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
            ParseError::UnmatchedTag { expected, found } => {
                write!(f, "Unmatched tag: expected </{}>, found </{}>", expected, found)
            }
            ParseError::InvalidCharacter(c) => write!(f, "Invalid character: '{}'", c),
            ParseError::InvalidEntity(e) => write!(f, "Invalid entity: &{};", e),
        }
    }
}

impl std::error::Error for ParseError {}

/// XML parser.
pub struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Create a new parser.
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }
    
    /// Parse the document.
    pub fn parse(&mut self) -> Result<Document, ParseError> {
        let mut doc = Document::new();
        
        self.skip_whitespace();
        
        // Parse declaration if present
        if self.starts_with("<?xml") {
            doc.declaration = Some(self.parse_declaration()?);
            self.skip_whitespace();
        }
        
        // Parse prologue (comments, PIs)
        while self.pos < self.input.len() {
            self.skip_whitespace();
            if self.starts_with("<!--") {
                doc.prologue.push(Node::Comment(self.parse_comment()?));
            } else if self.starts_with("<?") {
                let (target, data) = self.parse_processing_instruction()?;
                doc.prologue.push(Node::ProcessingInstruction(target, data));
            } else if self.starts_with("<!DOCTYPE") {
                self.skip_doctype()?;
            } else {
                break;
            }
        }
        
        // Parse root element
        if self.pos < self.input.len() && self.peek() == Some('<') {
            doc.root = Some(self.parse_element()?);
        }
        
        Ok(doc)
    }
    
    fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        self.expect("<?xml")?;
        self.skip_whitespace();
        
        let mut decl = Declaration::default();
        
        // Parse attributes
        while !self.starts_with("?>") {
            self.skip_whitespace();
            if self.starts_with("?>") {
                break;
            }
            
            let name = self.parse_name()?;
            self.skip_whitespace();
            self.expect("=")?;
            self.skip_whitespace();
            let value = self.parse_quoted_value()?;
            
            match name.as_str() {
                "version" => decl.version = value,
                "encoding" => decl.encoding = Some(value),
                "standalone" => decl.standalone = Some(value == "yes"),
                _ => {}
            }
        }
        
        self.expect("?>")?;
        Ok(decl)
    }
    
    fn parse_element(&mut self) -> Result<Element, ParseError> {
        self.expect("<")?;
        let tag = self.parse_name()?;
        let mut elem = Element::new(&tag);
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.starts_with("/>") {
                self.advance(2);
                return Ok(elem);
            }
            
            if self.starts_with(">") {
                self.advance(1);
                break;
            }
            
            let attr_name = self.parse_name()?;
            self.skip_whitespace();
            self.expect("=")?;
            self.skip_whitespace();
            let attr_value = self.parse_quoted_value()?;
            elem.attributes.insert(attr_name, attr_value);
        }
        
        // Parse children
        loop {
            if self.starts_with("</") {
                break;
            }
            
            if self.pos >= self.input.len() {
                return Err(ParseError::UnexpectedEof);
            }
            
            if self.starts_with("<!--") {
                elem.children.push(Node::Comment(self.parse_comment()?));
            } else if self.starts_with("<![CDATA[") {
                elem.children.push(Node::CData(self.parse_cdata()?));
            } else if self.starts_with("<?") {
                let (target, data) = self.parse_processing_instruction()?;
                elem.children.push(Node::ProcessingInstruction(target, data));
            } else if self.starts_with("<") {
                elem.children.push(Node::Element(self.parse_element()?));
            } else {
                let text = self.parse_text()?;
                if !text.trim().is_empty() {
                    elem.children.push(Node::Text(text));
                }
            }
        }
        
        // Parse closing tag
        self.expect("</")?;
        let close_tag = self.parse_name()?;
        if close_tag != tag {
            return Err(ParseError::UnmatchedTag {
                expected: tag,
                found: close_tag,
            });
        }
        self.skip_whitespace();
        self.expect(">")?;
        
        Ok(elem)
    }
    
    fn parse_name(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        
        // First char must be letter or _
        match self.peek() {
            Some(c) if c.is_alphabetic() || c == '_' || c == ':' => {
                self.advance(1);
            }
            Some(c) => return Err(ParseError::InvalidCharacter(c)),
            None => return Err(ParseError::UnexpectedEof),
        }
        
        // Rest can include digits, hyphens, dots
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == ':' {
                self.advance(1);
            } else {
                break;
            }
        }
        
        Ok(self.input[start..self.pos].to_string())
    }
    
    fn parse_quoted_value(&mut self) -> Result<String, ParseError> {
        let quote = self.peek().ok_or(ParseError::UnexpectedEof)?;
        if quote != '"' && quote != '\'' {
            return Err(ParseError::InvalidSyntax("Expected quote".to_string()));
        }
        self.advance(1);
        
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                let value = self.input[start..self.pos].to_string();
                self.advance(1);
                return Ok(self.unescape(&value));
            }
            self.advance(1);
        }
        
        Err(ParseError::UnexpectedEof)
    }
    
    fn parse_text(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '<' {
                break;
            }
            self.advance(1);
        }
        let text = self.input[start..self.pos].to_string();
        Ok(self.unescape(&text))
    }
    
    fn parse_comment(&mut self) -> Result<String, ParseError> {
        self.expect("<!--")?;
        let start = self.pos;
        
        while self.pos < self.input.len() {
            if self.starts_with("-->") {
                let comment = self.input[start..self.pos].to_string();
                self.advance(3);
                return Ok(comment);
            }
            self.advance(1);
        }
        
        Err(ParseError::UnexpectedEof)
    }
    
    fn parse_cdata(&mut self) -> Result<String, ParseError> {
        self.expect("<![CDATA[")?;
        let start = self.pos;
        
        while self.pos < self.input.len() {
            if self.starts_with("]]>") {
                let data = self.input[start..self.pos].to_string();
                self.advance(3);
                return Ok(data);
            }
            self.advance(1);
        }
        
        Err(ParseError::UnexpectedEof)
    }
    
    fn parse_processing_instruction(&mut self) -> Result<(String, String), ParseError> {
        self.expect("<?")?;
        let target = self.parse_name()?;
        self.skip_whitespace();
        
        let start = self.pos;
        while self.pos < self.input.len() {
            if self.starts_with("?>") {
                let data = self.input[start..self.pos].to_string();
                self.advance(2);
                return Ok((target, data.trim().to_string()));
            }
            self.advance(1);
        }
        
        Err(ParseError::UnexpectedEof)
    }
    
    fn skip_doctype(&mut self) -> Result<(), ParseError> {
        self.expect("<!DOCTYPE")?;
        let mut depth = 1;
        
        while self.pos < self.input.len() && depth > 0 {
            if self.starts_with("[") {
                depth += 1;
            } else if self.starts_with("]") {
                depth -= 1;
            } else if self.starts_with(">") && depth == 1 {
                self.advance(1);
                return Ok(());
            }
            self.advance(1);
        }
        
        Err(ParseError::UnexpectedEof)
    }
    
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance(1);
            } else {
                break;
            }
        }
    }
    
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    
    fn advance(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.input.len());
    }
    
    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }
    
    fn expect(&mut self, s: &str) -> Result<(), ParseError> {
        if self.starts_with(s) {
            self.advance(s.len());
            Ok(())
        } else {
            Err(ParseError::InvalidSyntax(format!("Expected '{}'", s)))
        }
    }
    
    fn unescape(&self, s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
    }
}

// =============================================================================
// XML Writer
// =============================================================================

/// XML writer.
pub struct Writer {
    output: String,
    indent: usize,
    indent_str: String,
    pretty: bool,
}

impl Writer {
    /// Create a new writer.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            indent_str: "  ".to_string(),
            pretty: true,
        }
    }
    
    /// Create a compact writer (no indentation).
    pub fn compact() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            indent_str: String::new(),
            pretty: false,
        }
    }
    
    /// Write a document.
    pub fn write_document(&mut self, doc: &Document) -> String {
        self.output.clear();
        
        // Write declaration
        if let Some(ref decl) = doc.declaration {
            self.write_declaration(decl);
        }
        
        // Write prologue
        for node in &doc.prologue {
            self.write_node(node);
        }
        
        // Write root
        if let Some(ref root) = doc.root {
            self.write_element(root);
        }
        
        self.output.clone()
    }
    
    /// Write an element.
    pub fn write_element(&mut self, elem: &Element) -> String {
        self.output.clear();
        self.write_element_inner(elem);
        self.output.clone()
    }
    
    fn write_declaration(&mut self, decl: &Declaration) {
        self.output.push_str("<?xml");
        self.output.push_str(&format!(" version=\"{}\"", decl.version));
        if let Some(ref enc) = decl.encoding {
            self.output.push_str(&format!(" encoding=\"{}\"", enc));
        }
        if let Some(standalone) = decl.standalone {
            self.output.push_str(&format!(" standalone=\"{}\"", if standalone { "yes" } else { "no" }));
        }
        self.output.push_str("?>");
        if self.pretty {
            self.output.push('\n');
        }
    }
    
    fn write_element_inner(&mut self, elem: &Element) {
        // Indent
        if self.pretty {
            self.output.push_str(&self.indent_str.repeat(self.indent));
        }
        
        // Opening tag
        self.output.push('<');
        self.output.push_str(&elem.tag);
        
        // Attributes
        for (key, value) in &elem.attributes {
            self.output.push(' ');
            self.output.push_str(key);
            self.output.push_str("=\"");
            self.output.push_str(&self.escape(value));
            self.output.push('"');
        }
        
        // Check if empty
        if elem.children.is_empty() && elem.text.is_none() {
            self.output.push_str("/>");
            if self.pretty {
                self.output.push('\n');
            }
            return;
        }
        
        self.output.push('>');
        
        // Text content
        if let Some(ref text) = elem.text {
            self.output.push_str(&self.escape(text));
        }
        
        // Children
        let has_element_children = elem.children.iter().any(|n| matches!(n, Node::Element(_)));
        
        if has_element_children && self.pretty {
            self.output.push('\n');
        }
        
        self.indent += 1;
        for child in &elem.children {
            self.write_node(child);
        }
        self.indent -= 1;
        
        // Closing tag
        if has_element_children && self.pretty {
            self.output.push_str(&self.indent_str.repeat(self.indent));
        }
        self.output.push_str("</");
        self.output.push_str(&elem.tag);
        self.output.push('>');
        if self.pretty {
            self.output.push('\n');
        }
    }
    
    fn write_node(&mut self, node: &Node) {
        match node {
            Node::Element(e) => self.write_element_inner(e),
            Node::Text(t) => {
                if self.pretty && !t.trim().is_empty() {
                    self.output.push_str(&self.indent_str.repeat(self.indent));
                }
                self.output.push_str(&self.escape(t));
                if self.pretty {
                    self.output.push('\n');
                }
            }
            Node::Comment(c) => {
                if self.pretty {
                    self.output.push_str(&self.indent_str.repeat(self.indent));
                }
                self.output.push_str("<!--");
                self.output.push_str(c);
                self.output.push_str("-->");
                if self.pretty {
                    self.output.push('\n');
                }
            }
            Node::CData(d) => {
                self.output.push_str("<![CDATA[");
                self.output.push_str(d);
                self.output.push_str("]]>");
            }
            Node::ProcessingInstruction(target, data) => {
                if self.pretty {
                    self.output.push_str(&self.indent_str.repeat(self.indent));
                }
                self.output.push_str("<?");
                self.output.push_str(target);
                if !data.is_empty() {
                    self.output.push(' ');
                    self.output.push_str(data);
                }
                self.output.push_str("?>");
                if self.pretty {
                    self.output.push('\n');
                }
            }
        }
    }
    
    fn escape(&self, s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Parse an XML string.
pub fn parse(xml: &str) -> Result<Document, ParseError> {
    Parser::new(xml).parse()
}

/// Parse an XML file.
pub fn parse_file(path: &str) -> Result<Document, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse(&content)?)
}

/// Write a document to a string.
pub fn to_string(doc: &Document) -> String {
    Writer::new().write_document(doc)
}

/// Write a document to a string (compact).
pub fn to_string_compact(doc: &Document) -> String {
    Writer::compact().write_document(doc)
}

/// Write a document to a file.
pub fn write_file(doc: &Document, path: &str) -> std::io::Result<()> {
    let content = to_string(doc);
    std::fs::write(path, content)
}

// =============================================================================
// Element Tree Builder (like Python's ElementTree)
// =============================================================================

/// Build an element tree using a builder pattern.
pub struct ElementBuilder {
    elem: Element,
}

impl ElementBuilder {
    /// Create a new builder.
    pub fn new(tag: &str) -> Self {
        Self {
            elem: Element::new(tag),
        }
    }
    
    /// Add an attribute.
    pub fn attr(mut self, key: &str, value: &str) -> Self {
        self.elem.set(key, value);
        self
    }
    
    /// Add text content.
    pub fn text(mut self, text: &str) -> Self {
        self.elem.text = Some(text.to_string());
        self
    }
    
    /// Add a child element.
    pub fn child(mut self, builder: ElementBuilder) -> Self {
        self.elem.append_element(builder.build());
        self
    }
    
    /// Add a child element directly.
    pub fn element(mut self, elem: Element) -> Self {
        self.elem.append_element(elem);
        self
    }
    
    /// Build the element.
    pub fn build(self) -> Element {
        self.elem
    }
}

/// Create an element builder.
pub fn element(tag: &str) -> ElementBuilder {
    ElementBuilder::new(tag)
}

/// Create a sub-element under a parent.
pub fn sub_element<'a>(parent: &'a mut Element, tag: &str) -> &'a mut Element {
    parent.append_element(Element::new(tag));
    if let Some(Node::Element(e)) = parent.children.last_mut() {
        e
    } else {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple() {
        let xml = r#"<?xml version="1.0"?><root><child attr="value">text</child></root>"#;
        let doc = parse(xml).unwrap();
        
        let root = doc.root.unwrap();
        assert_eq!(root.tag, "root");
        assert_eq!(root.children.len(), 1);
        
        let child = root.find("child").unwrap();
        assert_eq!(child.get("attr"), Some("value"));
        assert_eq!(child.text_content(), "text");
    }
    
    #[test]
    fn test_parse_nested() {
        let xml = r#"<a><b><c/></b></a>"#;
        let doc = parse(xml).unwrap();
        
        let a = doc.root.unwrap();
        assert_eq!(a.tag, "a");
        
        let b = a.find("b").unwrap();
        assert_eq!(b.tag, "b");
        
        let c = b.find("c").unwrap();
        assert_eq!(c.tag, "c");
    }
    
    #[test]
    fn test_parse_attributes() {
        let xml = r#"<elem a="1" b="2" c="3"/>"#;
        let doc = parse(xml).unwrap();
        
        let elem = doc.root.unwrap();
        assert_eq!(elem.get("a"), Some("1"));
        assert_eq!(elem.get("b"), Some("2"));
        assert_eq!(elem.get("c"), Some("3"));
    }
    
    #[test]
    fn test_parse_entities() {
        let xml = r#"<elem>&lt;hello&gt; &amp; &quot;world&quot;</elem>"#;
        let doc = parse(xml).unwrap();
        
        let elem = doc.root.unwrap();
        assert_eq!(elem.text_content(), "<hello> & \"world\"");
    }
    
    #[test]
    fn test_write() {
        let doc = Document::with_root(
            element("root")
                .attr("id", "1")
                .child(element("child").text("hello"))
                .build()
        );
        
        let xml = to_string_compact(&doc);
        assert!(xml.contains("<root"));
        assert!(xml.contains("id=\"1\""));
        assert!(xml.contains("<child>hello</child>"));
    }
    
    #[test]
    fn test_builder() {
        let root = element("html")
            .child(element("head")
                .child(element("title").text("Hello")))
            .child(element("body")
                .attr("class", "main")
                .child(element("p").text("World")))
            .build();
        
        assert_eq!(root.tag, "html");
        assert!(root.find("head").is_some());
        assert!(root.find("body").is_some());
    }
    
    #[test]
    fn test_cdata() {
        let xml = r#"<root><![CDATA[<not>xml</not>]]></root>"#;
        let doc = parse(xml).unwrap();
        
        let root = doc.root.unwrap();
        if let Some(Node::CData(data)) = root.children.first() {
            assert_eq!(data, "<not>xml</not>");
        } else {
            panic!("Expected CDATA");
        }
    }
    
    #[test]
    fn test_comment() {
        let xml = r#"<root><!-- comment --></root>"#;
        let doc = parse(xml).unwrap();
        
        let root = doc.root.unwrap();
        if let Some(Node::Comment(c)) = root.children.first() {
            assert_eq!(c.trim(), "comment");
        } else {
            panic!("Expected comment");
        }
    }
}

