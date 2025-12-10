//! Bytecode representation for the Roast VM.
//!
//! The bytecode is a stack-based intermediate representation that can be
//! interpreted or JIT-compiled.

use roast_common::{Interner, Symbol};
use roast_mir::{MirBody, MirBinOp, MirConstant, MirOperand, MirRvalue, MirStmtKind, MirTerminator, MirUnaryOp};
use rustc_hash::FxHashMap;
use std::fmt;

/// Operation codes for the bytecode VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    // Stack operations
    Nop = 0x00,
    Pop = 0x01,
    Dup = 0x02,
    Swap = 0x03,
    Rot3 = 0x04,

    // Constants
    LoadConst = 0x10,
    LoadNone = 0x11,
    LoadTrue = 0x12,
    LoadFalse = 0x13,
    LoadInt = 0x14,     // followed by i64
    LoadFloat = 0x15,   // followed by f64

    // Local variables
    LoadLocal = 0x20,   // followed by u16 slot
    StoreLocal = 0x21,
    LoadFast = 0x22,    // 1-byte slot optimization
    StoreFast = 0x23,

    // Global/name operations
    LoadGlobal = 0x30,  // followed by u16 name index
    StoreGlobal = 0x31,
    LoadName = 0x32,
    StoreName = 0x33,
    LoadAttr = 0x34,
    StoreAttr = 0x35,
    DeleteAttr = 0x36,

    // Subscript operations
    LoadSubscr = 0x40,
    StoreSubscr = 0x41,
    DeleteSubscr = 0x42,

    // Arithmetic operations
    Add = 0x50,
    Sub = 0x51,
    Mul = 0x52,
    Div = 0x53,
    FloorDiv = 0x54,
    Mod = 0x55,
    Pow = 0x56,
    Neg = 0x57,

    // Bitwise operations
    BitAnd = 0x60,
    BitOr = 0x61,
    BitXor = 0x62,
    BitNot = 0x63,
    Shl = 0x64,
    Shr = 0x65,

    // Comparison operations
    Eq = 0x70,
    Ne = 0x71,
    Lt = 0x72,
    Le = 0x73,
    Gt = 0x74,
    Ge = 0x75,
    Is = 0x76,
    IsNot = 0x77,
    In = 0x78,
    NotIn = 0x79,

    // Boolean operations
    Not = 0x80,
    And = 0x81,
    Or = 0x82,

    // Control flow
    Jump = 0x90,           // followed by i16 offset
    JumpIfTrue = 0x91,
    JumpIfFalse = 0x92,
    JumpIfTrueOrPop = 0x93,
    JumpIfFalseOrPop = 0x94,

    // Function operations
    Call = 0xA0,           // followed by u8 arg count
    CallKw = 0xA1,         // followed by u8 arg count, u8 kw count
    Return = 0xA2,
    Yield = 0xA3,
    YieldFrom = 0xA4,
    MakeFunction = 0xA9,   // followed by u16 const index (code), u16 name index, u8 arity

    // Async operations
    Await = 0xA5,          // Await an awaitable object (coroutine, future, etc.)
    GetAwaitable = 0xA6,   // Convert to awaitable (calls __await__)
    SetupAsyncWith = 0xA7, // Setup async with statement
    EndAsyncFor = 0xA8,    // End async for loop

    // Object creation
    BuildList = 0xB0,      // followed by u16 count
    BuildTuple = 0xB1,
    BuildDict = 0xB2,
    BuildSet = 0xB3,
    BuildString = 0xB4,
    BuildSlice = 0xB5,     // followed by u8 (2 or 3 for slice args)
    ListAppend = 0xB6,     // Append TOS to list at stack[depth], for list comprehensions

    // Unpacking
    UnpackSequence = 0xC0, // followed by u8 count
    UnpackEx = 0xC1,       // extended unpacking

    // Iteration
    GetIter = 0xD0,
    ForIter = 0xD1,        // followed by i16 jump offset

    // Exception handling
    SetupTry = 0xE0,       // followed by i16 handler offset
    PopExcept = 0xE1,
    Raise = 0xE2,
    Reraise = 0xE3,

    // Import
    Import = 0xF0,
    ImportFrom = 0xF1,
    ImportStar = 0xF2,

    // Reference operations (for ownership)
    Ref = 0xF8,            // Create shared reference
    RefMut = 0xF9,         // Create mutable reference
    Deref = 0xFA,          // Dereference
    Move = 0xFB,           // Move value
    Copy = 0xFC,           // Copy value
    Drop = 0xFD,           // Drop value

    // Pattern matching (using 0xC2-0xC9 range)
    MatchSequence = 0xC2, // Match against sequence pattern
    MatchMapping = 0xC3,  // Match against mapping pattern
    MatchClass = 0xC4,    // Match against class pattern
    MatchAs = 0xC5,       // Bind to name
    MatchOr = 0xC6,       // Or pattern
    MatchStar = 0xC7,     // Star pattern for sequences
    MatchGuard = 0xC8,    // Evaluate guard condition

    // Try operator (? operator for Result/Option propagation)
    TryUnwrap = 0xC9,     // followed by i16 error handler offset - unwrap or jump to handler

    // Special
    Halt = 0xFF,
}

