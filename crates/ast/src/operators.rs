//! Operator types and utilities.

use crate::expr::{BinOp, BoolOp, CmpOp, UnaryOp};

/// Augmented assignment operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AugOp {
    Add,
    Sub,
    Mult,
    Div,
    FloorDiv,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    MatMult,
}

impl AugOp {
    /// Converts to the corresponding binary operator.
    pub fn to_binop(self) -> BinOp {
        match self {
            AugOp::Add => BinOp::Add,
            AugOp::Sub => BinOp::Sub,
            AugOp::Mult => BinOp::Mult,
            AugOp::Div => BinOp::Div,
            AugOp::FloorDiv => BinOp::FloorDiv,
            AugOp::Mod => BinOp::Mod,
            AugOp::Pow => BinOp::Pow,
            AugOp::LShift => BinOp::LShift,
            AugOp::RShift => BinOp::RShift,
            AugOp::BitOr => BinOp::BitOr,
            AugOp::BitXor => BinOp::BitXor,
            AugOp::BitAnd => BinOp::BitAnd,
            AugOp::MatMult => BinOp::MatMult,
        }
    }

    /// Returns the symbol for this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            AugOp::Add => "+=",
            AugOp::Sub => "-=",
            AugOp::Mult => "*=",
            AugOp::Div => "/=",
            AugOp::FloorDiv => "//=",
            AugOp::Mod => "%=",
            AugOp::Pow => "**=",
            AugOp::LShift => "<<=",
            AugOp::RShift => ">>=",
            AugOp::BitOr => "|=",
            AugOp::BitXor => "^=",
            AugOp::BitAnd => "&=",
            AugOp::MatMult => "@=",
        }
    }
}

impl BinOp {
    /// Returns the symbol for this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mult => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::LShift => "<<",
            BinOp::RShift => ">>",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::BitAnd => "&",
            BinOp::MatMult => "@",
        }
    }

    /// Returns the precedence of this operator (higher = binds tighter).
    pub fn precedence(self) -> u8 {
        match self {
            BinOp::Pow => 14,
            BinOp::Mult | BinOp::Div | BinOp::FloorDiv | BinOp::Mod | BinOp::MatMult => 12,
            BinOp::Add | BinOp::Sub => 11,
            BinOp::LShift | BinOp::RShift => 10,
            BinOp::BitAnd => 9,
            BinOp::BitXor => 8,
            BinOp::BitOr => 7,
        }
    }

    /// Returns true if this operator is right-associative.
    pub fn is_right_assoc(self) -> bool {
        matches!(self, BinOp::Pow)
    }
}

impl UnaryOp {
    /// Returns the symbol for this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            UnaryOp::Invert => "~",
            UnaryOp::Not => "not",
            UnaryOp::UAdd => "+",
            UnaryOp::USub => "-",
        }
    }
}

impl BoolOp {
    /// Returns the symbol for this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            BoolOp::And => "and",
            BoolOp::Or => "or",
        }
    }
}

impl CmpOp {
    /// Returns the symbol for this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        }
    }
}

