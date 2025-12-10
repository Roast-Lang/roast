//! Baseline JIT compiler.
//!
//! The baseline compiler performs quick template-based compilation from bytecode
//! to native code. It prioritizes compilation speed over code quality.
//!
//! Key characteristics:
//! - No optimization passes
//! - Direct bytecode-to-native translation
//! - Minimal register allocation (mostly stack-based)
//! - Quick compile times (< 1ms for most functions)

use crate::{JitError, JitResult};
use roast_codegen::{Bytecode, Instruction, OpCode, Operand};
use rustc_hash::FxHashMap;

/// Configuration for baseline compilation.
#[derive(Clone, Debug)]
pub struct BaselineConfig {
    /// Enable OSR entry points.
    pub enable_osr: bool,
    /// Enable deopt points.
    pub enable_deopt: bool,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self {
            enable_osr: true,
            enable_deopt: true,
        }
    }
}

/// Call site info for patching.
#[derive(Clone, Debug)]
pub struct BaselineCallSite {
    /// Native code offset.
    pub offset: usize,
    /// Target function ID (if known).
    pub target: Option<u32>,
}

/// Baseline-compiled function.
#[derive(Clone)]
pub struct BaselineCode {
    /// Function ID.
    pub func_id: u32,
    /// Native code bytes.
    pub code: Vec<u8>,
    /// Entry point offset.
    pub entry_offset: usize,
    /// Size of the stack frame.
    pub frame_size: usize,
    /// Call sites for inline caching.
    pub call_sites: Vec<BaselineCallSite>,
}

/// The baseline compiler.
pub struct BaselineCompiler {
    config: BaselineConfig,
    /// Current function ID.
    func_id: u32,
    /// Generated code buffer.
    code: Vec<u8>,
    /// Current bytecode offset.
    bc_offset: usize,
    /// Bytecode to native offset mapping.
    bc_to_native: FxHashMap<usize, usize>,
    /// Labels for forward jumps.
    forward_labels: FxHashMap<usize, Vec<usize>>,
    /// Frame size.
    frame_size: usize,
    /// Number of locals.
    num_locals: usize,
    /// Call sites.
    call_sites: Vec<BaselineCallSite>,
}

impl BaselineCompiler {
    pub fn new(config: BaselineConfig) -> Self {
        Self {
            config,
            func_id: 0,
            code: Vec::with_capacity(4096),
            bc_offset: 0,
            bc_to_native: FxHashMap::default(),
            forward_labels: FxHashMap::default(),
            frame_size: 0,
            num_locals: 0,
            call_sites: Vec::new(),
        }
    }
    
    /// Compiles bytecode to baseline native code.
    pub fn compile(&mut self, func_id: u32, bytecode: &Bytecode) -> JitResult<BaselineCode> {
        self.reset();
        self.func_id = func_id;
        self.num_locals = bytecode.num_locals as usize;
        
        // Calculate frame size: locals + spill slots + alignment
        self.frame_size = (self.num_locals + 8) * 8; // 8 bytes per slot
        self.frame_size = (self.frame_size + 15) & !15; // 16-byte align
        
        // Emit prologue
        self.emit_prologue();
        
        // Compile each instruction
        for (offset, instr) in bytecode.instructions.iter().enumerate() {
            self.bc_offset = offset;
            self.bc_to_native.insert(offset, self.code.len());
            
            // Patch any forward jumps to this location
            self.patch_forward_jumps(offset);
            
            self.compile_instruction(instr)?;
        }
        
        // Emit epilogue
        self.emit_epilogue();
        
        Ok(BaselineCode {
            func_id,
            code: std::mem::take(&mut self.code),
            entry_offset: 0,
            frame_size: self.frame_size,
            call_sites: std::mem::take(&mut self.call_sites),
        })
    }
    
    fn reset(&mut self) {
        self.code.clear();
        self.bc_offset = 0;
        self.bc_to_native.clear();
        self.forward_labels.clear();
        self.frame_size = 0;
        self.num_locals = 0;
        self.call_sites.clear();
    }
    
