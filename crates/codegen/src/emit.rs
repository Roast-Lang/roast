//! Code emitter interface.

use crate::bytecode::{Bytecode, BytecodeBuilder};
use crate::target::TargetConfig;
use crate::{CodeGenerator, CodegenError, CodegenResult};
use roast_mir::MirBody;
use std::io::Write;

/// A code emitter that produces bytecode.
pub struct CodeEmitter {
    config: TargetConfig,
    modules: Vec<CompiledModule>,
}

/// A compiled module.
#[derive(Clone, Debug)]
pub struct CompiledModule {
    pub name: String,
    pub bytecode: Vec<Bytecode>,
}

impl CodeEmitter {
    /// Creates a new code emitter.
    pub fn new(config: TargetConfig) -> Self {
        Self {
            config,
            modules: Vec::new(),
        }
    }

    /// Creates a bytecode-only emitter.
    pub fn bytecode() -> Self {
        Self::new(TargetConfig::bytecode())
    }

    /// Compiles a MIR body to bytecode.
    pub fn compile(&mut self, body: &MirBody) -> CodegenResult<Bytecode> {
        let name = format!("func_{}", body.name.as_raw());
        let mut builder = BytecodeBuilder::new(name);
        Ok(builder.compile(body))
    }

    /// Compiles multiple MIR bodies into a module.
    pub fn compile_module(&mut self, name: &str, bodies: &[MirBody]) -> CodegenResult<CompiledModule> {
        let mut bytecodes = Vec::new();
        for body in bodies {
            bytecodes.push(self.compile(body)?);
        }
        Ok(CompiledModule {
            name: name.to_string(),
            bytecode: bytecodes,
        })
    }

    /// Adds a compiled module.
    pub fn add_module(&mut self, module: CompiledModule) {
        self.modules.push(module);
    }

    /// Returns all compiled modules.
    pub fn modules(&self) -> &[CompiledModule] {
        &self.modules
    }

    /// Writes bytecode to a file.
    pub fn write_bytecode<W: Write>(&self, module: &CompiledModule, writer: &mut W) -> std::io::Result<()> {
        // Write magic bytes
        writer.write_all(b"ROAST")?;

        // Write version
        writer.write_all(&[0, 1, 0])?;  // 0.1.0

        // Write module name length and name
        let name_bytes = module.name.as_bytes();
        writer.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
        writer.write_all(name_bytes)?;

        // Write number of functions
        writer.write_all(&(module.bytecode.len() as u32).to_le_bytes())?;

        // Write each function's bytecode
        for bc in &module.bytecode {
            self.write_function(bc, writer)?;
        }

        Ok(())
    }

    fn write_function<W: Write>(&self, bc: &Bytecode, writer: &mut W) -> std::io::Result<()> {
        // Write function name
        let name_bytes = bc.name.as_bytes();
        writer.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
        writer.write_all(name_bytes)?;

        // Write metadata
        writer.write_all(&bc.num_locals.to_le_bytes())?;
        writer.write_all(&[bc.num_params])?;

        // Write constants count and constants
        writer.write_all(&(bc.constants.len() as u16).to_le_bytes())?;
        for constant in &bc.constants {
            self.write_constant(constant, writer)?;
        }

        // Write names count and names
        writer.write_all(&(bc.names.len() as u16).to_le_bytes())?;
        for name in &bc.names {
            let bytes = name.as_bytes();
            writer.write_all(&(bytes.len() as u16).to_le_bytes())?;
            writer.write_all(bytes)?;
        }

        // Write instruction count and instructions
        writer.write_all(&(bc.instructions.len() as u32).to_le_bytes())?;
        for instr in &bc.instructions {
            writer.write_all(&[instr.opcode as u8])?;
            match &instr.operand {
                crate::bytecode::Operand::None => {}
                crate::bytecode::Operand::U8(v) => writer.write_all(&[*v])?,
                crate::bytecode::Operand::U16(v) => writer.write_all(&v.to_le_bytes())?,
                crate::bytecode::Operand::I16(v) => writer.write_all(&v.to_le_bytes())?,
                crate::bytecode::Operand::I64(v) => writer.write_all(&v.to_le_bytes())?,
                crate::bytecode::Operand::F64(v) => writer.write_all(&v.to_le_bytes())?,
                crate::bytecode::Operand::Const(v) => writer.write_all(&(*v as u16).to_le_bytes())?,
                crate::bytecode::Operand::MakeFunc(code_idx, name_idx, arity) => {
                    writer.write_all(&code_idx.to_le_bytes())?;
                    writer.write_all(&name_idx.to_le_bytes())?;
                    writer.write_all(&[*arity])?;
                }
            }
        }

        Ok(())
    }

    fn write_constant<W: Write>(&self, constant: &crate::bytecode::Constant, writer: &mut W) -> std::io::Result<()> {
        match constant {
            crate::bytecode::Constant::None => {
                writer.write_all(&[0x00])?;
            }
            crate::bytecode::Constant::Bool(v) => {
                writer.write_all(&[0x01, if *v { 1 } else { 0 }])?;
            }
            crate::bytecode::Constant::Int(v) => {
                writer.write_all(&[0x02])?;
                writer.write_all(&v.to_le_bytes())?;
            }
            crate::bytecode::Constant::BigInt(s) => {
                writer.write_all(&[0x03])?;
                let bytes = s.as_bytes();
                writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
                writer.write_all(bytes)?;
            }
            crate::bytecode::Constant::Float(v) => {
                writer.write_all(&[0x04])?;
                writer.write_all(&v.to_le_bytes())?;
            }
            crate::bytecode::Constant::Str(s) => {
                writer.write_all(&[0x05])?;
                let bytes = s.as_bytes();
                writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
                writer.write_all(bytes)?;
            }
            crate::bytecode::Constant::Bytes(b) => {
                writer.write_all(&[0x06])?;
                writer.write_all(&(b.len() as u32).to_le_bytes())?;
                writer.write_all(b)?;
            }
            crate::bytecode::Constant::Tuple(elements) => {
                writer.write_all(&[0x07])?;
                writer.write_all(&(elements.len() as u16).to_le_bytes())?;
                for elem in elements {
                    self.write_constant(elem, writer)?;
                }
            }
            crate::bytecode::Constant::Code(bc) => {
                writer.write_all(&[0x08])?;
                self.write_function(bc, writer)?;
            }
        }
        Ok(())
    }
}

impl CodeGenerator for CodeEmitter {
    type Output = Vec<CompiledModule>;

    fn generate(&mut self, body: &MirBody) -> CodegenResult<Self::Output> {
        let bytecode = self.compile(body)?;
        let module = CompiledModule {
            name: body.name.as_raw().to_string(),
            bytecode: vec![bytecode],
        };
        self.add_module(module);
        Ok(self.modules.clone())
    }

    fn finalize(self) -> CodegenResult<Self::Output> {
        Ok(self.modules)
    }
}
