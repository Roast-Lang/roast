//! LLVM IR builder utilities.
//!
//! Helper types and functions for building LLVM IR.

/// LLVM type representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LlvmType {
    Void,
    I1,
    I8,
    I16,
    I32,
    I64,
    I128,
    Float,
    Double,
    Ptr(Box<LlvmType>),
    Array(Box<LlvmType>, usize),
    Struct(Vec<LlvmType>),
    Function(Box<LlvmType>, Vec<LlvmType>),
}

impl std::fmt::Display for LlvmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlvmType::Void => write!(f, "void"),
            LlvmType::I1 => write!(f, "i1"),
            LlvmType::I8 => write!(f, "i8"),
            LlvmType::I16 => write!(f, "i16"),
            LlvmType::I32 => write!(f, "i32"),
            LlvmType::I64 => write!(f, "i64"),
            LlvmType::I128 => write!(f, "i128"),
            LlvmType::Float => write!(f, "float"),
            LlvmType::Double => write!(f, "double"),
            LlvmType::Ptr(inner) => write!(f, "{}*", inner),
            LlvmType::Array(elem, len) => write!(f, "[{} x {}]", len, elem),
            LlvmType::Struct(fields) => {
                write!(f, "{{ ")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", field)?;
                }
                write!(f, " }}")
            }
            LlvmType::Function(ret, params) => {
                write!(f, "{} (", ret)?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p)?;
                }
                write!(f, ")")
            }
        }
    }
}

/// LLVM IR builder for constructing IR incrementally.
pub struct LlvmIrBuilder {
    output: String,
    indent: usize,
}

impl LlvmIrBuilder {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    pub fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
        self.output.push_str(s);
        self.output.push('\n');
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
    }

    pub fn finish(self) -> String {
        self.output
    }
}

impl Default for LlvmIrBuilder {
    fn default() -> Self {
        Self::new()
    }
}