    /// Emits function prologue.
    fn emit_prologue(&mut self) {
        // x86-64 prologue:
        // push rbp
        // mov rbp, rsp
        // sub rsp, frame_size
        
        self.emit_byte(0x55); // push rbp
        self.emit_bytes(&[0x48, 0x89, 0xe5]); // mov rbp, rsp
        
        // sub rsp, frame_size
        if self.frame_size <= 127 {
            self.emit_bytes(&[0x48, 0x83, 0xec, self.frame_size as u8]);
        } else {
            self.emit_bytes(&[0x48, 0x81, 0xec]);
            self.emit_u32(self.frame_size as u32);
        }
    }
    
    /// Emits function epilogue.
    fn emit_epilogue(&mut self) {
        // x86-64 epilogue:
        // mov rsp, rbp
        // pop rbp
        // ret
        
        self.emit_bytes(&[0x48, 0x89, 0xec]); // mov rsp, rbp
        self.emit_byte(0x5d); // pop rbp
        self.emit_byte(0xc3); // ret
    }
    
    /// Compiles a single bytecode instruction.
    fn compile_instruction(&mut self, instr: &Instruction) -> JitResult<()> {
        match instr.opcode {
            OpCode::LoadConst => {
                self.compile_load_const(&instr.operand)?;
            }
            OpCode::LoadLocal => {
                self.compile_load_local(&instr.operand)?;
            }
            OpCode::StoreLocal => {
                self.compile_store_local(&instr.operand)?;
            }
            OpCode::LoadGlobal => {
                self.compile_load_global(&instr.operand)?;
            }
            OpCode::StoreGlobal => {
                self.compile_store_global(&instr.operand)?;
            }
            OpCode::Add => {
                self.compile_binary_op(BinaryOp::Add)?;
            }
            OpCode::Sub => {
                self.compile_binary_op(BinaryOp::Sub)?;
            }
            OpCode::Mul => {
                self.compile_binary_op(BinaryOp::Mul)?;
            }
            OpCode::Div => {
                self.compile_binary_op(BinaryOp::Div)?;
            }
            OpCode::Mod => {
                self.compile_binary_op(BinaryOp::Mod)?;
            }
            OpCode::Neg => {
                self.compile_unary_op(UnaryOp::Neg)?;
            }
            OpCode::Not => {
                self.compile_unary_op(UnaryOp::Not)?;
            }
            OpCode::Eq => {
                self.compile_compare(CompareOp::Eq)?;
            }
            OpCode::Ne => {
                self.compile_compare(CompareOp::Ne)?;
            }
            OpCode::Lt => {
                self.compile_compare(CompareOp::Lt)?;
            }
            OpCode::Le => {
                self.compile_compare(CompareOp::Le)?;
            }
            OpCode::Gt => {
                self.compile_compare(CompareOp::Gt)?;
            }
            OpCode::Ge => {
                self.compile_compare(CompareOp::Ge)?;
            }
            OpCode::Jump => {
                self.compile_jump(&instr.operand)?;
            }
            OpCode::JumpIfTrue => {
                self.compile_conditional_jump(&instr.operand, true)?;
            }
            OpCode::JumpIfFalse => {
                self.compile_conditional_jump(&instr.operand, false)?;
            }
            OpCode::Call => {
                self.compile_call(&instr.operand)?;
            }
            OpCode::Return => {
                self.compile_return()?;
            }
            OpCode::Pop => {
                self.compile_pop()?;
            }
            OpCode::Dup => {
                self.compile_dup()?;
            }
            OpCode::Nop => {
                // No-op
            }
            _ => {
                // For unsupported opcodes, emit a call to interpreter fallback
                self.compile_interpreter_fallback(instr)?;
            }
        }
        
        Ok(())
    }
    
    // ========================================================================
    // Instruction Compilation
    // ========================================================================
    
    fn compile_load_const(&mut self, operand: &Operand) -> JitResult<()> {
        match operand {
            Operand::Const(idx) => {
                // Load from constant pool via runtime call
                self.emit_mov_rdi_imm64(*idx as i64);
                self.emit_call_runtime(RuntimeFn::LoadConst);
                // push rax (result)
                self.emit_byte(0x50);
            }
            Operand::I64(v) => {
                // Immediate integer - load directly
                self.emit_push_imm64(*v);
            }
            Operand::U16(idx) => {
                self.emit_mov_rdi_imm64(*idx as i64);
                self.emit_call_runtime(RuntimeFn::LoadConst);
                self.emit_byte(0x50);
            }
            _ => return Err(JitError::InvalidBytecode("Invalid LoadConst operand".into())),
        }
        
        Ok(())
    }
    
