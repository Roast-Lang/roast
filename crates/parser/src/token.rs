//! Token definitions for the Roast lexer.

use roast_common::{Span, Symbol};
use std::fmt;

/// A token produced by the lexer.
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn is(&self, kind: TokenKind) -> bool {
        std::mem::discriminant(&self.kind) == std::mem::discriminant(&kind)
    }

    pub fn is_keyword(&self, kw: &str) -> bool {
        matches!(&self.kind, TokenKind::Name(sym) if sym.as_raw() > 0)
    }
}

/// Token kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    // Literals
    /// Integer literal
    Int(i128),
    /// Float literal
    Float(f64),
    /// String literal
    String(String),
    /// Bytes literal
    Bytes(Vec<u8>),
    /// F-string (parsed format string with literal and expression parts)
    /// Each tuple is (literal_part, expression_string)
    FString(Vec<(String, String)>),
    /// F-string start (legacy - kept for compatibility)
    FStringStart,
    /// F-string middle (literal part)
    FStringMiddle(String),
    /// F-string end
    FStringEnd,

    // Identifiers and keywords
    /// Identifier or keyword
    Name(Symbol),

    // Keywords (for faster matching)
    False,
    None,
    True,
    And,
    As,
    Assert,
    Async,
    Await,
    Break,
    Class,
    Continue,
    Def,
    Del,
    Elif,
    Else,
    Except,
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    Try,
    While,
    With,
    Yield,
    Match,
    Case,
    Type,

    // Roast-specific keywords
    Mut,
    Imm,
    Borrow,
    Move,
    Own,
    Ref,

    // Operators
    /// +
    Plus,
    /// -
    Minus,
    /// *
    Star,
    /// **
    DoubleStar,
    /// /
    Slash,
    /// //
    DoubleSlash,
    /// %
    Percent,
    /// @
    At,
    /// <<
    LeftShift,
    /// >>
    RightShift,
    /// &
    Ampersand,
    /// |
    Pipe,
    /// ^
    Caret,
    /// ~
    Tilde,
    /// :=
    ColonEqual,
    /// <
    Less,
    /// >
    Greater,
    /// <=
    LessEqual,
    /// >=
    GreaterEqual,
    /// ==
    EqualEqual,
    /// !=
    NotEqual,

    // Delimiters
    /// (
    LeftParen,
    /// )
    RightParen,
    /// [
    LeftBracket,
    /// ]
    RightBracket,
    /// {
    LeftBrace,
    /// }
    RightBrace,
    /// ,
    Comma,
    /// :
    Colon,
    /// ;
    Semi,
    /// .
    Dot,
    /// ...
    Ellipsis,
    /// =
    Equal,
    /// ->
    Arrow,
    /// ?
    Question,

    // Augmented assignment
    /// +=
    PlusEqual,
    /// -=
    MinusEqual,
    /// *=
    StarEqual,
    /// /=
    SlashEqual,
    /// //=
    DoubleSlashEqual,
    /// %=
    PercentEqual,
    /// **=
    DoubleStarEqual,
    /// @=
    AtEqual,
    /// &=
    AmpersandEqual,
    /// |=
    PipeEqual,
    /// ^=
    CaretEqual,
    /// <<=
    LeftShiftEqual,
    /// >>=
    RightShiftEqual,

    // Indentation
    /// Indentation increase
    Indent,
    /// Indentation decrease
    Dedent,
    /// Newline
    Newline,

    // Special
    /// End of file
    Eof,
    /// Error token
    Error(String),
}