impl OpCode {
    /// Returns the size of the instruction (including opcode byte).
    pub fn instruction_size(&self) -> usize {
        match self {
            OpCode::Nop | OpCode::Pop | OpCode::Dup | OpCode::Swap | OpCode::Rot3 |
            OpCode::LoadNone | OpCode::LoadTrue | OpCode::LoadFalse |
            OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::FloorDiv |
            OpCode::Mod | OpCode::Pow | OpCode::Neg |
            OpCode::BitAnd | OpCode::BitOr | OpCode::BitXor | OpCode::BitNot |
            OpCode::Shl | OpCode::Shr |
            OpCode::Eq | OpCode::Ne | OpCode::Lt | OpCode::Le | OpCode::Gt | OpCode::Ge |
            OpCode::Is | OpCode::IsNot | OpCode::In | OpCode::NotIn |
            OpCode::Not | OpCode::And | OpCode::Or |
            OpCode::Return | OpCode::Yield | OpCode::YieldFrom |
            OpCode::Await | OpCode::GetAwaitable |  // Async operations (no operand)
            OpCode::LoadSubscr | OpCode::StoreSubscr | OpCode::DeleteSubscr |
            OpCode::GetIter | OpCode::PopExcept | OpCode::Raise | OpCode::Reraise |
            OpCode::Ref | OpCode::RefMut | OpCode::Deref | OpCode::Move | OpCode::Copy | OpCode::Drop |
            OpCode::MatchGuard | OpCode::Halt => 1,

            // Pattern matching with u8 operand
            OpCode::MatchSequence | OpCode::MatchMapping | OpCode::MatchClass |
            OpCode::MatchAs | OpCode::MatchOr | OpCode::MatchStar => 2,

            OpCode::LoadFast | OpCode::StoreFast | OpCode::Call | OpCode::CallKw |
            OpCode::UnpackSequence | OpCode::UnpackEx | OpCode::BuildSlice |
            OpCode::ListAppend => 2,

            OpCode::LoadLocal | OpCode::StoreLocal |
            OpCode::LoadGlobal | OpCode::StoreGlobal |
            OpCode::LoadName | OpCode::StoreName |
            OpCode::LoadAttr | OpCode::StoreAttr | OpCode::DeleteAttr |
            OpCode::LoadConst |
            OpCode::Jump | OpCode::JumpIfTrue | OpCode::JumpIfFalse |
            OpCode::JumpIfTrueOrPop | OpCode::JumpIfFalseOrPop |
            OpCode::BuildList | OpCode::BuildTuple | OpCode::BuildDict |
            OpCode::BuildSet | OpCode::BuildString |
            OpCode::ForIter | OpCode::SetupTry | OpCode::TryUnwrap |
            OpCode::SetupAsyncWith | OpCode::EndAsyncFor |  // Async operations with i16 offset
            OpCode::Import | OpCode::ImportFrom | OpCode::ImportStar => 3,

            OpCode::LoadInt => 9,   // 1 + 8 bytes for i64
            OpCode::LoadFloat => 9, // 1 + 8 bytes for f64

            OpCode::MakeFunction => 6, // 1 + 2 (code const) + 2 (name const) + 1 (arity)
        }
    }
}

