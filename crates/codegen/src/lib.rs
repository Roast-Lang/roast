//! Code generation for Roast.
//!
//! This crate provides code generation backends for the Roast compiler:
//! - Bytecode: A portable bytecode format for the Roast VM
//! - Native: Native code generation via Cranelift
//!
//! The bytecode backend is always available and serves as the default.
//! Enable the `cranelift` feature for native code generation.

pub mod bytecode;
pub mod emit;
pub mod target;
pub mod native;
pub mod linker;

#[cfg(feature = "cranelift")]
pub mod cranelift;

pub use bytecode::{Bytecode, BytecodeBuilder, Constant, Instruction, OpCode, Operand};
pub use emit::CodeEmitter;
pub use target::{OptLevel, Target, TargetConfig};
pub use native::{NativeConfig, NativeCode, NativeCodeGen};
pub use linker::{Linker, LinkerConfig};

#[cfg(feature = "cranelift")]
pub use cranelift::CraneliftBackend;

use roast_mir::MirBody;
use thiserror::Error;

/// Code generation errors.
#[derive(Error, Debug)]
pub enum CodegenError {
    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Invalid MIR: {0}")]
    InvalidMir(String),

    #[error("Target error: {0}")]
    Target(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for code generation.
pub type CodegenResult<T> = Result<T, CodegenError>;

/// Trait for code generation backends.
pub trait CodeGenerator {
    /// The output type produced by this generator.
    type Output;

    /// Generates code for a MIR body.
    fn generate(&mut self, body: &MirBody) -> CodegenResult<Self::Output>;

    /// Finalizes code generation and returns the result.
    fn finalize(self) -> CodegenResult<Self::Output>;
}