    fn compile_load_local(&mut self, operand: &Operand) -> JitResult<()> {
        let local_idx = self.get_local_index(operand)?;
        let offset = self.local_offset(local_idx);
        
        // mov rax, [rbp - offset]
        self.emit_load_from_stack(offset);
        // push rax
        self.emit_byte(0x50);
        
        Ok(())
    }
    
    fn compile_store_local(&mut self, operand: &Operand) -> JitResult<()> {
        let local_idx = self.get_local_index(operand)?;
        let offset = self.local_offset(local_idx);
        
        // pop rax
        self.emit_byte(0x58);
        // mov [rbp - offset], rax
        self.emit_store_to_stack(offset);
        
        Ok(())
    }
    
    fn compile_load_global(&mut self, operand: &Operand) -> JitResult<()> {
        let name_idx = self.get_index(operand)?;
        
        // mov rdi, name_idx
        self.emit_mov_rdi_imm64(name_idx as i64);
        // call rt_load_global
        self.emit_call_runtime(RuntimeFn::LoadGlobal);
        // push rax
        self.emit_byte(0x50);
        
        Ok(())
    }
    
    fn compile_store_global(&mut self, operand: &Operand) -> JitResult<()> {
        let name_idx = self.get_index(operand)?;
        
        // pop rsi (value)
        self.emit_byte(0x5e);
        // mov rdi, name_idx
        self.emit_mov_rdi_imm64(name_idx as i64);
        // call rt_store_global
        self.emit_call_runtime(RuntimeFn::StoreGlobal);
        
        Ok(())
    }
    
    fn compile_binary_op(&mut self, op: BinaryOp) -> JitResult<()> {
        // pop rbx (right)
        self.emit_byte(0x5b);
        // pop rax (left)
        self.emit_byte(0x58);
        
        match op {
            BinaryOp::Add => {
                // add rax, rbx
                self.emit_bytes(&[0x48, 0x01, 0xd8]);
            }
            BinaryOp::Sub => {
                // sub rax, rbx
                self.emit_bytes(&[0x48, 0x29, 0xd8]);
            }
            BinaryOp::Mul => {
                // imul rax, rbx
                self.emit_bytes(&[0x48, 0x0f, 0xaf, 0xc3]);
            }
            BinaryOp::Div => {
                // cqo (sign extend rax into rdx:rax)
                self.emit_bytes(&[0x48, 0x99]);
                // idiv rbx
                self.emit_bytes(&[0x48, 0xf7, 0xfb]);
            }
            BinaryOp::Mod => {
                // cqo
                self.emit_bytes(&[0x48, 0x99]);
                // idiv rbx
                self.emit_bytes(&[0x48, 0xf7, 0xfb]);
                // mov rax, rdx (remainder)
                self.emit_bytes(&[0x48, 0x89, 0xd0]);
            }
        }
        
        // push rax
        self.emit_byte(0x50);
        
        Ok(())
    }
    
    fn compile_unary_op(&mut self, op: UnaryOp) -> JitResult<()> {
        // pop rax
        self.emit_byte(0x58);
        
        match op {
            UnaryOp::Neg => {
                // neg rax
                self.emit_bytes(&[0x48, 0xf7, 0xd8]);
            }
            UnaryOp::Not => {
                // xor rax, 1
                self.emit_bytes(&[0x48, 0x83, 0xf0, 0x01]);
            }
        }
        
        // push rax
        self.emit_byte(0x50);
        
        Ok(())
    }
    
    fn compile_compare(&mut self, op: CompareOp) -> JitResult<()> {
        // pop rbx (right)
        self.emit_byte(0x5b);
        // pop rax (left)
        self.emit_byte(0x58);
        
        // cmp rax, rbx
        self.emit_bytes(&[0x48, 0x39, 0xd8]);
        
        // setXX al
        let setcc = match op {
            CompareOp::Eq => 0x94, // sete
            CompareOp::Ne => 0x95, // setne
            CompareOp::Lt => 0x9c, // setl
            CompareOp::Le => 0x9e, // setle
            CompareOp::Gt => 0x9f, // setg
            CompareOp::Ge => 0x9d, // setge
        };
        self.emit_bytes(&[0x0f, setcc, 0xc0]);
        
        // movzx rax, al
        self.emit_bytes(&[0x48, 0x0f, 0xb6, 0xc0]);
        
        // push rax
        self.emit_byte(0x50);
        
        Ok(())
    }
    