/// A single bytecode instruction.
#[derive(Copy, Clone, Debug)]
pub struct Instruction {
    pub opcode: OpCode,
    pub operand: Operand,
    pub line: u32,
}

/// Instruction operand.
#[derive(Copy, Clone, Debug, Default)]
pub enum Operand {
    #[default]
    None,
    U8(u8),
    U16(u16),
    I16(i16),
    I64(i64),
    F64(f64),
    Const(usize),
    /// MakeFunction operand: (code_const_idx, name_const_idx, arity)
    MakeFunc(u16, u16, u8),
}

impl Instruction {
    pub fn new(opcode: OpCode) -> Self {
        Self {
            opcode,
            operand: Operand::None,
            line: 0,
        }
    }

    pub fn with_operand(opcode: OpCode, operand: Operand) -> Self {
        Self {
            opcode,
            operand,
            line: 0,
        }
    }

    pub fn with_line(mut self, line: u32) -> Self {
        self.line = line;
        self
    }
}

/// A compiled bytecode chunk.
#[derive(Clone, Debug, Default)]
pub struct Bytecode {
    /// The bytecode instructions.
    pub instructions: Vec<Instruction>,
    /// Constant pool.
    pub constants: Vec<Constant>,
    /// Name pool (for variables, attributes, etc).
    pub names: Vec<String>,
    /// Number of local variable slots.
    pub num_locals: u16,
    /// Number of parameters.
    pub num_params: u8,
    /// Function name.
    pub name: String,
    /// Source file name.
    pub filename: String,
    /// Line number table for debugging.
    pub line_table: Vec<(usize, u32)>,
}

/// A constant value in the constant pool.
#[derive(Clone, Debug)]
pub enum Constant {
    None,
    Bool(bool),
    Int(i64),
    BigInt(String),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<Constant>),
    Code(Box<Bytecode>),
}

impl Bytecode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Adds an instruction.
    pub fn emit(&mut self, instruction: Instruction) -> usize {
        let offset = self.instructions.len();
        self.instructions.push(instruction);
        offset
    }

    /// Adds a constant and returns its index.
    pub fn add_constant(&mut self, constant: Constant) -> usize {
        self.constants.push(constant);
        self.constants.len() - 1
    }

    /// Adds a name and returns its index.
    pub fn add_name(&mut self, name: impl Into<String>) -> u16 {
        let name = name.into();
        for (i, n) in self.names.iter().enumerate() {
            if n == &name {
                return i as u16;
            }
        }
        self.names.push(name);
        (self.names.len() - 1) as u16
    }

    /// Returns the current instruction offset.
    pub fn current_offset(&self) -> usize {
        self.instructions.len()
    }

    /// Patches a jump instruction at the given offset.
    pub fn patch_jump(&mut self, offset: usize, target: usize) {
        let delta = (target as isize - offset as isize - 1) as i16;
        self.instructions[offset].operand = Operand::I16(delta);
    }
}

/// Bytecode builder for compiling MIR to bytecode.
pub struct BytecodeBuilder<'a> {
    current: Bytecode,
    local_slots: FxHashMap<u32, u16>,
    next_slot: u16,
    label_offsets: FxHashMap<u32, usize>,
    pending_jumps: Vec<(usize, u32)>,
    interner: Option<&'a Interner>,
}

