//! Recursive descent parser for Roast.

use crate::token::{Token, TokenKind};
use roast_ast::*;
use roast_common::{Diagnostic, DiagnosticSink, Interner, SourceFile, Span, Symbol};
use num_bigint::BigInt;
use thiserror::Error;

/// Parser error type.
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("unexpected token: expected {expected}, found {found}")]
    UnexpectedToken {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("unexpected end of file")]
    UnexpectedEof { span: Span },

    #[error("{message}")]
    Custom { message: String, span: Span },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            ParseError::UnexpectedToken { span, .. } => *span,
            ParseError::UnexpectedEof { span } => *span,
            ParseError::Custom { span, .. } => *span,
        }
    }
}

/// The Roast parser.
pub struct Parser<'a> {
    tokens: &'a [Token],
    source: &'a SourceFile,
    interner: &'a Interner,
    diagnostics: &'a mut DiagnosticSink,
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Creates a new parser.
    pub fn new(
        tokens: &'a [Token],
        source: &'a SourceFile,
        interner: &'a Interner,
        diagnostics: &'a mut DiagnosticSink,
    ) -> Self {
        Self {
            tokens,
            source,
            interner,
            diagnostics,
            pos: 0,
        }
    }

    // ========== Token access ==========

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens.last().expect("token stream should not be empty")
        })
    }

    fn peek(&self) -> &TokenKind {
        &self.current().kind
    }

    fn peek_ahead(&self, n: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + n).map(|t| &t.kind)
    }

    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.iter().any(|k| self.at(k))
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        let old_pos = self.pos;
        if !self.at_eof() {
            self.pos += 1;
        }
        self.tokens.get(old_pos).unwrap_or_else(|| {
            self.tokens.last().expect("token stream should not be empty")
        })
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<&Token, ParseError> {
        if self.at(kind) {
            Ok(self.advance())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: format!("{}", kind),
                found: format!("{}", self.peek()),
                span: self.current().span,
            })
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn span_from(&self, start: Span) -> Span {
        if self.pos > 0 {
            start.merge(self.tokens[self.pos - 1].span)
        } else {
            start
        }
    }

    // ========== Module parsing ==========

    /// Parses a complete module.
    pub fn parse_module(&mut self, name: String) -> Result<Module, ParseError> {
        let start = self.current().span;
        let mut body = Vec::new();

        self.skip_newlines();

        while !self.at_eof() {
            match self.parse_statement() {
                Ok(stmt) => body.push(stmt),
                Err(e) => {
                    self.diagnostics.report(
                        Diagnostic::error(e.to_string())
                            .with_span(e.span())
                    );
                    // Try to recover
                    self.synchronize();
                }
            }
            self.skip_newlines();
        }

        Ok(Module::new(
            self.source.id,
            name,
            body,
            self.span_from(start),
        ))
    }

    /// Synchronize after an error.
    fn synchronize(&mut self) {
        while !self.at_eof() {
            if matches!(self.peek(), TokenKind::Newline) {
                self.advance();
                return;
            }
            if self.peek().can_start_stmt() {
                return;
            }
            self.advance();
        }
    }

    // ========== Statement parsing ==========

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        
        let start = self.current().span;

        // Handle decorators
        if matches!(self.peek(), TokenKind::At) {
            return self.parse_decorated();
        }

        match self.peek() {
            TokenKind::Def => self.parse_function_def(Vec::new(), false),
            TokenKind::Async => {
                self.advance();
                if matches!(self.peek(), TokenKind::Def) {
                    self.parse_function_def(Vec::new(), true)
                } else if matches!(self.peek(), TokenKind::For) {
                    self.parse_for_stmt(true)
                } else if matches!(self.peek(), TokenKind::With) {
                    self.parse_with_stmt(true)
                } else {
                    Err(ParseError::UnexpectedToken {
                        expected: "def, for, or with after async".to_string(),
                        found: format!("{}", self.peek()),
                        span: self.current().span,
                    })
                }
            }
            TokenKind::Class => self.parse_class_def(Vec::new()),
            TokenKind::If => self.parse_if_stmt(),
            TokenKind::For => self.parse_for_stmt(false),
            TokenKind::While => self.parse_while_stmt(),
            TokenKind::Try => self.parse_try_stmt(),
            TokenKind::With => self.parse_with_stmt(false),
            TokenKind::Match => self.parse_match_stmt(),
            TokenKind::Return => self.parse_return_stmt(),
            TokenKind::Raise => self.parse_raise_stmt(),
            TokenKind::Break => {
                self.advance();
                Ok(Stmt::new(StmtKind::Break, self.span_from(start)))
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Stmt::new(StmtKind::Continue, self.span_from(start)))
            }
            TokenKind::Pass => {
                self.advance();
                Ok(Stmt::new(StmtKind::Pass, self.span_from(start)))
            }
            TokenKind::Import => self.parse_import_stmt(),
            TokenKind::From => self.parse_from_import_stmt(),
            TokenKind::Global => self.parse_global_stmt(),
            TokenKind::Nonlocal => self.parse_nonlocal_stmt(),
            TokenKind::Assert => self.parse_assert_stmt(),
            TokenKind::Del => self.parse_del_stmt(),
            TokenKind::Type => self.parse_type_alias(),
            _ => self.parse_expr_or_assign_stmt(),
        }
    }

    fn parse_decorated(&mut self) -> Result<Stmt, ParseError> {
        let mut decorators = Vec::new();

        while matches!(self.peek(), TokenKind::At) {
            let start = self.current().span;
            self.advance(); // @

            // Parse the decorator expression including attribute access (e.g., @app.route)
            // Use parse_unary_postfix to handle cases like @app.route("/path")
            let name = self.parse_unary_postfix()?;
            let mut arguments = Vec::new();
            let mut keywords = Vec::new();

            // Check if we need to parse additional arguments
            // Note: if the decorator was already a call (e.g., @app.route("/path")),
            // then parse_unary_postfix already consumed the arguments
            if let ExprKind::Call { func, args, keywords: kws } = &name.kind {
                // The decorator was a call expression, extract the function and args
                decorators.push(Decorator {
                    name: (**func).clone(),
                    arguments: args.clone(),
                    keywords: kws.clone(),
                    span: self.span_from(start),
                });
            } else {
                // Simple decorator without call, or attribute access like @app.route
                // Check if there's a call after it
                if matches!(self.peek(), TokenKind::LeftParen) {
                    self.advance();
                    // Parse decorator arguments
                    while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
                        if matches!(self.peek(), TokenKind::Name(_)) {
                            if let Some(TokenKind::Equal) = self.peek_ahead(1) {
                                // Keyword argument
                                let kw_name = self.parse_ident()?;
                                self.advance(); // =
                                let value = self.parse_expression()?;
                                keywords.push(Keyword {
                                    name: Some(kw_name),
                                    value,
                                    span: self.span_from(start),
                                });
                            } else {
                                arguments.push(self.parse_expression()?);
                            }
                        } else {
                            arguments.push(self.parse_expression()?);
                        }
                        if !matches!(self.peek(), TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                }

                decorators.push(Decorator {
                    name,
                    arguments,
                    keywords,
                    span: self.span_from(start),
                });
            }

            self.skip_newlines();
        }

        // Parse the decorated definition
        match self.peek() {
            TokenKind::Def => self.parse_function_def(decorators, false),
            TokenKind::Async => {
                self.advance();
                self.parse_function_def(decorators, true)
            }
            TokenKind::Class => self.parse_class_def(decorators),
            _ => Err(ParseError::UnexpectedToken {
                expected: "def or class after decorator".to_string(),
                found: format!("{}", self.peek()),
                span: self.current().span,
            }),
        }
    }

    fn parse_function_def(&mut self, decorators: Vec<Decorator>, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Def)?;

        let name = self.parse_ident()?;

        // Type parameters
        let type_params = if matches!(self.peek(), TokenKind::LeftBracket) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::LeftParen)?;
        let args = self.parse_parameters()?;
        self.expect(&TokenKind::RightParen)?;

        let returns = if matches!(self.peek(), TokenKind::Arrow) {
            self.advance();
            Some(Box::new(self.parse_type_expr()?))
        } else {
            None
        };

        // Parse optional where clause: where T: Bound, U: OtherBound
        let where_clause = if matches!(self.peek(), TokenKind::Where) {
            Some(self.parse_where_clause()?)
        } else {
            None
        };

        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        Ok(Stmt::new(
            StmtKind::FunctionDef {
                name,
                args,
                body,
                decorators,
                returns,
                type_params,
                where_clause,
                is_async,
            },
            self.span_from(start),
        ))
    }

    fn parse_parameters(&mut self) -> Result<Arguments, ParseError> {
        let mut args = Arguments::new();
        let mut seen_default = false;
        let mut seen_star = false;
        let mut seen_double_star = false;

        while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
            // Handle *args
            if matches!(self.peek(), TokenKind::Star) {
                self.advance();
                if matches!(self.peek(), TokenKind::Name(_)) {
                    let arg = self.parse_parameter()?;
                    args.vararg = Some(Box::new(arg));
                }
                seen_star = true;
            }
            // Handle **kwargs
            else if matches!(self.peek(), TokenKind::DoubleStar) {
                self.advance();
                let arg = self.parse_parameter()?;
                args.kwarg = Some(Box::new(arg));
                seen_double_star = true;
            }
            // Regular parameter
            else if matches!(self.peek(), TokenKind::Name(_)) {
                let arg = self.parse_parameter()?;
                let has_default = arg.default.is_some();

                if seen_star {
                    args.kwonlyargs.push(arg);
                } else {
                    if has_default {
                        seen_default = true;
                    } else if seen_default && !seen_star {
                        return Err(ParseError::Custom {
                            message: "non-default argument follows default argument".to_string(),
                            span: self.current().span,
                        });
                    }
                    args.args.push(arg);
                }
            } else {
                break;
            }

            if !matches!(self.peek(), TokenKind::RightParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        Ok(args)
    }

    /// Parse lambda parameters - similar to parse_parameters but ends at `:` instead of `)`
    fn parse_lambda_parameters(&mut self) -> Result<Arguments, ParseError> {
        let mut args = Arguments::new();
        let mut seen_default = false;
        let mut seen_star = false;
        let mut seen_double_star = false;

        // Lambda parameters end at Colon, not RightParen
        while !matches!(self.peek(), TokenKind::Colon | TokenKind::Eof) {
            // Handle *args
            if matches!(self.peek(), TokenKind::Star) {
                self.advance();
                if matches!(self.peek(), TokenKind::Name(_)) {
                    let arg = self.parse_lambda_parameter()?;
                    args.vararg = Some(Box::new(arg));
                }
                seen_star = true;
            }
            // Handle **kwargs
            else if matches!(self.peek(), TokenKind::DoubleStar) {
                self.advance();
                let arg = self.parse_lambda_parameter()?;
                args.kwarg = Some(Box::new(arg));
                seen_double_star = true;
            }
            // Regular parameter
            else if matches!(self.peek(), TokenKind::Name(_)) {
                let arg = self.parse_lambda_parameter()?;
                let has_default = arg.default.is_some();

                if seen_star {
                    args.kwonlyargs.push(arg);
                } else {
                    if has_default {
                        seen_default = true;
                    } else if seen_default && !seen_star {
                        return Err(ParseError::Custom {
                            message: "non-default argument follows default argument".to_string(),
                            span: self.current().span,
                        });
                    }
                    args.args.push(arg);
                }
            } else {
                break;
            }

            // Comma separates parameters, but stop at colon
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(args)
    }

    /// Parse a lambda parameter - simpler than regular parameter (no type annotations with colon)
    fn parse_lambda_parameter(&mut self) -> Result<Arg, ParseError> {
        let start = self.current().span;
        let name = self.parse_ident()?;

        // Lambda parameters can have defaults but NOT type annotations (colon is ambiguous)
        let default = if matches!(self.peek(), TokenKind::Equal) {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        Ok(Arg {
            name,
            annotation: None,  // Lambda params don't have type annotations
            default,
            ownership: Ownership::None,
            span: self.span_from(start),
        })
    }

    fn parse_parameter(&mut self) -> Result<Arg, ParseError> {
        let start = self.current().span;
        
        // Check for ownership modifier
        let ownership = self.parse_ownership()?;
        
        let name = self.parse_ident()?;

        let annotation = if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            Some(Box::new(self.parse_type_expr()?))
        } else {
            None
        };

        let default = if matches!(self.peek(), TokenKind::Equal) {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        Ok(Arg {
            name,
            annotation,
            default,
            ownership,
            span: self.span_from(start),
        })
    }

    fn parse_ownership(&mut self) -> Result<Ownership, ParseError> {
        Ok(match self.peek() {
            TokenKind::Mut => {
                self.advance();
                Ownership::Mut
            }
            TokenKind::Imm => {
                self.advance();
                Ownership::Imm
            }
            TokenKind::Borrow => {
                self.advance();
                Ownership::Borrow
            }
            // TokenKind::Move removed - 'move' is no longer a reserved keyword (BUG-001)
            TokenKind::Own => {
                self.advance();
                Ownership::Own
            }
            TokenKind::Ref => {
                self.advance();
                Ownership::Borrow
            }
            _ => Ownership::None,
        })
    }

    fn parse_type_params(&mut self) -> Result<Vec<TypeParam>, ParseError> {
        let mut params = Vec::new();
        self.expect(&TokenKind::LeftBracket)?;

        while !matches!(self.peek(), TokenKind::RightBracket | TokenKind::Eof) {
            let start = self.current().span;
            
            let kind = if matches!(self.peek(), TokenKind::Star) {
                self.advance();
                if matches!(self.peek(), TokenKind::Star) {
                    self.advance();
                    TypeParamKind::ParamSpec
                } else {
                    TypeParamKind::TypeVarTuple
                }
            } else {
                TypeParamKind::TypeVar
            };

            let name = self.parse_ident()?;

            let bound = if matches!(self.peek(), TokenKind::Colon) {
                self.advance();
                Some(Box::new(self.parse_type_expr()?))
            } else {
                None
            };

            let default = if matches!(self.peek(), TokenKind::Equal) {
                self.advance();
                Some(Box::new(self.parse_type_expr()?))
            } else {
                None
            };

            params.push(TypeParam {
                name,
                bound,
                default,
                kind,
                span: self.span_from(start),
            });

            if !matches!(self.peek(), TokenKind::RightBracket) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        self.expect(&TokenKind::RightBracket)?;
        Ok(params)
    }

    /// Parse a where clause: `where T: Bound + OtherBound, U: Protocol`
    /// 
    /// Syntax example:
    /// ```text
    /// where TypeParam: Bound1 + Bound2, OtherTypeParam: Bound3
    /// ```
    fn parse_where_clause(&mut self) -> Result<WhereClause, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Where)?;

        let mut constraints = Vec::new();

        loop {
            let constraint_start = self.current().span;
            
            // Parse the type parameter name
            let type_param = self.parse_ident()?;
            
            // Expect a colon
            self.expect(&TokenKind::Colon)?;
            
            // Parse bounds separated by +
            let mut bounds = Vec::new();
            loop {
                bounds.push(self.parse_type_expr()?);
                
                if matches!(self.peek(), TokenKind::Plus) {
                    self.advance();
                } else {
                    break;
                }
            }
            
            constraints.push(WhereConstraint {
                type_param,
                bounds,
                span: self.span_from(constraint_start),
            });
            
            // Multiple constraints separated by comma
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(WhereClause {
            constraints,
            span: self.span_from(start),
        })
    }

    fn parse_class_def(&mut self, decorators: Vec<Decorator>) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Class)?;

        let name = self.parse_ident()?;

        let type_params = if matches!(self.peek(), TokenKind::LeftBracket) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        let mut bases = Vec::new();
        let mut keywords = Vec::new();

        if matches!(self.peek(), TokenKind::LeftParen) {
            self.advance();
            while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
                if matches!(self.peek(), TokenKind::Name(_)) {
                    if let Some(TokenKind::Equal) = self.peek_ahead(1) {
                        let kw_start = self.current().span;
                        let kw_name = self.parse_ident()?;
                        self.advance(); // =
                        let value = self.parse_expression()?;
                        keywords.push(Keyword {
                            name: Some(kw_name),
                            value,
                            span: self.span_from(kw_start),
                        });
                    } else {
                        bases.push(self.parse_expression()?);
                    }
                } else {
                    bases.push(self.parse_expression()?);
                }
                if !matches!(self.peek(), TokenKind::RightParen) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightParen)?;
        }

        // Parse optional where clause: where T: Bound, U: OtherBound
        let where_clause = if matches!(self.peek(), TokenKind::Where) {
            Some(self.parse_where_clause()?)
        } else {
            None
        };

        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        Ok(Stmt::new(
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                body,
                decorators,
                type_params,
                where_clause,
            },
            self.span_from(start),
        ))
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::If)?;

        let test = self.parse_expression()?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        let orelse = if matches!(self.peek(), TokenKind::Elif) {
            vec![self.parse_elif_stmt()?]
        } else if matches!(self.peek(), TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Stmt::new(
            StmtKind::If {
                test: Box::new(test),
                body,
                orelse,
            },
            self.span_from(start),
        ))
    }

    fn parse_elif_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Elif)?;

        let test = self.parse_expression()?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        let orelse = if matches!(self.peek(), TokenKind::Elif) {
            vec![self.parse_elif_stmt()?]
        } else if matches!(self.peek(), TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Stmt::new(
            StmtKind::If {
                test: Box::new(test),
                body,
                orelse,
            },
            self.span_from(start),
        ))
    }

    fn parse_for_stmt(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::For)?;

        let target = self.parse_target()?;
        
        // Roast extension: type annotation on loop variable
        let target_annotation = if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            Some(Box::new(self.parse_type_expr()?))
        } else {
            None
        };

        self.expect(&TokenKind::In)?;
        let iter = self.parse_expression()?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        let orelse = if matches!(self.peek(), TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Stmt::new(
            StmtKind::For {
                target: Box::new(target),
                iter: Box::new(iter),
                body,
                orelse,
                is_async,
                target_annotation,
            },
            self.span_from(start),
        ))
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::While)?;

        let test = self.parse_expression()?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        let orelse = if matches!(self.peek(), TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Stmt::new(
            StmtKind::While {
                test: Box::new(test),
                body,
                orelse,
            },
            self.span_from(start),
        ))
    }

    fn parse_try_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Try)?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        let mut handlers = Vec::new();
        while matches!(self.peek(), TokenKind::Except) {
            handlers.push(self.parse_except_handler()?);
        }

        let orelse = if matches!(self.peek(), TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };

        let finalbody = if matches!(self.peek(), TokenKind::Finally) {
            self.advance();
            self.expect(&TokenKind::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Stmt::new(
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            },
            self.span_from(start),
        ))
    }

    fn parse_except_handler(&mut self) -> Result<ExceptHandler, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Except)?;

        let ty = if !matches!(self.peek(), TokenKind::Colon) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        let name = if matches!(self.peek(), TokenKind::As) {
            self.advance();
            Some(self.parse_ident()?)
        } else {
            None
        };

        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        Ok(ExceptHandler {
            ty,
            name,
            body,
            span: self.span_from(start),
        })
    }

    fn parse_with_stmt(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::With)?;

        let mut items = Vec::new();
        loop {
            let item_start = self.current().span;
            let context_expr = self.parse_expression()?;
            let optional_vars = if matches!(self.peek(), TokenKind::As) {
                self.advance();
                Some(Box::new(self.parse_target()?))
            } else {
                None
            };
            items.push(WithItem {
                context_expr: Box::new(context_expr),
                optional_vars,
                span: self.span_from(item_start),
            });

            if !matches!(self.peek(), TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        Ok(Stmt::new(
            StmtKind::With {
                items,
                body,
                is_async,
            },
            self.span_from(start),
        ))
    }

    fn parse_match_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Match)?;

        let subject = self.parse_expression()?;
        self.expect(&TokenKind::Colon)?;
        self.skip_newlines();
        self.expect(&TokenKind::Indent)?;

        let mut cases = Vec::new();
        while matches!(self.peek(), TokenKind::Case) {
            cases.push(self.parse_match_case()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::Dedent)?;

        Ok(Stmt::new(
            StmtKind::Match {
                subject: Box::new(subject),
                cases,
            },
            self.span_from(start),
        ))
    }

    fn parse_match_case(&mut self) -> Result<MatchCase, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Case)?;

        let pattern = self.parse_pattern()?;

        let guard = if matches!(self.peek(), TokenKind::If) {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;

        Ok(MatchCase {
            pattern,
            guard,
            body,
            span: self.span_from(start),
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let start = self.current().span;
        let mut pattern = self.parse_pattern_primary()?;

        // Check for 'as' pattern
        if matches!(self.peek(), TokenKind::As) {
            self.advance();
            let name = self.parse_ident()?;
            pattern = Pattern::new(
                PatternKind::MatchAs {
                    pattern: Some(Box::new(pattern)),
                    name: Some(name),
                },
                self.span_from(start),
            );
        }

        // Check for '|' patterns
        if matches!(self.peek(), TokenKind::Pipe) {
            let mut patterns = vec![pattern];
            while matches!(self.peek(), TokenKind::Pipe) {
                self.advance();
                patterns.push(self.parse_pattern_primary()?);
            }
            pattern = Pattern::new(
                PatternKind::MatchOr { patterns },
                self.span_from(start),
            );
        }

        Ok(pattern)
    }

    fn parse_pattern_primary(&mut self) -> Result<Pattern, ParseError> {
        let start = self.current().span;

        match self.peek().clone() {
            TokenKind::Name(sym) if self.interner.resolve(sym) == Some("_") => {
                self.advance();
                Ok(Pattern::new(PatternKind::Wildcard, self.span_from(start)))
            }
            TokenKind::Name(_) => {
                let ident = self.parse_ident()?;
                // Could be a class pattern or capture
                if matches!(self.peek(), TokenKind::LeftParen) {
                    // Class pattern
                    self.advance();
                    let mut patterns = Vec::new();
                    let mut kwd_attrs = Vec::new();
                    let mut kwd_patterns = Vec::new();

                    while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
                        if matches!(self.peek(), TokenKind::Name(_)) {
                            if let Some(TokenKind::Equal) = self.peek_ahead(1) {
                                let attr = self.parse_ident()?;
                                self.advance(); // =
                                let pat = self.parse_pattern()?;
                                kwd_attrs.push(attr);
                                kwd_patterns.push(pat);
                            } else {
                                patterns.push(self.parse_pattern()?);
                            }
                        } else {
                            patterns.push(self.parse_pattern()?);
                        }
                        if !matches!(self.peek(), TokenKind::RightParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;

                    Ok(Pattern::new(
                        PatternKind::MatchClass {
                            cls: Box::new(Expr::new(
                                ExprKind::Name {
                                    id: ident.clone(),
                                    ctx: ExprContext::Load,
                                },
                                ident.span,
                            )),
                            patterns,
                            kwd_attrs,
                            kwd_patterns,
                        },
                        self.span_from(start),
                    ))
                } else {
                    // Capture pattern
                    Ok(Pattern::new(
                        PatternKind::Capture { name: ident },
                        self.span_from(start),
                    ))
                }
            }
            TokenKind::LeftBracket => {
                self.advance();
                let mut patterns = Vec::new();
                while !matches!(self.peek(), TokenKind::RightBracket | TokenKind::Eof) {
                    if matches!(self.peek(), TokenKind::Star) {
                        self.advance();
                        let name = if matches!(self.peek(), TokenKind::Name(_)) {
                            Some(self.parse_ident()?)
                        } else {
                            None
                        };
                        patterns.push(Pattern::new(
                            PatternKind::MatchStar { name },
                            self.span_from(start),
                        ));
                    } else {
                        patterns.push(self.parse_pattern()?);
                    }
                    if !matches!(self.peek(), TokenKind::RightBracket) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightBracket)?;
                Ok(Pattern::new(
                    PatternKind::MatchSequence { patterns },
                    self.span_from(start),
                ))
            }
            TokenKind::LeftBrace => {
                self.advance();
                let mut keys = Vec::new();
                let mut patterns = Vec::new();
                let mut rest = None;

                while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
                    if matches!(self.peek(), TokenKind::DoubleStar) {
                        self.advance();
                        rest = Some(self.parse_ident()?);
                    } else {
                        keys.push(self.parse_expression()?);
                        self.expect(&TokenKind::Colon)?;
                        patterns.push(self.parse_pattern()?);
                    }
                    if !matches!(self.peek(), TokenKind::RightBrace) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightBrace)?;
                Ok(Pattern::new(
                    PatternKind::MatchMapping {
                        keys,
                        patterns,
                        rest,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Int(_) | TokenKind::Float(_) | TokenKind::String(_) 
            | TokenKind::True | TokenKind::False | TokenKind::None => {
                let value = self.parse_primary()?;
                Ok(Pattern::new(
                    PatternKind::MatchValue {
                        value: Box::new(value),
                    },
                    self.span_from(start),
                ))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "pattern".to_string(),
                found: format!("{}", self.peek()),
                span: self.current().span,
            }),
        }
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Return)?;

        let value = if self.peek().can_start_expr() {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        Ok(Stmt::new(
            StmtKind::Return { value },
            self.span_from(start),
        ))
    }

    fn parse_raise_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Raise)?;

        let exc = if self.peek().can_start_expr() {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        let cause = if matches!(self.peek(), TokenKind::From) {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        Ok(Stmt::new(
            StmtKind::Raise { exc, cause },
            self.span_from(start),
        ))
    }

    fn parse_import_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Import)?;

        let mut names = Vec::new();
        loop {
            names.push(self.parse_alias()?);
            if !matches!(self.peek(), TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        Ok(Stmt::new(StmtKind::Import { names }, self.span_from(start)))
    }

    fn parse_from_import_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::From)?;

        let mut level = 0;
        while matches!(self.peek(), TokenKind::Dot) {
            self.advance();
            level += 1;
        }

        // Use dotted name for module path - e.g., `from test_modules.mathlib import func`
        let module = if matches!(self.peek(), TokenKind::Name(_)) {
            Some(self.parse_dotted_name()?)
        } else {
            None
        };

        self.expect(&TokenKind::Import)?;

        let names = if matches!(self.peek(), TokenKind::Star) {
            self.advance();
            vec![Alias {
                name: Ident::new(self.interner.intern("*"), self.current().span),
                asname: None,
                span: self.current().span,
            }]
        } else if matches!(self.peek(), TokenKind::LeftParen) {
            self.advance();
            let mut names = Vec::new();
            while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
                names.push(self.parse_alias()?);
                if !matches!(self.peek(), TokenKind::RightParen) {
                    self.expect(&TokenKind::Comma)?;
                }
            }
            self.expect(&TokenKind::RightParen)?;
            names
        } else {
            let mut names = Vec::new();
            loop {
                names.push(self.parse_alias()?);
                if !matches!(self.peek(), TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
            names
        };

        Ok(Stmt::new(
            StmtKind::ImportFrom {
                module,
                names,
                level,
            },
            self.span_from(start),
        ))
    }

    /// Parse a dotted name like `a.b.c` and return an Ident with the whole dotted string.
    /// Used for import statements where module paths can be dotted.
    /// 
    /// Also handles `py:` prefix for Python module imports:
    /// - `py:numpy` -> imports numpy from Python
    /// - `py:pandas.core` -> imports pandas.core from Python
    fn parse_dotted_name(&mut self) -> Result<Ident, ParseError> {
        let start = self.current().span;
        let first = self.parse_ident()?;
        
        let first_name = self.interner.resolve(first.name)
            .unwrap_or("<unknown>")
            .to_string();
        
        // Check for py: or python: prefix (e.g., py:numpy, python:pandas)
        let (prefix, mut name_parts) = if (first_name == "py" || first_name == "python") 
            && matches!(self.peek(), TokenKind::Colon) 
        {
            self.advance(); // consume ':'
            let module_name = self.parse_ident()?;
            let mod_name = self.interner.resolve(module_name.name)
                .unwrap_or("<unknown>")
                .to_string();
            (Some(first_name), vec![mod_name])
        } else {
            (None, vec![first_name])
        };
        
        // Continue parsing dotted parts (e.g., .submodule.subsubmodule)
        while matches!(self.peek(), TokenKind::Dot) {
            self.advance(); // consume '.'
            let part = self.parse_ident()?;
            name_parts.push(
                self.interner.resolve(part.name)
                    .unwrap_or("<unknown>")
                    .to_string()
            );
        }
        
        // Build the full name with prefix if present
        let full_name = if let Some(p) = prefix {
            format!("{}:{}", p, name_parts.join("."))
        } else {
            name_parts.join(".")
        };
        
        let sym = self.interner.intern(&full_name);
        
        Ok(Ident::new(sym, self.span_from(start)))
    }

    fn parse_alias(&mut self) -> Result<Alias, ParseError> {
        let start = self.current().span;
        // Use dotted name for imports - e.g., `import test_modules.mathlib`
        let name = self.parse_dotted_name()?;

        let asname = if matches!(self.peek(), TokenKind::As) {
            self.advance();
            Some(self.parse_ident()?)
        } else {
            None
        };

        Ok(Alias {
            name,
            asname,
            span: self.span_from(start),
        })
    }

    fn parse_global_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Global)?;

        let mut names = Vec::new();
        loop {
            names.push(self.parse_ident()?);
            if !matches!(self.peek(), TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        Ok(Stmt::new(
            StmtKind::Global { names },
            self.span_from(start),
        ))
    }

    fn parse_nonlocal_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Nonlocal)?;

        let mut names = Vec::new();
        loop {
            names.push(self.parse_ident()?);
            if !matches!(self.peek(), TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        Ok(Stmt::new(
            StmtKind::Nonlocal { names },
            self.span_from(start),
        ))
    }

    fn parse_assert_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Assert)?;

        let test = self.parse_expression()?;

        let msg = if matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        Ok(Stmt::new(
            StmtKind::Assert {
                test: Box::new(test),
                msg,
            },
            self.span_from(start),
        ))
    }

    fn parse_del_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Del)?;

        let mut targets = Vec::new();
        loop {
            targets.push(self.parse_target()?);
            if !matches!(self.peek(), TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        Ok(Stmt::new(
            StmtKind::Delete { targets },
            self.span_from(start),
        ))
    }

    fn parse_type_alias(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Type)?;

        let name = self.parse_ident()?;

        let type_params = if matches!(self.peek(), TokenKind::LeftBracket) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::Equal)?;
        let value = self.parse_type_expr()?;

        Ok(Stmt::new(
            StmtKind::TypeAlias {
                name,
                type_params,
                value: Box::new(value),
            },
            self.span_from(start),
        ))
    }

    /// Parse a tuple expression or single expression (for assignment targets).
    /// Handles `a, b, c` as a tuple without parentheses.
    fn parse_tuple_or_expression(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let first = self.parse_expression()?;
        
        // Check if this is a tuple (comma-separated values)
        if matches!(self.peek(), TokenKind::Comma) {
            let mut elts = vec![first];
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                // Allow trailing comma before = or :
                if matches!(self.peek(), TokenKind::Equal | TokenKind::Colon | TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof) {
                    break;
                }
                elts.push(self.parse_expression()?);
            }
            return Ok(Expr::new(
                ExprKind::Tuple {
                    elts,
                    ctx: ExprContext::Store, // For assignment targets
                },
                self.span_from(start),
            ));
        }
        
        Ok(first)
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        let expr = self.parse_tuple_or_expression()?;

        // Check for augmented assignment
        if self.peek().is_augmented_assign() {
            let op = self.peek().to_aug_op().unwrap();
            self.advance();
            let value = self.parse_expression()?;
            return Ok(Stmt::new(
                StmtKind::AugAssign {
                    target: Box::new(expr),
                    op,
                    value: Box::new(value),
                },
                self.span_from(start),
            ));
        }

        // Check for annotated assignment
        if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            let ownership = self.parse_ownership()?;
            let annotation = self.parse_type_expr()?;
            let value = if matches!(self.peek(), TokenKind::Equal) {
                self.advance();
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };
            return Ok(Stmt::new(
                StmtKind::AnnAssign {
                    target: Box::new(expr),
                    annotation: Box::new(annotation),
                    value,
                    simple: true,
                    ownership,
                },
                self.span_from(start),
            ));
        }

        // Check for regular assignment
        if matches!(self.peek(), TokenKind::Equal) {
            let mut targets = vec![expr];
            while matches!(self.peek(), TokenKind::Equal) {
                self.advance();
                let next = self.parse_tuple_or_expression()?;
                targets.push(next);
            }
            let value = targets.pop().unwrap();
            return Ok(Stmt::new(
                StmtKind::Assign {
                    targets,
                    value: Box::new(value),
                },
                self.span_from(start),
            ));
        }

        // Just an expression statement
        Ok(Stmt::new(
            StmtKind::Expr {
                value: Box::new(expr),
            },
            self.span_from(start),
        ))
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.skip_newlines();

        // Simple statement on same line
        if !matches!(self.peek(), TokenKind::Newline | TokenKind::Indent) {
            let stmt = self.parse_statement()?;
            return Ok(vec![stmt]);
        }

        self.skip_newlines();
        self.expect(&TokenKind::Indent)?;

        let mut stmts = Vec::new();
        while !matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }
            stmts.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(stmts)
    }

    // ========== Expression parsing ==========

    /// Parses an expression.
    pub fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_conditional()
    }

    fn parse_conditional(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let expr = self.parse_or()?;

        if matches!(self.peek(), TokenKind::If) {
            self.advance();
            let test = self.parse_or()?;
            self.expect(&TokenKind::Else)?;
            let orelse = self.parse_conditional()?;

            return Ok(Expr::new(
                ExprKind::IfExp {
                    test: Box::new(test),
                    body: Box::new(expr),
                    orelse: Box::new(orelse),
                },
                self.span_from(start),
            ));
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_and()?;

        while matches!(self.peek(), TokenKind::Or) {
            self.advance();
            let right = self.parse_and()?;
            expr = Expr::new(
                ExprKind::BoolOp {
                    op: BoolOp::Or,
                    values: vec![expr, right],
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_not()?;

        while matches!(self.peek(), TokenKind::And) {
            self.advance();
            let right = self.parse_not()?;
            expr = Expr::new(
                ExprKind::BoolOp {
                    op: BoolOp::And,
                    values: vec![expr, right],
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;

        if matches!(self.peek(), TokenKind::Not) {
            self.advance();
            let operand = self.parse_not()?;
            return Ok(Expr::new(
                ExprKind::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
                self.span_from(start),
            ));
        }

        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let left = self.parse_bitor()?;

        let mut ops = Vec::new();
        let mut comparators = Vec::new();

        loop {
            let op = match self.peek() {
                TokenKind::Less => CmpOp::Lt,
                TokenKind::Greater => CmpOp::Gt,
                TokenKind::LessEqual => CmpOp::LtE,
                TokenKind::GreaterEqual => CmpOp::GtE,
                TokenKind::EqualEqual => CmpOp::Eq,
                TokenKind::NotEqual => CmpOp::NotEq,
                TokenKind::In => CmpOp::In,
                TokenKind::Not if matches!(self.peek_ahead(1), Some(TokenKind::In)) => {
                    self.advance();
                    CmpOp::NotIn
                }
                TokenKind::Is => {
                    self.advance();
                    if matches!(self.peek(), TokenKind::Not) {
                        self.advance();
                        ops.push(CmpOp::IsNot);
                        comparators.push(self.parse_bitor()?);
                        continue;
                    } else {
                        ops.push(CmpOp::Is);
                        comparators.push(self.parse_bitor()?);
                        continue;
                    }
                }
                _ => break,
            };
            self.advance();
            ops.push(op);
            comparators.push(self.parse_bitor()?);
        }

        if ops.is_empty() {
            Ok(left)
        } else {
            Ok(Expr::new(
                ExprKind::Compare {
                    left: Box::new(left),
                    ops,
                    comparators,
                },
                self.span_from(start),
            ))
        }
    }

    fn parse_bitor(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_bitxor()?;

        while matches!(self.peek(), TokenKind::Pipe) {
            self.advance();
            let right = self.parse_bitxor()?;
            expr = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(expr),
                    op: BinOp::BitOr,
                    right: Box::new(right),
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_bitxor(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_bitand()?;

        while matches!(self.peek(), TokenKind::Caret) {
            self.advance();
            let right = self.parse_bitand()?;
            expr = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(expr),
                    op: BinOp::BitXor,
                    right: Box::new(right),
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_bitand(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_shift()?;

        while matches!(self.peek(), TokenKind::Ampersand) {
            self.advance();
            let right = self.parse_shift()?;
            expr = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(expr),
                    op: BinOp::BitAnd,
                    right: Box::new(right),
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_arith()?;

        loop {
            let op = match self.peek() {
                TokenKind::LeftShift => BinOp::LShift,
                TokenKind::RightShift => BinOp::RShift,
                _ => break,
            };
            self.advance();
            let right = self.parse_arith()?;
            expr = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_arith(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_term()?;

        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_term()?;
            expr = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_factor()?;

        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mult,
                TokenKind::At => BinOp::MatMult,
                TokenKind::Slash => BinOp::Div,
                TokenKind::DoubleSlash => BinOp::FloorDiv,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_factor()?;
            expr = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                self.span_from(start),
            );
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;

        let op = match self.peek() {
            TokenKind::Plus => Some(UnaryOp::UAdd),
            TokenKind::Minus => Some(UnaryOp::USub),
            TokenKind::Tilde => Some(UnaryOp::Invert),
            _ => None,
        };

        if let Some(op) = op {
            self.advance();
            let operand = self.parse_factor()?;
            return Ok(Expr::new(
                ExprKind::UnaryOp {
                    op,
                    operand: Box::new(operand),
                },
                self.span_from(start),
            ));
        }

        // Handle borrow expressions: &expr or &mut expr
        if matches!(self.peek(), TokenKind::Ampersand) {
            self.advance();
            // Check for &mut
            let ownership = if matches!(self.peek(), TokenKind::Mut) {
                self.advance();
                Ownership::Mut
            } else {
                Ownership::Borrow
            };
            let operand = self.parse_factor()?;
            return Ok(Expr::new(
                ExprKind::OwnershipExpr {
                    value: Box::new(operand),
                    ownership,
                },
                self.span_from(start),
            ));
        }

        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;

        // Handle await
        if matches!(self.peek(), TokenKind::Await) {
            self.advance();
            // Parse the awaited expression including calls, subscripts, attributes
            // e.g., `await get_value()` should parse the whole `get_value()` call
            let value = self.parse_unary_postfix()?;
            return Ok(Expr::new(
                ExprKind::Await {
                    value: Box::new(value),
                },
                self.span_from(start),
            ));
        }

        let base = self.parse_unary_postfix()?;

        if matches!(self.peek(), TokenKind::DoubleStar) {
            self.advance();
            let exp = self.parse_factor()?;
            return Ok(Expr::new(
                ExprKind::BinOp {
                    left: Box::new(base),
                    op: BinOp::Pow,
                    right: Box::new(exp),
                },
                self.span_from(start),
            ));
        }

        Ok(base)
    }

    fn parse_unary_postfix(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                TokenKind::LeftParen => {
                    self.advance();
                    let (args, keywords) = self.parse_call_args()?;
                    self.expect(&TokenKind::RightParen)?;
                    expr = Expr::new(
                        ExprKind::Call {
                            func: Box::new(expr),
                            args,
                            keywords,
                        },
                        self.span_from(start),
                    );
                }
                TokenKind::LeftBracket => {
                    self.advance();
                    let slice = self.parse_slice()?;
                    self.expect(&TokenKind::RightBracket)?;
                    expr = Expr::new(
                        ExprKind::Subscript {
                            value: Box::new(expr),
                            slice: Box::new(slice),
                            ctx: ExprContext::Load,
                        },
                        self.span_from(start),
                    );
                }
                TokenKind::Dot => {
                    self.advance();
                    let attr = self.parse_ident()?;
                    expr = Expr::new(
                        ExprKind::Attribute {
                            value: Box::new(expr),
                            attr,
                            ctx: ExprContext::Load,
                        },
                        self.span_from(start),
                    );
                }
                // Try operator: expr? - propagates errors like Rust's ?
                TokenKind::Question => {
                    self.advance();
                    expr = Expr::new(
                        ExprKind::Try {
                            value: Box::new(expr),
                        },
                        self.span_from(start),
                    );
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<(Vec<Expr>, Vec<Keyword>), ParseError> {
        let mut args = Vec::new();
        let mut keywords = Vec::new();
        let mut seen_keyword = false;

        while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
            let start = self.current().span;

            // Check for **kwargs
            if matches!(self.peek(), TokenKind::DoubleStar) {
                self.advance();
                let value = self.parse_expression()?;
                keywords.push(Keyword {
                    name: None,
                    value,
                    span: self.span_from(start),
                });
                seen_keyword = true;
            }
            // Check for *args
            else if matches!(self.peek(), TokenKind::Star) {
                self.advance();
                let value = self.parse_expression()?;
                args.push(Expr::new(
                    ExprKind::Starred {
                        value: Box::new(value),
                        ctx: ExprContext::Load,
                    },
                    self.span_from(start),
                ));
            }
            // Check for keyword argument
            else if matches!(self.peek(), TokenKind::Name(_)) {
                if let Some(TokenKind::Equal) = self.peek_ahead(1) {
                    let name = self.parse_ident()?;
                    self.advance(); // =
                    let value = self.parse_expression()?;
                    keywords.push(Keyword {
                        name: Some(name),
                        value,
                        span: self.span_from(start),
                    });
                    seen_keyword = true;
                } else if seen_keyword {
                    return Err(ParseError::Custom {
                        message: "positional argument follows keyword argument".to_string(),
                        span: self.current().span,
                    });
                } else {
                    args.push(self.parse_expression()?);
                }
            } else {
                if seen_keyword {
                    return Err(ParseError::Custom {
                        message: "positional argument follows keyword argument".to_string(),
                        span: self.current().span,
                    });
                }
                args.push(self.parse_expression()?);
            }

            if !matches!(self.peek(), TokenKind::RightParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }

        Ok((args, keywords))
    }

    fn parse_slice(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;

        // Check if this is a simple index or a slice
        let lower = if matches!(self.peek(), TokenKind::Colon) {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };

        if !matches!(self.peek(), TokenKind::Colon) {
            // Simple index
            return Ok(*lower.unwrap());
        }

        self.advance(); // :

        let upper = if matches!(self.peek(), TokenKind::Colon | TokenKind::RightBracket) {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };

        let step = if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            if matches!(self.peek(), TokenKind::RightBracket) {
                None
            } else {
                Some(Box::new(self.parse_expression()?))
            }
        } else {
            None
        };

        Ok(Expr::new(
            ExprKind::Slice { lower, upper, step },
            self.span_from(start),
        ))
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;

        match self.peek().clone() {
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::IntLit {
                        value: BigInt::from(n),
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Float(n) => {
                self.advance();
                Ok(Expr::new(ExprKind::FloatLit { value: n }, self.span_from(start)))
            }
            TokenKind::String(s) => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::StringLit {
                        value: s,
                        prefix: StringPrefix::None,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::FString(parts) => {
                self.advance();
                // Convert FString token parts to JoinedStr AST
                // Each part is (literal_part, expression_part)
                let mut values = Vec::new();
                
                for (literal, expr_str) in parts {
                    // Add literal part if non-empty
                    if !literal.is_empty() {
                        values.push(Expr::new(
                            ExprKind::StringLit {
                                value: literal,
                                prefix: StringPrefix::None,
                            },
                            self.span_from(start),
                        ));
                    }
                    
                    // Parse and add expression part if non-empty
                    if !expr_str.is_empty() {
                        // Parse the expression string into an AST
                        let expr_ast = self.parse_fstring_expr(&expr_str, start)?;
                        values.push(Expr::new(
                            ExprKind::FormattedValue {
                                value: Box::new(expr_ast),
                                conversion: None,
                                format_spec: None,
                            },
                            self.span_from(start),
                        ));
                    }
                }
                
                Ok(Expr::new(
                    ExprKind::JoinedStr { values },
                    self.span_from(start),
                ))
            }
            TokenKind::Bytes(b) => {
                self.advance();
                Ok(Expr::new(ExprKind::BytesLit { value: b }, self.span_from(start)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::new(ExprKind::BoolLit { value: true }, self.span_from(start)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::BoolLit { value: false },
                    self.span_from(start),
                ))
            }
            TokenKind::None => {
                self.advance();
                Ok(Expr::new(ExprKind::NoneLit, self.span_from(start)))
            }
            TokenKind::Ellipsis => {
                self.advance();
                Ok(Expr::new(ExprKind::Ellipsis, self.span_from(start)))
            }
            TokenKind::Name(sym) => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::Name {
                        id: Ident::new(sym, self.span_from(start)),
                        ctx: ExprContext::Load,
                    },
                    self.span_from(start),
                ))
            }
            // Allow 'type' keyword to be used as identifier (for type() builtin function)
            TokenKind::Type => {
                self.advance();
                // Create a Name expression with "type" as the identifier
                let type_sym = self.interner.intern("type");
                Ok(Expr::new(
                    ExprKind::Name {
                        id: Ident::new(type_sym, self.span_from(start)),
                        ctx: ExprContext::Load,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::LeftParen => {
                self.advance();
                
                // Empty tuple
                if matches!(self.peek(), TokenKind::RightParen) {
                    self.advance();
                    return Ok(Expr::new(
                        ExprKind::Tuple {
                            elts: Vec::new(),
                            ctx: ExprContext::Load,
                        },
                        self.span_from(start),
                    ));
                }

                // Generator expression or parenthesized expression
                let first = self.parse_expression()?;

                // Check for generator expression
                if matches!(self.peek(), TokenKind::For) {
                    let generators = self.parse_comprehension_clauses()?;
                    self.expect(&TokenKind::RightParen)?;
                    return Ok(Expr::new(
                        ExprKind::GeneratorExp {
                            elt: Box::new(first),
                            generators,
                        },
                        self.span_from(start),
                    ));
                }

                // Check for tuple
                if matches!(self.peek(), TokenKind::Comma) {
                    let mut elts = vec![first];
                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        if matches!(self.peek(), TokenKind::RightParen) {
                            break;
                        }
                        elts.push(self.parse_expression()?);
                    }
                    self.expect(&TokenKind::RightParen)?;
                    return Ok(Expr::new(
                        ExprKind::Tuple {
                            elts,
                            ctx: ExprContext::Load,
                        },
                        self.span_from(start),
                    ));
                }

                // Parenthesized expression
                self.expect(&TokenKind::RightParen)?;
                Ok(first)
            }
            TokenKind::LeftBracket => {
                self.advance();

                // Empty list
                if matches!(self.peek(), TokenKind::RightBracket) {
                    self.advance();
                    return Ok(Expr::new(
                        ExprKind::List {
                            elts: Vec::new(),
                            ctx: ExprContext::Load,
                        },
                        self.span_from(start),
                    ));
                }

                let first = self.parse_expression()?;

                // List comprehension
                if matches!(self.peek(), TokenKind::For) {
                    let generators = self.parse_comprehension_clauses()?;
                    self.expect(&TokenKind::RightBracket)?;
                    return Ok(Expr::new(
                        ExprKind::ListComp {
                            elt: Box::new(first),
                            generators,
                        },
                        self.span_from(start),
                    ));
                }

                // Regular list
                let mut elts = vec![first];
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::RightBracket) {
                        break;
                    }
                    elts.push(self.parse_expression()?);
                }
                self.expect(&TokenKind::RightBracket)?;
                Ok(Expr::new(
                    ExprKind::List {
                        elts,
                        ctx: ExprContext::Load,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::LeftBrace => {
                self.advance();

                // Empty dict
                if matches!(self.peek(), TokenKind::RightBrace) {
                    self.advance();
                    return Ok(Expr::new(
                        ExprKind::Dict {
                            keys: Vec::new(),
                            values: Vec::new(),
                        },
                        self.span_from(start),
                    ));
                }

                // Could be set or dict
                let first = self.parse_expression()?;

                // Dict
                if matches!(self.peek(), TokenKind::Colon) {
                    self.advance();
                    let first_value = self.parse_expression()?;

                    // Dict comprehension
                    if matches!(self.peek(), TokenKind::For) {
                        let generators = self.parse_comprehension_clauses()?;
                        self.expect(&TokenKind::RightBrace)?;
                        return Ok(Expr::new(
                            ExprKind::DictComp {
                                key: Box::new(first),
                                value: Box::new(first_value),
                                generators,
                            },
                            self.span_from(start),
                        ));
                    }

                    let mut keys = vec![Some(first)];
                    let mut values = vec![first_value];

                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        if matches!(self.peek(), TokenKind::RightBrace) {
                            break;
                        }
                        if matches!(self.peek(), TokenKind::DoubleStar) {
                            self.advance();
                            keys.push(None);
                            values.push(self.parse_expression()?);
                        } else {
                            keys.push(Some(self.parse_expression()?));
                            self.expect(&TokenKind::Colon)?;
                            values.push(self.parse_expression()?);
                        }
                    }
                    self.expect(&TokenKind::RightBrace)?;
                    return Ok(Expr::new(
                        ExprKind::Dict { keys, values },
                        self.span_from(start),
                    ));
                }

                // Set comprehension
                if matches!(self.peek(), TokenKind::For) {
                    let generators = self.parse_comprehension_clauses()?;
                    self.expect(&TokenKind::RightBrace)?;
                    return Ok(Expr::new(
                        ExprKind::SetComp {
                            elt: Box::new(first),
                            generators,
                        },
                        self.span_from(start),
                    ));
                }

                // Regular set
                let mut elts = vec![first];
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::RightBrace) {
                        break;
                    }
                    elts.push(self.parse_expression()?);
                }
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::new(ExprKind::Set { elts }, self.span_from(start)))
            }
            TokenKind::Lambda => {
                self.advance();
                let args = if matches!(self.peek(), TokenKind::Colon) {
                    Arguments::new()
                } else {
                    self.parse_lambda_parameters()?
                };
                self.expect(&TokenKind::Colon)?;
                let body = self.parse_expression()?;
                Ok(Expr::new(
                    ExprKind::Lambda {
                        args,
                        body: Box::new(body),
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Yield => {
                self.advance();
                if matches!(self.peek(), TokenKind::From) {
                    self.advance();
                    let value = self.parse_expression()?;
                    Ok(Expr::new(
                        ExprKind::YieldFrom {
                            value: Box::new(value),
                        },
                        self.span_from(start),
                    ))
                } else if self.peek().can_start_expr() {
                    let value = self.parse_expression()?;
                    Ok(Expr::new(
                        ExprKind::Yield {
                            value: Some(Box::new(value)),
                        },
                        self.span_from(start),
                    ))
                } else {
                    Ok(Expr::new(
                        ExprKind::Yield { value: None },
                        self.span_from(start),
                    ))
                }
            }
            TokenKind::Star => {
                self.advance();
                let value = self.parse_expression()?;
                Ok(Expr::new(
                    ExprKind::Starred {
                        value: Box::new(value),
                        ctx: ExprContext::Load,
                    },
                    self.span_from(start),
                ))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "expression".to_string(),
                found: format!("{}", self.peek()),
                span: self.current().span,
            }),
        }
    }

    fn parse_comprehension_clauses(&mut self) -> Result<Vec<Comprehension>, ParseError> {
        let mut generators = Vec::new();

        while matches!(self.peek(), TokenKind::For | TokenKind::Async) {
            let start = self.current().span;
            let is_async = if matches!(self.peek(), TokenKind::Async) {
                self.advance();
                true
            } else {
                false
            };

            self.expect(&TokenKind::For)?;
            let target = self.parse_target()?;
            self.expect(&TokenKind::In)?;
            let iter = self.parse_or()?;

            let mut ifs = Vec::new();
            while matches!(self.peek(), TokenKind::If) {
                self.advance();
                ifs.push(self.parse_or()?);
            }

            generators.push(Comprehension {
                target: Box::new(target),
                iter: Box::new(iter),
                ifs,
                is_async,
                span: self.span_from(start),
            });
        }

        Ok(generators)
    }

    fn parse_target(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut expr = self.parse_primary()?;

        // Handle subscripts and attributes
        loop {
            match self.peek() {
                TokenKind::LeftBracket => {
                    self.advance();
                    let slice = self.parse_slice()?;
                    self.expect(&TokenKind::RightBracket)?;
                    expr = Expr::new(
                        ExprKind::Subscript {
                            value: Box::new(expr),
                            slice: Box::new(slice),
                            ctx: ExprContext::Store,
                        },
                        self.span_from(start),
                    );
                }
                TokenKind::Dot => {
                    self.advance();
                    let attr = self.parse_ident()?;
                    expr = Expr::new(
                        ExprKind::Attribute {
                            value: Box::new(expr),
                            attr,
                            ctx: ExprContext::Store,
                        },
                        self.span_from(start),
                    );
                }
                TokenKind::Comma => {
                    let mut elts = vec![expr];
                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        if !self.peek().can_start_expr() {
                            break;
                        }
                        elts.push(self.parse_primary()?);
                    }
                    return Ok(Expr::new(
                        ExprKind::Tuple {
                            elts,
                            ctx: ExprContext::Store,
                        },
                        self.span_from(start),
                    ));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_ident(&mut self) -> Result<Ident, ParseError> {
        match self.peek() {
            TokenKind::Name(sym) => {
                let sym = *sym;
                let span = self.current().span;
                self.advance();
                Ok(Ident::new(sym, span))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: format!("{}", self.peek()),
                span: self.current().span,
            }),
        }
    }

    /// Parse an expression from an f-string placeholder.
    /// This creates a sub-parser for the expression text.
    fn parse_fstring_expr(&mut self, expr_str: &str, span: Span) -> Result<Expr, ParseError> {
        // For simple identifiers, just return a Name expression directly
        let trimmed = expr_str.trim();
        if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') && !trimmed.is_empty() {
            let sym = self.interner.intern(trimmed);
            return Ok(Expr::new(
                ExprKind::Name {
                    id: Ident::new(sym, span),
                    ctx: ExprContext::Load,
                },
                span,
            ));
        }
        
        // For more complex expressions, create a sub-parser to properly parse them
        use crate::lexer::Lexer;
        use roast_common::{FileId, SourceFile};
        
        // Create a temporary source file for the sub-parser
        let file_id = FileId::new(9999); // Temporary ID for f-string expressions
        let temp_source = SourceFile::new(
            file_id,
            "<fstring>".to_string(),
            trimmed.to_string(),
        );
        
        // Create a new lexer for the expression string
        let mut lexer = Lexer::new(&temp_source, self.interner);
        let tokens: Vec<_> = lexer.collect();
        
        // Create a sub-parser for these tokens
        let mut sub_parser = Parser {
            tokens: &tokens,
            source: &temp_source,
            interner: self.interner,
            diagnostics: self.diagnostics,
            pos: 0,
        };
        
        // Parse as an expression
        sub_parser.parse_expression()
    }

    // ========== Type expression parsing ==========

    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        self.parse_type_union()
    }

    fn parse_type_union(&mut self) -> Result<TypeExpr, ParseError> {
        let start = self.current().span;
        let mut types = vec![self.parse_type_primary()?];

        while matches!(self.peek(), TokenKind::Pipe) {
            self.advance();
            types.push(self.parse_type_primary()?);
        }

        if types.len() == 1 {
            Ok(types.pop().unwrap())
        } else {
            Ok(TypeExpr::new(
                TypeExprKind::Union { types },
                self.span_from(start),
            ))
        }
    }

    fn parse_type_primary(&mut self) -> Result<TypeExpr, ParseError> {
        let start = self.current().span;

        match self.peek().clone() {
            TokenKind::None => {
                // Handle None as a type expression (keyword token)
                self.advance();
                Ok(TypeExpr::new(TypeExprKind::None, self.span_from(start)))
            }
            TokenKind::Name(sym) => {
                self.advance();
                let ident = Ident::new(sym, self.span_from(start));
                
                // Check for special type names
                let name_str = self.interner.resolve(sym);
                
                let mut ty = match name_str {
                    Some("None") => TypeExpr::new(TypeExprKind::None, self.span_from(start)),
                    Some("Any") => TypeExpr::new(TypeExprKind::Any, self.span_from(start)),
                    Some("Self") => TypeExpr::new(TypeExprKind::SelfType, self.span_from(start)),
                    _ => TypeExpr::new(
                        TypeExprKind::Name { name: ident },
                        self.span_from(start),
                    ),
                };

                // Handle generic subscript
                if matches!(self.peek(), TokenKind::LeftBracket) {
                    self.advance();
                    
                    let mut args = Vec::new();
                    while !matches!(self.peek(), TokenKind::RightBracket | TokenKind::Eof) {
                        args.push(self.parse_type_expr()?);
                        if !matches!(self.peek(), TokenKind::RightBracket) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RightBracket)?;

                    let slice = if args.len() == 1 {
                        args.pop().unwrap()
                    } else {
                        TypeExpr::new(
                            TypeExprKind::Tuple { elts: args },
                            self.span_from(start),
                        )
                    };

                    ty = TypeExpr::new(
                        TypeExprKind::Subscript {
                            value: Box::new(ty),
                            slice: Box::new(slice),
                        },
                        self.span_from(start),
                    );
                }

                // Handle optional shorthand ?
                if matches!(self.peek(), TokenKind::Question) {
                    self.advance();
                    ty = TypeExpr::new(
                        TypeExprKind::Optional {
                            inner: Box::new(ty),
                        },
                        self.span_from(start),
                    );
                }

                Ok(ty)
            }
            TokenKind::LeftParen => {
                self.advance();
                let mut elts = Vec::new();
                while !matches!(self.peek(), TokenKind::RightParen | TokenKind::Eof) {
                    elts.push(self.parse_type_expr()?);
                    if !matches!(self.peek(), TokenKind::RightParen) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightParen)?;

                if elts.len() == 1 {
                    Ok(elts.pop().unwrap())
                } else {
                    Ok(TypeExpr::new(
                        TypeExprKind::Tuple { elts },
                        self.span_from(start),
                    ))
                }
            }
            TokenKind::LeftBracket => {
                // Callable params
                self.advance();
                let mut params = Vec::new();
                while !matches!(self.peek(), TokenKind::RightBracket | TokenKind::Eof) {
                    params.push(self.parse_type_expr()?);
                    if !matches!(self.peek(), TokenKind::RightBracket) {
                        self.expect(&TokenKind::Comma)?;
                    }
                }
                self.expect(&TokenKind::RightBracket)?;
                Ok(TypeExpr::new(
                    TypeExprKind::Tuple { elts: params },
                    self.span_from(start),
                ))
            }
            TokenKind::String(s) => {
                self.advance();
                Ok(TypeExpr::new(
                    TypeExprKind::StringAnnotation { value: s },
                    self.span_from(start),
                ))
            }
            TokenKind::Ellipsis => {
                self.advance();
                // Ellipsis in Callable[..., ReturnType]
                Ok(TypeExpr::new(TypeExprKind::Any, self.span_from(start)))
            }
            TokenKind::Star => {
                self.advance();
                let inner = self.parse_type_primary()?;
                Ok(TypeExpr::new(
                    TypeExprKind::Unpack {
                        inner: Box::new(inner),
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Ampersand => {
                self.advance();
                let mutable = if matches!(self.peek(), TokenKind::Mut) {
                    self.advance();
                    true
                } else {
                    false
                };
                let inner = self.parse_type_primary()?;
                Ok(TypeExpr::new(
                    TypeExprKind::Ref {
                        inner: Box::new(inner),
                        mutable,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Own => {
                // owned T or own T
                self.advance();
                let inner = self.parse_type_primary()?;
                Ok(TypeExpr::new(
                    TypeExprKind::Owned {
                        inner: Box::new(inner),
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Borrow => {
                // borrow T
                self.advance();
                let mutable = if matches!(self.peek(), TokenKind::Mut) {
                    self.advance();
                    true
                } else {
                    false
                };
                let inner = self.parse_type_primary()?;
                Ok(TypeExpr::new(
                    TypeExprKind::Borrowed {
                        inner: Box::new(inner),
                        mutable,
                    },
                    self.span_from(start),
                ))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "type expression".to_string(),
                found: format!("{}", self.peek()),
                span: self.current().span,
            }),
        }
    }
}