    fn compile_jump(&mut self, operand: &Operand) -> JitResult<()> {
        let target = self.get_jump_target(operand)?;
        
        if let Some(&native_offset) = self.bc_to_native.get(&target) {
            // Backward jump - we know the target
            let rel = native_offset as i32 - (self.code.len() as i32 + 5);
            self.emit_byte(0xe9); // jmp rel32
            self.emit_i32(rel);
        } else {
            // Forward jump - need to patch later
            self.emit_byte(0xe9);
            let patch_offset = self.code.len();
            self.emit_i32(0); // placeholder
            self.forward_labels.entry(target).or_default().push(patch_offset);
        }
        
        Ok(())
    }
    
    fn compile_conditional_jump(&mut self, operand: &Operand, jump_if_true: bool) -> JitResult<()> {
        let target = self.get_jump_target(operand)?;
        
        // pop rax (condition)
        self.emit_byte(0x58);
        // test rax, rax
        self.emit_bytes(&[0x48, 0x85, 0xc0]);
        
        let jcc = if jump_if_true { 0x85 } else { 0x84 }; // jne / je
        
        if let Some(&native_offset) = self.bc_to_native.get(&target) {
            let rel = native_offset as i32 - (self.code.len() as i32 + 6);
            self.emit_bytes(&[0x0f, jcc]);
            self.emit_i32(rel);
        } else {
            self.emit_bytes(&[0x0f, jcc]);
            let patch_offset = self.code.len();
            self.emit_i32(0);
            self.forward_labels.entry(target).or_default().push(patch_offset);
        }
        
        Ok(())
    }
    
    fn compile_call(&mut self, operand: &Operand) -> JitResult<()> {
        let argc = self.get_arg_count(operand)?;
        
        // Record call site for inline caching
        let call_site = BaselineCallSite {
            offset: self.code.len(),
            target: None,
        };
        self.call_sites.push(call_site);
        
        // mov rdi, argc
        self.emit_mov_rdi_imm64(argc as i64);
        // call rt_call_function
        self.emit_call_runtime(RuntimeFn::CallFunction);
        // push rax (return value)
        self.emit_byte(0x50);
        
        Ok(())
    }
    
    fn compile_return(&mut self) -> JitResult<()> {
        // pop rax (return value)
        self.emit_byte(0x58);
        // epilogue and ret
        self.emit_epilogue();
        
        Ok(())
    }
    
    fn compile_pop(&mut self) -> JitResult<()> {
        // add rsp, 8
        self.emit_bytes(&[0x48, 0x83, 0xc4, 0x08]);
        Ok(())
    }
    
    fn compile_dup(&mut self) -> JitResult<()> {
        // push [rsp]
        self.emit_bytes(&[0xff, 0x34, 0x24]);
        Ok(())
    }
    
    fn compile_interpreter_fallback(&mut self, _instr: &Instruction) -> JitResult<()> {
        // For complex instructions, call back into interpreter
        // mov rdi, bc_offset
        self.emit_mov_rdi_imm64(self.bc_offset as i64);
        // call rt_interpret_one
        self.emit_call_runtime(RuntimeFn::InterpretOne);
        
        Ok(())
    }
    
    // ========================================================================
    // Code Emission Helpers
    // ========================================================================
    
    fn emit_byte(&mut self, b: u8) {
        self.code.push(b);
    }
    
    fn emit_bytes(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }
    
    fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    
    fn emit_i32(&mut self, v: i32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    
    fn emit_u64(&mut self, v: u64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    
    fn emit_push_imm64(&mut self, v: i64) {
        if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
            // push imm32 (sign-extended)
            self.emit_byte(0x68);
            self.emit_i32(v as i32);
        } else {
            // mov rax, imm64; push rax
            self.emit_bytes(&[0x48, 0xb8]);
            self.emit_u64(v as u64);
            self.emit_byte(0x50);
        }
    }
    
    fn emit_mov_rdi_imm64(&mut self, v: i64) {
        // mov rdi, imm64
        self.emit_bytes(&[0x48, 0xbf]);
        self.emit_u64(v as u64);
    }
    
    fn emit_load_from_stack(&mut self, offset: i32) {
        // mov rax, [rbp - offset]
        if offset >= -128 && offset <= 127 {
            self.emit_bytes(&[0x48, 0x8b, 0x45]);
            self.emit_byte((-offset) as u8);
        } else {
            self.emit_bytes(&[0x48, 0x8b, 0x85]);
            self.emit_i32(-offset);
        }
    }
    
    fn emit_store_to_stack(&mut self, offset: i32) {
        // mov [rbp - offset], rax
        if offset >= -128 && offset <= 127 {
            self.emit_bytes(&[0x48, 0x89, 0x45]);
            self.emit_byte((-offset) as u8);
        } else {
            self.emit_bytes(&[0x48, 0x89, 0x85]);
            self.emit_i32(-offset);
        }
    }
    
    fn emit_call_runtime(&mut self, _func: RuntimeFn) {
        // In a real implementation, this would:
        // 1. Look up the runtime function address
        // 2. Emit an indirect call or direct call with relocation
        
        // For now, emit a placeholder: call [rip+0]
        self.emit_bytes(&[0xff, 0x15, 0x00, 0x00, 0x00, 0x00]);
    }
    
    fn patch_forward_jumps(&mut self, bc_offset: usize) {
        if let Some(patches) = self.forward_labels.remove(&bc_offset) {
            let native_offset = self.code.len();
            for patch_offset in patches {
                let rel = native_offset as i32 - patch_offset as i32 - 4;
                let bytes = rel.to_le_bytes();
                self.code[patch_offset] = bytes[0];
                self.code[patch_offset + 1] = bytes[1];
                self.code[patch_offset + 2] = bytes[2];
                self.code[patch_offset + 3] = bytes[3];
            }
        }
    }
    
    fn local_offset(&self, idx: usize) -> i32 {
        ((idx + 1) * 8) as i32
    }
    
    fn get_local_index(&self, operand: &Operand) -> JitResult<usize> {
        match operand {
            Operand::U8(idx) => Ok(*idx as usize),
            Operand::U16(idx) => Ok(*idx as usize),
            Operand::I64(idx) => Ok(*idx as usize),
            _ => Err(JitError::InvalidBytecode("Missing local index".into())),
        }
    }
    
    fn get_index(&self, operand: &Operand) -> JitResult<usize> {
        match operand {
            Operand::U8(idx) => Ok(*idx as usize),
            Operand::U16(idx) => Ok(*idx as usize),
            Operand::Const(idx) => Ok(*idx),
            Operand::I64(idx) => Ok(*idx as usize),
            _ => Err(JitError::InvalidBytecode("Missing index".into())),
        }
    }
    
    fn get_jump_target(&self, operand: &Operand) -> JitResult<usize> {
        match operand {
            Operand::I16(offset) => {
                // Relative offset from current position
                let target = (self.bc_offset as isize + *offset as isize) as usize;
                Ok(target)
            }
            Operand::U16(idx) => Ok(*idx as usize),
            Operand::I64(idx) => Ok(*idx as usize),
            _ => Err(JitError::InvalidBytecode("Missing jump target".into())),
        }
    }
    
    fn get_arg_count(&self, operand: &Operand) -> JitResult<usize> {
        match operand {
            Operand::U8(n) => Ok(*n as usize),
            Operand::U16(n) => Ok(*n as usize),
            _ => Ok(0),
        }
    }
}

/// Binary operations.
#[derive(Clone, Copy, Debug)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Unary operations.
#[derive(Clone, Copy, Debug)]
enum UnaryOp {
    Neg,
    Not,
}

/// Comparison operations.
#[derive(Clone, Copy, Debug)]
enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Runtime functions that can be called from JIT code.
#[derive(Clone, Copy, Debug)]
enum RuntimeFn {
    LoadConst,
    LoadGlobal,
    StoreGlobal,
    CallFunction,
    InterpretOne,
}

impl Default for BaselineCompiler {
    fn default() -> Self {
        Self::new(BaselineConfig::default())
    }
}