impl<'a> BytecodeBuilder<'a> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            current: Bytecode::new(name),
            local_slots: FxHashMap::default(),
            next_slot: 0,
            label_offsets: FxHashMap::default(),
            pending_jumps: Vec::new(),
            interner: None,
        }
    }

    pub fn with_interner(name: impl Into<String>, interner: &'a Interner) -> Self {
        Self {
            current: Bytecode::new(name),
            local_slots: FxHashMap::default(),
            next_slot: 0,
            label_offsets: FxHashMap::default(),
            pending_jumps: Vec::new(),
            interner: Some(interner),
        }
    }

    /// Resolve a symbol to its string name
    fn resolve_symbol(&self, symbol: Symbol) -> String {
        if let Some(interner) = self.interner {
            interner.resolve(symbol).unwrap_or("<unknown>").to_string()
        } else {
            // Fallback to raw ID if no interner
            format!("{}", symbol.as_raw())
        }
    }

    /// Compiles a MIR body to bytecode.
    pub fn compile(&mut self, body: &MirBody) -> Bytecode {
        // Allocate slots for parameters and locals
        for param in &body.params {
            self.allocate_local(param.local.id);
        }
        for local in &body.locals {
            self.allocate_local(local.id);
        }

        self.current.num_params = body.params.len() as u8;
        self.current.num_locals = self.next_slot;

        // Compile each block
        for block in &body.blocks {
            self.label_offsets.insert(block.id, self.current.current_offset());

            for stmt in &block.stmts {
                self.compile_stmt(stmt);
            }

            self.compile_terminator(&block.terminator);
        }

        // Patch pending jumps
        for (offset, target_block) in &self.pending_jumps {
            if let Some(&target_offset) = self.label_offsets.get(target_block) {
                self.current.patch_jump(*offset, target_offset);
            }
        }

        std::mem::take(&mut self.current)
    }

    fn allocate_local(&mut self, id: u32) -> u16 {
        if let Some(&slot) = self.local_slots.get(&id) {
            return slot;
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.local_slots.insert(id, slot);
        slot
    }

    fn get_local(&self, id: u32) -> u16 {
        *self.local_slots.get(&id).expect("local not allocated")
    }

    fn compile_stmt(&mut self, stmt: &roast_mir::MirStmt) {
        match &stmt.kind {
            MirStmtKind::Assign { place, value } => {
                // Check if there are projections (like index assignment)
                if !place.projections.is_empty() {
                    // Handle indexed assignment: obj[key] = value
                    // Stack layout for StoreSubscr: [..., obj, value, key] -> stores value in obj[key]
                    let base_slot = self.get_local(place.local);
                    // Load the base object
                    if base_slot < 256 {
                        self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(base_slot as u8)));
                    } else {
                        self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(base_slot)));
                    }
                    // Compile the value
                    self.compile_rvalue(value);
                    // Handle projections - for now just the last one for simple cases
                    if let Some(roast_mir::MirProjection::Index(idx_local)) = place.projections.last() {
                        let idx_slot = self.get_local(*idx_local);
                        if idx_slot < 256 {
                            self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(idx_slot as u8)));
                        } else {
                            self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(idx_slot)));
                        }
                        self.emit(Instruction::new(OpCode::StoreSubscr));
                    }
                } else {
                    // Simple assignment to local variable
                    self.compile_rvalue(value);
                    let slot = self.get_local(place.local);
                    if slot < 256 {
                        self.emit(Instruction::with_operand(OpCode::StoreFast, Operand::U8(slot as u8)));
                    } else {
                        self.emit(Instruction::with_operand(OpCode::StoreLocal, Operand::U16(slot)));
                    }
                }
            }
            MirStmtKind::ListAppend { list, value } => {
                // For list comprehension: load list first, compile value, then ListAppend
                let list_slot = self.get_local(list.local);
                // Load the list onto stack
                if list_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(list_slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(list_slot)));
                }
                // Compile the value (pushes value onto stack)
                self.compile_operand(value);
                // Stack is now: [..., list, value]
                // ListAppend(1) means: append TOS (value) to stack[TOS-1] (list)
                self.emit(Instruction::with_operand(OpCode::ListAppend, Operand::U8(1)));
                // The list on the stack is now modified. Store it back.
                if list_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::StoreFast, Operand::U8(list_slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::StoreLocal, Operand::U16(list_slot)));
                }
            }
            MirStmtKind::SetAttr { object, attr, value } => {
                // TODO: Full implementation
            }
            MirStmtKind::SetAttrIndex { object, attr, index, value } => {
                // Attribute assignment: obj.attr = value
                // Stack layout for StoreAttr: [..., obj, value] -> pops both
                self.compile_operand(object);
                self.compile_operand(value);
                let attr_str = self.interner
                    .map(|i| i.resolve(*attr).unwrap_or("<unknown>"))
                    .unwrap_or("<unknown>");
                let name_idx = self.current.add_name(attr_str);
                self.emit(Instruction::with_operand(OpCode::StoreAttr, Operand::U16(name_idx)));
            }
            MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) => {
                // These are hints for the borrow checker, no bytecode needed
            }
            MirStmtKind::TryEnd => {
                // Pop exception frame - use existing TryEnd opcode if available
                // For now, no bytecode as VM handles this differently
            }
            MirStmtKind::Nop => {}
        }
    }

    fn compile_rvalue(&mut self, rvalue: &MirRvalue) {
        match rvalue {
            MirRvalue::Use(operand) => {
                self.compile_operand(operand);
            }
            MirRvalue::Ref(place, mutable) => {
                let slot = self.get_local(place.local);
                if slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(slot)));
                }
                if *mutable {
                    self.emit(Instruction::new(OpCode::RefMut));
                } else {
                    self.emit(Instruction::new(OpCode::Ref));
                }
            }
            MirRvalue::BinaryOp(op, left, right) => {
                self.compile_operand(left);
                self.compile_operand(right);
                let opcode = match op {
                    MirBinOp::Add => OpCode::Add,
                    MirBinOp::Sub => OpCode::Sub,
                    MirBinOp::Mul => OpCode::Mul,
                    MirBinOp::Div => OpCode::Div,
                    MirBinOp::FloorDiv => OpCode::FloorDiv,
                    MirBinOp::Rem => OpCode::Mod,
                    MirBinOp::Pow => OpCode::Pow,
                    MirBinOp::BitAnd => OpCode::BitAnd,
                    MirBinOp::BitOr => OpCode::BitOr,
                    MirBinOp::BitXor => OpCode::BitXor,
                    MirBinOp::Shl => OpCode::Shl,
                    MirBinOp::Shr => OpCode::Shr,
                    MirBinOp::Eq => OpCode::Eq,
                    MirBinOp::Ne => OpCode::Ne,
                    MirBinOp::Lt => OpCode::Lt,
                    MirBinOp::Le => OpCode::Le,
                    MirBinOp::Gt => OpCode::Gt,
                    MirBinOp::Ge => OpCode::Ge,
                    MirBinOp::In => OpCode::In,
                    MirBinOp::NotIn => OpCode::NotIn,
                };
                self.emit(Instruction::new(opcode));
            }
            MirRvalue::UnaryOp(op, operand) => {
                self.compile_operand(operand);
                let opcode = match op {
                    MirUnaryOp::Neg => OpCode::Neg,
                    MirUnaryOp::Not => OpCode::Not,
                    MirUnaryOp::BitNot => OpCode::BitNot,
                };
                self.emit(Instruction::new(opcode));
            }
            MirRvalue::Aggregate(kind, operands) => {
                for op in operands {
                    self.compile_operand(op);
                }
                let count = operands.len() as u16;
                match kind {
                    roast_mir::MirAggregateKind::Tuple => {
                        self.emit(Instruction::with_operand(OpCode::BuildTuple, Operand::U16(count)));
                    }
                    roast_mir::MirAggregateKind::List => {
                        self.emit(Instruction::with_operand(OpCode::BuildList, Operand::U16(count)));
                    }
                    roast_mir::MirAggregateKind::Dict => {
                        self.emit(Instruction::with_operand(OpCode::BuildDict, Operand::U16(count / 2)));
                    }
                    roast_mir::MirAggregateKind::Set => {
                        self.emit(Instruction::with_operand(OpCode::BuildSet, Operand::U16(count)));
                    }
                    roast_mir::MirAggregateKind::Slice => {
                        // Slice is built from 3 operands: start, stop, step
                        self.emit(Instruction::new(OpCode::BuildSlice));
                    }
                    roast_mir::MirAggregateKind::Struct(_) => {
                        self.emit(Instruction::with_operand(OpCode::BuildTuple, Operand::U16(count)));
                    }
                    roast_mir::MirAggregateKind::Lambda { params, body } => {
                        // Compile the lambda body to bytecode
                        let lambda_name = format!("__lambda_{}", self.current.constants.len());
                        let mut lambda_compiler = BytecodeBuilder::with_interner(&lambda_name, self.interner.unwrap());
                        let lambda_bytecode = lambda_compiler.compile(body);

                        // Add the bytecode as a constant
                        let code_const_idx = self.current.add_constant(Constant::Code(Box::new(lambda_bytecode)));

                        // Add the name as a constant
                        let name_const_idx = self.current.add_constant(Constant::Str(lambda_name));

                        // Emit MakeFunction instruction
                        self.emit(Instruction::with_operand(
                            OpCode::MakeFunction,
                            Operand::MakeFunc(code_const_idx as u16, name_const_idx as u16, params.len() as u8),
                        ));
                    }
                }
            }
            MirRvalue::Attr(operand, attr_name) => {
                // Load the object
                self.compile_operand(operand);
                // Load attribute - resolve symbol to string
                let attr_str = self.interner
                    .map(|i| i.resolve(*attr_name).unwrap_or("<unknown>"))
                    .unwrap_or("<unknown>");
                let name_idx = self.current.add_name(attr_str);
                self.emit(Instruction::with_operand(OpCode::LoadAttr, Operand::U16(name_idx)));
            }
            MirRvalue::Len(place) => {
                let slot = self.get_local(place.local);
                if slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(slot)));
                }
                let name_idx = self.current.add_name("__len__");
                self.emit(Instruction::with_operand(OpCode::LoadAttr, Operand::U16(name_idx)));
                self.emit(Instruction::with_operand(OpCode::Call, Operand::U8(0)));
            }
            MirRvalue::Cast(operand, _ty) => {
                // For now, just load the operand (runtime will handle type casting)
                self.compile_operand(operand);
            }
            MirRvalue::Await(operand) => {
                // Compile the awaited value, then emit Await opcode
                self.compile_operand(operand);
                self.emit(Instruction::new(OpCode::Await));
            }
        }
    }

    fn compile_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                let slot = self.get_local(place.local);
                if slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(slot)));
                }
                // Handle projections
                for proj in &place.projections {
                    match proj {
                        roast_mir::MirProjection::Field(idx) => {
                            let idx = self.current.add_constant(Constant::Int(*idx as i64));
                            self.emit(Instruction::with_operand(OpCode::LoadConst, Operand::U16(idx as u16)));
                            self.emit(Instruction::new(OpCode::LoadSubscr));
                        }
                        roast_mir::MirProjection::Index(local) => {
                            let slot = self.get_local(*local);
                            if slot < 256 {
                                self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(slot as u8)));
                            } else {
                                self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(slot)));
                            }
                            self.emit(Instruction::new(OpCode::LoadSubscr));
                        }
                        roast_mir::MirProjection::Slice { lower, upper, step } => {
                            // Load slice bounds and call BuildSlice
                            let lower_slot = self.get_local(*lower);
                            let upper_slot = self.get_local(*upper);
                            let step_slot = self.get_local(*step);
                            // Load lower
                            if lower_slot < 256 {
                                self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(lower_slot as u8)));
                            } else {
                                self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(lower_slot)));
                            }
                            // Load upper
                            if upper_slot < 256 {
                                self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(upper_slot as u8)));
                            } else {
                                self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(upper_slot)));
                            }
                            // Load step
                            if step_slot < 256 {
                                self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(step_slot as u8)));
                            } else {
                                self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(step_slot)));
                            }
                            // Build slice and subscript
                            self.emit(Instruction::with_operand(OpCode::BuildSlice, Operand::U8(3)));
                            self.emit(Instruction::new(OpCode::LoadSubscr));
                        }
                        roast_mir::MirProjection::Deref => {
                            self.emit(Instruction::new(OpCode::Deref));
                        }
                    }
                }
                // If this is a move, emit Move opcode
                if matches!(operand, MirOperand::Move(_)) {
                    self.emit(Instruction::new(OpCode::Move));
                }
            }
            MirOperand::Constant(constant) => {
                match constant {
                    MirConstant::None => {
                        self.emit(Instruction::new(OpCode::LoadNone));
                    }
                    MirConstant::Bool(true) => {
                        self.emit(Instruction::new(OpCode::LoadTrue));
                    }
                    MirConstant::Bool(false) => {
                        self.emit(Instruction::new(OpCode::LoadFalse));
                    }
                    MirConstant::Int(n) => {
                        if *n >= i64::MIN as i128 && *n <= i64::MAX as i128 {
                            self.emit(Instruction::with_operand(OpCode::LoadInt, Operand::I64(*n as i64)));
                        } else {
                            let idx = self.current.add_constant(Constant::BigInt(n.to_string()));
                            self.emit(Instruction::with_operand(OpCode::LoadConst, Operand::U16(idx as u16)));
                        }
                    }
                    MirConstant::Float(f) => {
                        self.emit(Instruction::with_operand(OpCode::LoadFloat, Operand::F64(*f)));
                    }
                    MirConstant::Str(s) => {
                        let idx = self.current.add_constant(Constant::Str(s.clone()));
                        self.emit(Instruction::with_operand(OpCode::LoadConst, Operand::U16(idx as u16)));
                    }
                    MirConstant::Bytes(b) => {
                        let idx = self.current.add_constant(Constant::Bytes(b.clone()));
                        self.emit(Instruction::with_operand(OpCode::LoadConst, Operand::U16(idx as u16)));
                    }
                    MirConstant::Unit => {
                        self.emit(Instruction::new(OpCode::LoadNone));
                    }
                }
            }
            MirOperand::Global(name) => {
                // Emit LoadGlobal for global/builtin references
                let name_str = self.resolve_symbol(*name);
                let name_idx = self.current.add_name(name_str);
                self.emit(Instruction::with_operand(OpCode::LoadGlobal, Operand::U16(name_idx)));
            }
        }
    }

    fn compile_terminator(&mut self, term: &MirTerminator) {
        match term {
            MirTerminator::Return(operand) => {
                // If there's a return value, load it onto the stack
                if let Some(op) = operand {
                    self.compile_operand(op);
                }
                self.emit(Instruction::new(OpCode::Return));
            }
            MirTerminator::Goto(target) => {
                let offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                self.pending_jumps.push((offset, *target));
            }
            MirTerminator::SwitchInt { discr, targets, otherwise } => {
                self.compile_operand(discr);

                // For boolean discriminants, use JumpIfTrue directly for true case
                // This is the common case for if/while conditions
                if targets.len() == 1 && targets[0].0 == 1 {
                    // Simple boolean branch: true -> target, false -> otherwise
                    let offset = self.emit(Instruction::with_operand(OpCode::JumpIfTrue, Operand::I16(0)));
                    self.pending_jumps.push((offset, targets[0].1));
                    let else_offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                    self.pending_jumps.push((else_offset, *otherwise));
                } else {
                    // For integer switches, use comparison
                    for (value, target) in targets {
                        self.emit(Instruction::new(OpCode::Dup));
                        self.emit(Instruction::with_operand(OpCode::LoadInt, Operand::I64(*value as i64)));
                        self.emit(Instruction::new(OpCode::Eq));
                        let offset = self.emit(Instruction::with_operand(OpCode::JumpIfTrue, Operand::I16(0)));
                        self.pending_jumps.push((offset, *target));
                    }

                    // Jump to otherwise block
                    self.emit(Instruction::new(OpCode::Pop));
                    let offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                    self.pending_jumps.push((offset, *otherwise));
                }
            }
            MirTerminator::Call { func, args, destination, target, .. } => {
                self.compile_operand(func);
                for arg in args {
                    self.compile_operand(arg);
                }
                self.emit(Instruction::with_operand(OpCode::Call, Operand::U8(args.len() as u8)));

                // Store result
                let slot = self.get_local(destination.local);
                if slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::StoreFast, Operand::U8(slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::StoreLocal, Operand::U16(slot)));
                }

                // Jump to target
                if let Some(target) = target {
                    let offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                    self.pending_jumps.push((offset, *target));
                }
            }
            MirTerminator::Assert { cond, expected, target, msg } => {
                self.compile_operand(cond);
                if !expected {
                    self.emit(Instruction::new(OpCode::Not));
                }
                // If assertion passes, continue
                let offset = self.emit(Instruction::with_operand(OpCode::JumpIfTrue, Operand::I16(0)));
                self.pending_jumps.push((offset, *target));

                // Otherwise, raise assertion error
                let msg_idx = self.current.add_constant(Constant::Str(msg.clone()));
                self.emit(Instruction::with_operand(OpCode::LoadConst, Operand::U16(msg_idx as u16)));
                self.emit(Instruction::new(OpCode::Raise));
            }
            MirTerminator::Drop { place, target, .. } => {
                let slot = self.get_local(place.local);
                if slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(slot)));
                }
                self.emit(Instruction::new(OpCode::Drop));

                let offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                self.pending_jumps.push((offset, *target));
            }
            MirTerminator::ForIter { iter, loop_var, body, exit } => {
                // Load iterator onto stack
                let iter_slot = self.get_local(iter.local);
                if iter_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(iter_slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(iter_slot)));
                }

                // GetIter converts iterable to iterator (if not already)
                self.emit(Instruction::new(OpCode::GetIter));

                // Store iterator back (it may have been modified/wrapped)
                if iter_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::StoreFast, Operand::U8(iter_slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::StoreLocal, Operand::U16(iter_slot)));
                }

                // Load iterator again for ForIter
                if iter_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(iter_slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::LoadLocal, Operand::U16(iter_slot)));
                }

                // ForIter: gets next item, pushes it, or jumps to exit
                // The jump offset will be patched later
                let for_iter_offset = self.emit(Instruction::with_operand(OpCode::ForIter, Operand::I16(0)));
                self.pending_jumps.push((for_iter_offset, *exit));

                // Store the yielded value into loop variable
                let var_slot = self.get_local(*loop_var);
                if var_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::StoreFast, Operand::U8(var_slot as u8)));
                } else {
                    self.emit(Instruction::with_operand(OpCode::StoreLocal, Operand::U16(var_slot)));
                }

                // Update iterator back (ForIter modified it in-place)
                if iter_slot < 256 {
                    self.emit(Instruction::with_operand(OpCode::LoadFast, Operand::U8(iter_slot as u8)));
                    self.emit(Instruction::with_operand(OpCode::StoreFast, Operand::U8(iter_slot as u8)));
                }

                // Jump to body block
                let body_offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                self.pending_jumps.push((body_offset, *body));
            }
            MirTerminator::Unreachable => {
                self.emit(Instruction::new(OpCode::Halt));
            }
            MirTerminator::TryBegin { body: try_body, exit, .. } => {
                // Setup exception handler and jump to try body
                // TODO: Implement proper try/except bytecode generation
                let offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                self.pending_jumps.push((offset, *try_body));
                let _ = exit; // Will be used when implementing proper exception handling
            }
            MirTerminator::Raise { exc, .. } => {
                // Raise an exception
                if let Some(exc_op) = exc {
                    self.compile_operand(exc_op);
                    self.emit(Instruction::new(OpCode::Raise));
                } else {
                    // Re-raise current exception
                    self.emit(Instruction::new(OpCode::Reraise));
                }
            }
            MirTerminator::MethodCall { receiver, args, destination, target, .. } => {
                // Compile method call as regular call for bytecode
                // Push receiver (self) and then arguments
                self.compile_operand(receiver);
                for arg in args {
                    self.compile_operand(arg);
                }
                // Call with receiver + args count
                self.emit(Instruction::with_operand(OpCode::Call, Operand::U8((args.len() + 1) as u8)));
                // Store result
                if let Some(_dest_local) = Some(destination.local) {
                    // Store to local
                    self.emit(Instruction::with_operand(OpCode::StoreLocal, Operand::U8(destination.local as u8)));
                }
                // Jump to continuation
                if let Some(target_block) = target {
                    let offset = self.emit(Instruction::with_operand(OpCode::Jump, Operand::I16(0)));
                    self.pending_jumps.push((offset, *target_block));
                }
            }
        }
    }

    fn emit(&mut self, instruction: Instruction) -> usize {
        self.current.emit(instruction)
    }
}

impl fmt::Display for Bytecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Function: {}", self.name)?;
        writeln!(f, "  Params: {}, Locals: {}", self.num_params, self.num_locals)?;
        writeln!(f, "  Constants: {:?}", self.constants)?;
        writeln!(f, "  Names: {:?}", self.names)?;
        writeln!(f, "  Instructions:")?;
        for (i, instr) in self.instructions.iter().enumerate() {
            writeln!(f, "    {:04}: {:?} {:?}", i, instr.opcode, instr.operand)?;
        }
        Ok(())
    }
}
