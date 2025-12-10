//! LLVM code generation - comprehensive implementation.
//!
//! This module provides the full LLVM code generator that handles:
//! - All primitive types (int, float, bool, str)
//! - Collections (list, dict, set, tuple)
//! - Classes and objects
//! - Closures and lambdas
//! - Iterators and generators
//! - Exception handling
//! - Async/await

pub use crate::{LlvmCodeGen, LlvmError, LlvmResult};

use std::collections::HashMap;
use roast_mir::*;
use roast_typer::Type;
use crate::runtime::{RUNTIME_DECLARATIONS, TypeTag};

/// Extended function generator with full language support.
pub struct FullFunctionGen<'a> {
    /// Module-level codegen
    pub module: &'a mut ModuleCodeGen,
    /// Current function's MIR
    pub body: &'a MirBody,
    /// SSA value counter
    pub next_ssa: usize,
    /// Local variable to LLVM value mapping
    pub locals: HashMap<u32, LlvmValue>,
    /// Generated IR buffer
    pub ir: String,
    /// Exception handler stack
    pub exception_handlers: Vec<ExceptionHandler>,
    /// Loop context for break/continue
    pub loop_stack: Vec<LoopContext>,
}

/// Module-level code generator.
pub struct ModuleCodeGen {
    /// String constants pool
    pub strings: HashMap<String, usize>,
    /// Next string ID
    pub next_string_id: usize,
    /// Function declarations
    pub function_decls: Vec<String>,
    /// Class metadata
    pub classes: HashMap<String, ClassInfo>,
    /// Global variables
    pub globals: HashMap<String, GlobalInfo>,
}

/// LLVM value reference.
#[derive(Clone, Debug)]
pub struct LlvmValue {
    pub name: String,
    pub ty: LlvmType,
    pub is_ptr: bool,
}

/// LLVM type representation.
#[derive(Clone, Debug, PartialEq)]
pub enum LlvmType {
    Void,
    I1,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Ptr,
    Array(Box<LlvmType>, usize),
    Struct(Vec<LlvmType>),
    Function(Box<LlvmType>, Vec<LlvmType>),
}

impl LlvmType {
    pub fn to_ir_string(&self) -> String {
        match self {
            LlvmType::Void => "void".to_string(),
            LlvmType::I1 => "i1".to_string(),
            LlvmType::I8 => "i8".to_string(),
            LlvmType::I16 => "i16".to_string(),
            LlvmType::I32 => "i32".to_string(),
            LlvmType::I64 => "i64".to_string(),
            LlvmType::F32 => "float".to_string(),
            LlvmType::F64 => "double".to_string(),
            LlvmType::Ptr => "i8*".to_string(),
            LlvmType::Array(elem, size) => format!("[{} x {}]", size, elem.to_ir_string()),
            LlvmType::Struct(fields) => {
                let fields_str: Vec<_> = fields.iter().map(|f| f.to_ir_string()).collect();
                format!("{{ {} }}", fields_str.join(", "))
            }
            LlvmType::Function(ret, params) => {
                let params_str: Vec<_> = params.iter().map(|p| p.to_ir_string()).collect();
                format!("{} ({})*", ret.to_ir_string(), params_str.join(", "))
            }
        }
    }

    /// Convert Roast type to LLVM type.
    pub fn from_roast(ty: &Type) -> Self {
        match ty {
            Type::Bool => LlvmType::I1,
            Type::Int | Type::Int64 => LlvmType::I64,
            Type::Int8 => LlvmType::I8,
            Type::Int16 => LlvmType::I16,
            Type::Int32 => LlvmType::I32,
            Type::Float | Type::Float64 => LlvmType::F64,
            Type::Float32 => LlvmType::F32,
            // NoneType as I64 (0 = None sentinel) since void can't be stored
            Type::NoneType => LlvmType::I64,
            Type::Str => LlvmType::Ptr,
            Type::List(_) => LlvmType::Ptr,
            Type::Dict(_, _) => LlvmType::Ptr,
            Type::Set(_) => LlvmType::Ptr,
            Type::Tuple(_) => LlvmType::Ptr,
            Type::Class(_) => LlvmType::Ptr,
            Type::Optional(_) => LlvmType::I64,
            Type::Callable { .. } => LlvmType::Ptr,
            _ => LlvmType::I64,
        }
    }
}

/// Exception handler info.
#[derive(Clone, Debug)]
pub struct ExceptionHandler {
    pub catch_block: u32,
    pub finally_block: Option<u32>,
    pub exception_types: Vec<String>,
}

/// Loop context for break/continue.
#[derive(Clone, Debug)]
pub struct LoopContext {
    pub continue_block: u32,
    pub break_block: u32,
}

/// Class information.
#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub fields: Vec<(String, LlvmType)>,
    pub methods: Vec<String>,
    pub vtable_name: String,
}

/// Global variable info.
#[derive(Clone, Debug)]
pub struct GlobalInfo {
    pub name: String,
    pub ty: LlvmType,
    pub initializer: Option<String>,
}

impl ModuleCodeGen {
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
            next_string_id: 0,
            function_decls: Vec::new(),
            classes: HashMap::new(),
            globals: HashMap::new(),
        }
    }

    /// Add a string constant and return its global name.
    pub fn add_string(&mut self, s: &str) -> String {
        if let Some(&id) = self.strings.get(s) {
            format!("@.str.{}", id)
        } else {
            let id = self.next_string_id;
            self.next_string_id += 1;
            self.strings.insert(s.to_string(), id);
            format!("@.str.{}", id)
        }
    }

    /// Generate string constant definitions.
    pub fn emit_strings(&self) -> String {
        let mut ir = String::new();
        for (s, id) in &self.strings {
            let escaped = escape_string(s);
            ir.push_str(&format!(
                "@.str.{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
                id,
                s.len() + 1,
                escaped
            ));
        }
        ir
    }
}

/// Escape string for LLVM IR.
pub fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\5C"),
            '\n' => result.push_str("\\0A"),
            '\r' => result.push_str("\\0D"),
            '\t' => result.push_str("\\09"),
            '"' => result.push_str("\\22"),
            c if c.is_ascii() && !c.is_control() => result.push(c),
            c => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("\\{:02X}", b));
                }
            }
        }
    }
    result
}
