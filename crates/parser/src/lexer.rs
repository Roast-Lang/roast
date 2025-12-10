//! Lexer for Roast source code.
//!
//! The lexer handles Python-style indentation-based scoping,
//! producing INDENT and DEDENT tokens as appropriate.

use crate::token::{keyword_to_token, Token, TokenKind};
use roast_common::{FileId, Interner, SourceFile, Span};
use std::str::Chars;

/// The Roast lexer.
pub struct Lexer<'src> {
    source: &'src str,
    chars: Chars<'src>,
    file_id: FileId,
    interner: &'src Interner,

    // Position tracking
    pos: u32,
    line: u32,
    col: u32,

    // Indentation tracking
    indent_stack: Vec<u32>,
    pending_dedents: u32,
    at_line_start: bool,

    // Paren/bracket nesting (implicit line continuation)
    nesting: u32,

    // Pending tokens
    pending_tokens: Vec<Token>,

    // Current character
    current: Option<char>,

    // EOF emitted
    eof_emitted: bool,
}

impl<'src> Lexer<'src> {
    /// Creates a new lexer for the given source file.
    pub fn new(source_file: &'src SourceFile, interner: &'src Interner) -> Self {
        let source = source_file.source();
        let mut chars = source.chars();
        let current = chars.next();

        Self {
            source,
            chars,
            file_id: source_file.id,
            interner,
            pos: 0,
            line: 1,
            col: 1,
            indent_stack: vec![0],
            pending_dedents: 0,
            at_line_start: true,
            nesting: 0,
            pending_tokens: Vec::new(),
            current,
            eof_emitted: false,
        }
    }

    /// Peeks at the current character.
    fn peek(&self) -> Option<char> {
        self.current
    }

    /// Peeks at the next character (one ahead of current).
    fn peek_next(&self) -> Option<char> {
        self.chars.clone().next()
    }

    /// Advances to the next character.
    fn advance(&mut self) -> Option<char> {
        let c = self.current?;
        self.pos += c.len_utf8() as u32;
        self.current = self.chars.next();

        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }

        Some(c)
    }

    /// Creates a span from start to current position.
    fn span_from(&self, start: u32) -> Span {
        Span::new(self.file_id, start, self.pos)
    }

    /// Creates a token.
    fn token(&self, kind: TokenKind, start: u32) -> Token {
        Token::new(kind, self.span_from(start))
    }

    /// Skips whitespace (but not newlines outside of nesting).
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\x0c' => {
                    self.advance();
                }
                '\\' if self.peek_next() == Some('\n') => {
                    // Line continuation
                    self.advance(); // backslash
                    self.advance(); // newline
                }
                _ => break,
            }
        }
    }

    /// Skips a comment (from # to end of line).
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    /// Handles indentation at the start of a line.
    fn handle_indentation(&mut self) -> Option<Token> {
        if !self.at_line_start || self.nesting > 0 {
            return None;
        }

        let start = self.pos;
        let mut indent = 0u32;

        // Count leading spaces/tabs
        loop {
            match self.peek() {
                Some(' ') => {
                    indent += 1;
                    self.advance();
                }
                Some('\t') => {
                    // Tabs count as 8 spaces (aligned to 8)
                    indent = (indent / 8 + 1) * 8;
                    self.advance();
                }
                Some('#') => {
                    // Comment-only line, skip it and restart on next line
                    self.skip_comment();
                    if self.peek() == Some('\n') {
                        self.advance();
                    }
                    // Reset for next line and restart indentation handling
                    self.at_line_start = true;
                    indent = 0;
                    continue;  // Restart the loop for the next line
                }
                Some('\n') | Some('\r') => {
                    // Blank line, skip it and restart on next line
                    self.advance();
                    if self.peek() == Some('\n') {
                        self.advance();
                    }
                    // Reset for next line and restart indentation handling
                    self.at_line_start = true;
                    indent = 0;
                    continue;  // Restart the loop for the next line
                }
                _ => break,
            }
        }

        self.at_line_start = false;

        let current_indent = *self.indent_stack.last().unwrap();

        if indent > current_indent {
            self.indent_stack.push(indent);
            return Some(self.token(TokenKind::Indent, start));
        } else if indent < current_indent {
            // Generate DEDENT tokens
            while let Some(&top) = self.indent_stack.last() {
                if top <= indent {
                    break;
                }
                self.indent_stack.pop();
                self.pending_dedents += 1;
            }

            if *self.indent_stack.last().unwrap() != indent {
                return Some(self.token(
                    TokenKind::Error("inconsistent indentation".to_string()),
                    start,
                ));
            }

            if self.pending_dedents > 0 {
                self.pending_dedents -= 1;
                return Some(self.token(TokenKind::Dedent, start));
            }
        }

        None
    }

    /// Scans a number literal.
    fn scan_number(&mut self) -> Token {
        let start = self.pos;
        let mut s = String::new();
        let mut is_float = false;

        // Check for hex, octal, or binary
        if self.peek() == Some('0') {
            s.push(self.advance().unwrap());
            match self.peek() {
                Some('x') | Some('X') => {
                    s.push(self.advance().unwrap());
                    while let Some(c) = self.peek() {
                        if c.is_ascii_hexdigit() || c == '_' {
                            if c != '_' {
                                s.push(c);
                            }
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let value = i128::from_str_radix(&s[2..], 16).unwrap_or(0);
                    return self.token(TokenKind::Int(value), start);
                }
                Some('o') | Some('O') => {
                    s.push(self.advance().unwrap());
                    while let Some(c) = self.peek() {
                        if ('0'..='7').contains(&c) || c == '_' {
                            if c != '_' {
                                s.push(c);
                            }
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let value = i128::from_str_radix(&s[2..], 8).unwrap_or(0);
                    return self.token(TokenKind::Int(value), start);
                }
                Some('b') | Some('B') => {
                    s.push(self.advance().unwrap());
                    while let Some(c) = self.peek() {
                        if c == '0' || c == '1' || c == '_' {
                            if c != '_' {
                                s.push(c);
                            }
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let value = i128::from_str_radix(&s[2..], 2).unwrap_or(0);
                    return self.token(TokenKind::Int(value), start);
                }
                _ => {}
            }
        }

        // Regular decimal number
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                if c != '_' {
                    s.push(c);
                }
                self.advance();
            } else {
                break;
            }
        }

        // Decimal point
        if self.peek() == Some('.') && self.peek_next().map_or(false, |c| c.is_ascii_digit()) {
            is_float = true;
            s.push(self.advance().unwrap());
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    if c != '_' {
                        s.push(c);
                    }
                    self.advance();
                } else {
                    break;
                }
            }
        }

        // Exponent
        if matches!(self.peek(), Some('e') | Some('E')) {
            is_float = true;
            s.push(self.advance().unwrap());
            if matches!(self.peek(), Some('+') | Some('-')) {
                s.push(self.advance().unwrap());
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    if c != '_' {
                        s.push(c);
                    }
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if is_float {
            let value = s.parse::<f64>().unwrap_or(0.0);
            self.token(TokenKind::Float(value), start)
        } else {
            let value = s.parse::<i128>().unwrap_or(0);
            self.token(TokenKind::Int(value), start)
        }
    }

    /// Scans a string literal.
    fn scan_string(&mut self, quote: char) -> Token {
        let start = self.pos;
        self.advance(); // Opening quote

        // Check for triple-quoted string
        let triple = if self.peek() == Some(quote) && self.peek_next() == Some(quote) {
            self.advance();
            self.advance();
            true
        } else {
            false
        };

        let mut s = String::new();
        let mut closed = false;

        loop {
            match self.peek() {
                None => break,
                Some('\n') if !triple => {
                    return self.token(
                        TokenKind::Error("unterminated string literal".to_string()),
                        start,
                    );
                }
                Some(c) if c == quote => {
                    self.advance();
                    if triple {
                        if self.peek() == Some(quote) && self.peek_next() == Some(quote) {
                            self.advance();
                            self.advance();
                            closed = true;
                            break;
                        } else {
                            s.push(quote);
                        }
                    } else {
                        closed = true;
                        break;
                    }
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => {
                            s.push('\n');
                            self.advance();
                        }
                        Some('r') => {
                            s.push('\r');
                            self.advance();
                        }
                        Some('t') => {
                            s.push('\t');
                            self.advance();
                        }
                        Some('\\') => {
                            s.push('\\');
                            self.advance();
                        }
                        Some('\'') => {
                            s.push('\'');
                            self.advance();
                        }
                        Some('"') => {
                            s.push('"');
                            self.advance();
                        }
                        Some('0') => {
                            s.push('\0');
                            self.advance();
                        }
                        Some('\n') => {
                            self.advance(); // Line continuation
                        }
                        Some(c) => {
                            s.push('\\');
                            s.push(c);
                            self.advance();
                        }
                        None => break,
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.advance();
                }
            }
        }

        if !closed {
            return self.token(
                TokenKind::Error("unterminated string literal".to_string()),
                start,
            );
        }

        self.token(TokenKind::String(s), start)
    }

    /// Scans an f-string literal (format string).
    /// F-strings like f"Hello {name}!" are parsed into FString token
    /// which contains the raw parts and expression placeholders.
    fn scan_fstring(&mut self) -> Token {
        let start = self.pos;
        let quote = self.advance().unwrap(); // Opening quote

        // We tokenize f-strings as a special FString token that contains
        // a list of (literal_part, expression_part) pairs
        let mut parts: Vec<(String, String)> = Vec::new();
        let mut current_literal = String::new();
        let mut closed = false;

        loop {
            match self.peek() {
                None | Some('\n') => {
                    return self.token(
                        TokenKind::Error("unterminated f-string".to_string()),
                        start,
                    );
                }
                Some(c) if c == quote => {
                    self.advance();
                    closed = true;
                    break;
                }
                Some('{') => {
                    self.advance();
                    // Check for escaped brace {{
                    if self.peek() == Some('{') {
                        self.advance();
                        current_literal.push('{');
                        continue;
                    }
                    // Parse expression inside {}
                    let mut expr = String::new();
                    let mut brace_depth = 1;
                    while brace_depth > 0 {
                        match self.peek() {
                            None => {
                                return self.token(
                                    TokenKind::Error("unterminated f-string expression".to_string()),
                                    start,
                                );
                            }
                            Some('{') => {
                                brace_depth += 1;
                                expr.push(self.advance().unwrap());
                            }
                            Some('}') => {
                                brace_depth -= 1;
                                if brace_depth > 0 {
                                    expr.push(self.advance().unwrap());
                                } else {
                                    self.advance(); // consume closing }
                                }
                            }
                            Some(c) => {
                                expr.push(self.advance().unwrap());
                            }
                        }
                    }
                    parts.push((current_literal.clone(), expr));
                    current_literal.clear();
                }
                Some('}') => {
                    self.advance();
                    // Check for escaped brace }}
                    if self.peek() == Some('}') {
                        self.advance();
                        current_literal.push('}');
                    } else {
                        current_literal.push('}');
                    }
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => { current_literal.push('\n'); self.advance(); }
                        Some('t') => { current_literal.push('\t'); self.advance(); }
                        Some('r') => { current_literal.push('\r'); self.advance(); }
                        Some('\\') => { current_literal.push('\\'); self.advance(); }
                        Some('"') => { current_literal.push('"'); self.advance(); }
                        Some('\'') => { current_literal.push('\''); self.advance(); }
                        Some(c) => { current_literal.push('\\'); current_literal.push(c); self.advance(); }
                        None => {}
                    }
                }
                Some(c) => {
                    current_literal.push(c);
                    self.advance();
                }
            }
        }

        // Add final literal part (might be empty)
        if !current_literal.is_empty() || parts.is_empty() {
            parts.push((current_literal, String::new()));
        }

        if !closed {
            return self.token(
                TokenKind::Error("unterminated f-string".to_string()),
                start,
            );
        }

        self.token(TokenKind::FString(parts), start)
    }

    /// Scans an identifier or keyword.
    fn scan_identifier(&mut self) -> Token {
        let start = self.pos;
        let mut s = String::new();

        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c > '\x7f' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Check for keyword
        if let Some(kw) = keyword_to_token(&s) {
            return self.token(kw, start);
        }

        let sym = self.interner.intern(&s);
        self.token(TokenKind::Name(sym), start)
    }

    /// Scans the next token.
    fn scan_token(&mut self) -> Option<Token> {
        // Return pending dedents first
        if self.pending_dedents > 0 {
            self.pending_dedents -= 1;
            return Some(Token::new(TokenKind::Dedent, Span::new(self.file_id, self.pos, self.pos)));
        }

        // Return pending tokens
        if let Some(tok) = self.pending_tokens.pop() {
            return Some(tok);
        }

        // Handle indentation at line start
        if let Some(tok) = self.handle_indentation() {
            return Some(tok);
        }

        self.skip_whitespace();

        // Skip comments
        if self.peek() == Some('#') {
            self.skip_comment();
        }

        let start = self.pos;
        let c = self.peek()?;

        // Newline
        if c == '\n' || c == '\r' {
            self.advance();
            if c == '\r' && self.peek() == Some('\n') {
                self.advance();
            }
            self.at_line_start = true;
            if self.nesting == 0 {
                return Some(self.token(TokenKind::Newline, start));
            } else {
                return self.scan_token(); // Skip newline inside brackets
            }
        }

        // Numbers
        if c.is_ascii_digit() {
            return Some(self.scan_number());
        }

        // Strings
        if c == '"' || c == '\'' {
            return Some(self.scan_string(c));
        }

        // Raw strings, byte strings, f-strings
        if matches!(c, 'r' | 'R' | 'b' | 'B' | 'f' | 'F') {
            let next = self.peek_next();
            if matches!(next, Some('"') | Some('\'')) {
                // Handle f-strings specially
                if matches!(c, 'f' | 'F') {
                    self.advance(); // consume 'f'
                    return Some(self.scan_fstring());
                }
                // TODO: Handle other string prefixes (r, b) properly
                self.advance();
                return Some(self.scan_string(next.unwrap()));
            }
            if matches!(c, 'r' | 'R' | 'b' | 'B') {
                let next2 = self.chars.clone().nth(1);
                if matches!(self.peek_next(), Some('b' | 'B' | 'r' | 'R'))
                    && matches!(next2, Some('"') | Some('\''))
                {
                    self.advance();
                    self.advance();
                    return Some(self.scan_string(next2.unwrap()));
                }
            }
        }

        // Identifiers and keywords (and f-strings)
        if c.is_alphabetic() || c == '_' || c > '\x7f' {
            // Check for f-string prefix - peek() returns current char, peek_next() returns the next one
            // At this point, c == peek() == 'f', so we need peek_next() to be the quote
            if (c == 'f' || c == 'F') && matches!(self.peek_next(), Some('"') | Some('\'')) {
                self.advance(); // consume 'f'
                return Some(self.scan_fstring());
            }
            return Some(self.scan_identifier());
        }

        // Operators and delimiters
        self.advance();
        let kind = match c {
            '+' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PlusEqual
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::MinusEqual
                } else if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                if self.peek() == Some('*') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::DoubleStarEqual
                    } else {
                        TokenKind::DoubleStar
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::StarEqual
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                if self.peek() == Some('/') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::DoubleSlashEqual
                    } else {
                        TokenKind::DoubleSlash
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::SlashEqual
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PercentEqual
                } else {
                    TokenKind::Percent
                }
            }
            '@' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::AtEqual
                } else {
                    TokenKind::At
                }
            }
            '&' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::AmpersandEqual
                } else {
                    TokenKind::Ampersand
                }
            }
            '|' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PipeEqual
                } else {
                    TokenKind::Pipe
                }
            }
            '^' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::CaretEqual
                } else {
                    TokenKind::Caret
                }
            }
            '~' => TokenKind::Tilde,
            '<' => {
                if self.peek() == Some('<') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::LeftShiftEqual
                    } else {
                        TokenKind::LeftShift
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }
            '>' => {
                if self.peek() == Some('>') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::RightShiftEqual
                    } else {
                        TokenKind::RightShift
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::EqualEqual
                } else {
                    TokenKind::Equal
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::NotEqual
                } else {
                    TokenKind::Error(format!("unexpected character: {}", c))
                }
            }
            ':' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::ColonEqual
                } else {
                    TokenKind::Colon
                }
            }
            '(' => {
                self.nesting += 1;
                TokenKind::LeftParen
            }
            ')' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::RightParen
            }
            '[' => {
                self.nesting += 1;
                TokenKind::LeftBracket
            }
            ']' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::RightBracket
            }
            '{' => {
                self.nesting += 1;
                TokenKind::LeftBrace
            }
            '}' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::RightBrace
            }
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semi,
            '.' => {
                if self.peek() == Some('.') && self.peek_next() == Some('.') {
                    self.advance();
                    self.advance();
                    TokenKind::Ellipsis
                } else {
                    TokenKind::Dot
                }
            }
            '?' => TokenKind::Question,
            _ => TokenKind::Error(format!("unexpected character: {}", c)),
        };

        Some(self.token(kind, start))
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tok) = self.scan_token() {
            return Some(tok);
        }

        // At EOF, emit remaining DEDENTs
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            return Some(Token::new(
                TokenKind::Dedent,
                Span::new(self.file_id, self.pos, self.pos),
            ));
        }

        // Emit EOF once
        if !self.eof_emitted {
            self.eof_emitted = true;
            return Some(Token::new(
                TokenKind::Eof,
                Span::new(self.file_id, self.pos, self.pos),
            ));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roast_common::FileId;

    fn lex(source: &str) -> Vec<TokenKind> {
        let interner = Interner::new();
        let file = SourceFile::new(FileId::new(0), "test.roast".to_string(), source.to_string());
        let lexer = Lexer::new(&file, &interner);
        lexer.map(|t| t.kind).collect()
    }

    #[test]
    fn test_numbers() {
        let tokens = lex("42 3.14 0xff 0b1010");
        assert!(matches!(tokens[0], TokenKind::Int(42)));
        assert!(matches!(tokens[1], TokenKind::Float(f) if (f - 3.14).abs() < 0.001));
        assert!(matches!(tokens[2], TokenKind::Int(255)));
        assert!(matches!(tokens[3], TokenKind::Int(10)));
    }

    #[test]
    fn test_strings() {
        let tokens = lex(r#""hello" 'world'"#);
        assert!(matches!(&tokens[0], TokenKind::String(s) if s == "hello"));
        assert!(matches!(&tokens[1], TokenKind::String(s) if s == "world"));
    }

    #[test]
    fn test_operators() {
        let tokens = lex("+ - * ** / // % @ << >> & | ^ ~ < > <= >= == != := ->");
        assert_eq!(tokens[0], TokenKind::Plus);
        assert_eq!(tokens[1], TokenKind::Minus);
        assert_eq!(tokens[2], TokenKind::Star);
        assert_eq!(tokens[3], TokenKind::DoubleStar);
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("def class if else for while");
        assert_eq!(tokens[0], TokenKind::Def);
        assert_eq!(tokens[1], TokenKind::Class);
        assert_eq!(tokens[2], TokenKind::If);
        assert_eq!(tokens[3], TokenKind::Else);
        assert_eq!(tokens[4], TokenKind::For);
        assert_eq!(tokens[5], TokenKind::While);
    }

    #[test]
    fn test_indentation() {
        let tokens = lex("if x:\n    y\n    z");
        assert!(tokens.contains(&TokenKind::Indent));
    }
}