impl TokenKind {
    /// Returns true if this is a comparison operator.
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            TokenKind::Less
                | TokenKind::Greater
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::EqualEqual
                | TokenKind::NotEqual
                | TokenKind::In
                | TokenKind::Is
                | TokenKind::Not
        )
    }

    /// Returns true if this is an augmented assignment operator.
    pub fn is_augmented_assign(&self) -> bool {
        matches!(
            self,
            TokenKind::PlusEqual
                | TokenKind::MinusEqual
                | TokenKind::StarEqual
                | TokenKind::SlashEqual
                | TokenKind::DoubleSlashEqual
                | TokenKind::PercentEqual
                | TokenKind::DoubleStarEqual
                | TokenKind::AtEqual
                | TokenKind::AmpersandEqual
                | TokenKind::PipeEqual
                | TokenKind::CaretEqual
                | TokenKind::LeftShiftEqual
                | TokenKind::RightShiftEqual
        )
    }

    /// Returns the corresponding augmented assignment operator.
    pub fn to_aug_op(&self) -> Option<roast_ast::AugOp> {
        Some(match self {
            TokenKind::PlusEqual => roast_ast::AugOp::Add,
            TokenKind::MinusEqual => roast_ast::AugOp::Sub,
            TokenKind::StarEqual => roast_ast::AugOp::Mult,
            TokenKind::SlashEqual => roast_ast::AugOp::Div,
            TokenKind::DoubleSlashEqual => roast_ast::AugOp::FloorDiv,
            TokenKind::PercentEqual => roast_ast::AugOp::Mod,
            TokenKind::DoubleStarEqual => roast_ast::AugOp::Pow,
            TokenKind::AtEqual => roast_ast::AugOp::MatMult,
            TokenKind::AmpersandEqual => roast_ast::AugOp::BitAnd,
            TokenKind::PipeEqual => roast_ast::AugOp::BitOr,
            TokenKind::CaretEqual => roast_ast::AugOp::BitXor,
            TokenKind::LeftShiftEqual => roast_ast::AugOp::LShift,
            TokenKind::RightShiftEqual => roast_ast::AugOp::RShift,
            _ => return None,
        })
    }

    /// Returns true if this token can start an expression.
    pub fn can_start_expr(&self) -> bool {
        matches!(
            self,
            TokenKind::Name(_)
                | TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::Bytes(_)
                | TokenKind::FString(_)
                | TokenKind::FStringStart
                | TokenKind::True
                | TokenKind::False
                | TokenKind::None
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftBrace
                | TokenKind::Lambda
                | TokenKind::Not
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Tilde
                | TokenKind::Star
                | TokenKind::DoubleStar
                | TokenKind::Await
                | TokenKind::Ellipsis
                | TokenKind::Type
        )
    }

    /// Returns true if this token can start a statement.
    pub fn can_start_stmt(&self) -> bool {
        self.can_start_expr()
            || matches!(
                self,
                TokenKind::Def
                    | TokenKind::Class
                    | TokenKind::If
                    | TokenKind::For
                    | TokenKind::While
                    | TokenKind::Try
                    | TokenKind::With
                    | TokenKind::Match
                    | TokenKind::Return
                    | TokenKind::Raise
                    | TokenKind::Break
                    | TokenKind::Continue
                    | TokenKind::Pass
                    | TokenKind::Import
                    | TokenKind::From
                    | TokenKind::Global
                    | TokenKind::Nonlocal
                    | TokenKind::Assert
                    | TokenKind::Del
                    | TokenKind::Async
                    | TokenKind::At
                    | TokenKind::Type
            )
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Int(n) => write!(f, "{}", n),
            TokenKind::Float(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::Bytes(b) => write!(f, "b\"{}\"", String::from_utf8_lossy(b)),
            TokenKind::Name(sym) => write!(f, "<name:{}>", sym.as_raw()),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::DoubleStar => write!(f, "**"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::DoubleSlash => write!(f, "//"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::At => write!(f, "@"),
            TokenKind::LeftShift => write!(f, "<<"),
            TokenKind::RightShift => write!(f, ">>"),
            TokenKind::Ampersand => write!(f, "&"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::Less => write!(f, "<"),
            TokenKind::Greater => write!(f, ">"),
            TokenKind::LessEqual => write!(f, "<="),
            TokenKind::GreaterEqual => write!(f, ">="),
            TokenKind::EqualEqual => write!(f, "=="),
            TokenKind::NotEqual => write!(f, "!="),
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::LeftBracket => write!(f, "["),
            TokenKind::RightBracket => write!(f, "]"),
            TokenKind::LeftBrace => write!(f, "{{"),
            TokenKind::RightBrace => write!(f, "}}"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::ColonEqual => write!(f, ":="),
            TokenKind::Semi => write!(f, ";"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Ellipsis => write!(f, "..."),
            TokenKind::Equal => write!(f, "="),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Indent => write!(f, "INDENT"),
            TokenKind::Dedent => write!(f, "DEDENT"),
            TokenKind::Newline => write!(f, "NEWLINE"),
            TokenKind::Eof => write!(f, "EOF"),
            TokenKind::Error(e) => write!(f, "ERROR({})", e),
            // Keywords
            TokenKind::False => write!(f, "False"),
            TokenKind::None => write!(f, "None"),
            TokenKind::True => write!(f, "True"),
            TokenKind::And => write!(f, "and"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Assert => write!(f, "assert"),
            TokenKind::Async => write!(f, "async"),
            TokenKind::Await => write!(f, "await"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Class => write!(f, "class"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Def => write!(f, "def"),
            TokenKind::Del => write!(f, "del"),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Except => write!(f, "except"),
            TokenKind::Finally => write!(f, "finally"),
            TokenKind::For => write!(f, "for"),
            TokenKind::From => write!(f, "from"),
            TokenKind::Global => write!(f, "global"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Is => write!(f, "is"),
            TokenKind::Lambda => write!(f, "lambda"),
            TokenKind::Nonlocal => write!(f, "nonlocal"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Pass => write!(f, "pass"),
            TokenKind::Raise => write!(f, "raise"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::Try => write!(f, "try"),
            TokenKind::While => write!(f, "while"),
            TokenKind::With => write!(f, "with"),
            TokenKind::Yield => write!(f, "yield"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::Case => write!(f, "case"),
            TokenKind::Type => write!(f, "type"),
            TokenKind::Mut => write!(f, "mut"),
            TokenKind::Imm => write!(f, "imm"),
            TokenKind::Borrow => write!(f, "borrow"),
            TokenKind::Move => write!(f, "move"),
            TokenKind::Own => write!(f, "own"),
            TokenKind::Ref => write!(f, "ref"),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Maps a keyword string to its token kind.
pub fn keyword_to_token(s: &str) -> Option<TokenKind> {
    Some(match s {
        "False" => TokenKind::False,
        "None" => TokenKind::None,
        "True" => TokenKind::True,
        "and" => TokenKind::And,
        "as" => TokenKind::As,
        "assert" => TokenKind::Assert,
        "async" => TokenKind::Async,
        "await" => TokenKind::Await,
        "break" => TokenKind::Break,
        "class" => TokenKind::Class,
        "continue" => TokenKind::Continue,
        "def" => TokenKind::Def,
        "del" => TokenKind::Del,
        "elif" => TokenKind::Elif,
        "else" => TokenKind::Else,
        "except" => TokenKind::Except,
        "finally" => TokenKind::Finally,
        "for" => TokenKind::For,
        "from" => TokenKind::From,
        "global" => TokenKind::Global,
        "if" => TokenKind::If,
        "import" => TokenKind::Import,
        "in" => TokenKind::In,
        "is" => TokenKind::Is,
        "lambda" => TokenKind::Lambda,
        "nonlocal" => TokenKind::Nonlocal,
        "not" => TokenKind::Not,
        "or" => TokenKind::Or,
        "pass" => TokenKind::Pass,
        "raise" => TokenKind::Raise,
        "return" => TokenKind::Return,
        "try" => TokenKind::Try,
        "while" => TokenKind::While,
        "with" => TokenKind::With,
        "yield" => TokenKind::Yield,
        "match" => TokenKind::Match,
        "case" => TokenKind::Case,
        "type" => TokenKind::Type,
        // Roast keywords
        "mut" => TokenKind::Mut,
        "imm" => TokenKind::Imm,
        "borrow" => TokenKind::Borrow,
        "move" => TokenKind::Move,
        "own" => TokenKind::Own,
        "owned" => TokenKind::Own,  // Alias for 'own'
        "ref" => TokenKind::Ref,
        _ => return None,
    })
}
