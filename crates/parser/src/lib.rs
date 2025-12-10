//! Lexer and parser for the Roast language.
//!
//! This crate provides a complete frontend for parsing Roast source code
//! into an Abstract Syntax Tree (AST).

pub mod lexer;
pub mod parser;
pub mod token;

pub use lexer::Lexer;
pub use parser::{ParseError, Parser};
pub use token::{Token, TokenKind};

use roast_ast::Module;
use roast_common::{DiagnosticSink, FileId, Interner, SourceFile};

/// Parse a string of Roast source code into a module.
pub fn parse_module(
    source: &str,
    file_name: &str,
    interner: &Interner,
    diagnostics: &mut DiagnosticSink,
) -> Result<Module, ParseError> {
    let file_id = FileId::new(0);
    let source_file = SourceFile::new(file_id, file_name.to_string(), source.to_string());
    
    let lexer = Lexer::new(&source_file, interner);
    let tokens: Vec<_> = lexer.collect();
    
    let mut parser = Parser::new(&tokens, &source_file, interner, diagnostics);
    parser.parse_module(file_name.to_string())
}

/// Parse a single expression from source.
pub fn parse_expression(
    source: &str,
    interner: &Interner,
    diagnostics: &mut DiagnosticSink,
) -> Result<roast_ast::Expr, ParseError> {
    let file_id = FileId::new(0);
    let source_file = SourceFile::new(file_id, "<expr>".to_string(), source.to_string());
    
    let lexer = Lexer::new(&source_file, interner);
    let tokens: Vec<_> = lexer.collect();
    
    let mut parser = Parser::new(&tokens, &source_file, interner, diagnostics);
    parser.parse_expression()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let interner = Interner::new();
        let mut diagnostics = DiagnosticSink::new();
        
        let result = parse_module(
            "x = 42",
            "test.roast",
            &interner,
            &mut diagnostics,
        );
        
        assert!(result.is_ok());
    }
}

