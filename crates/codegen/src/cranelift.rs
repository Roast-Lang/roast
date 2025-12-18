//! Cranelift native code generation backend.
//!
//! This module provides native code generation using Cranelift,
//! a fast and portable code generator.

#[cfg(feature = "cranelift")]
mod backend {
    use cranelift_codegen::ir::{
        types, AbiParam, Block, InstBuilder, Type as ClifType,
        Value, MemFlags, UserFuncName, FuncRef,
    };
    use cranelift_codegen::isa::TargetIsa;
    use cranelift_codegen::settings::{self, Configurable, Flags};
    use cranelift_codegen::Context;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
    use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
    use cranelift_object::{ObjectBuilder, ObjectModule, ObjectProduct};
    use target_lexicon::Triple;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Arc;

    use roast_mir::*;
    use roast_typer::Type;
    use crate::{CodegenError, CodegenResult, OptLevel};

    /// Cranelift code generator.
    pub struct CraneliftBackend {
        /// Target ISA.
        isa: Arc<dyn TargetIsa>,
        /// Object module for code generation.
        module: ObjectModule,
        /// Function context.
        ctx: Context,
        /// Function builder context (reusable).
        func_ctx: FunctionBuilderContext,
        /// Data descriptions for constants.
        _data_desc: DataDescription,
        /// Compiled functions.
        pub compiled_funcs: HashMap<String, FuncId>,
        /// Pre-declared function signatures (for forward references).
        pub declared_funcs: HashMap<String, FuncId>,
        /// String constants.
        _string_constants: HashMap<String, DataId>,
        /// Optimization level.
        _opt_level: OptLevel,
    }

    impl CraneliftBackend {
        /// Create a new Cranelift backend.
        pub fn new(target_triple: Option<&str>, opt_level: OptLevel) -> CodegenResult<Self> {
            let _triple = match target_triple {
                Some(t) => Triple::from_str(t)
                    .map_err(|e| CodegenError::Target(format!("Invalid target: {}", e)))?,
                None => Triple::host(),
            };

            let mut flag_builder = settings::builder();

            // Set optimization level
            match opt_level {
                OptLevel::None => {
                    flag_builder.set("opt_level", "none").unwrap();
                }
                OptLevel::Less => {
                    flag_builder.set("opt_level", "speed").unwrap();
                }
                OptLevel::Default | OptLevel::Aggressive => {
                    flag_builder.set("opt_level", "speed_and_size").unwrap();
                }
                OptLevel::Size | OptLevel::SizeMin => {
                    flag_builder.set("opt_level", "speed_and_size").unwrap();
                }
            }

            // Enable position-independent code
            flag_builder.set("is_pic", "true").unwrap();

            let flags = Flags::new(flag_builder);

            let isa = cranelift_native::builder()
                .map_err(|e| CodegenError::Target(e.to_string()))?
                .finish(flags.clone())
                .map_err(|e| CodegenError::Target(e.to_string()))?;

            let object_builder = ObjectBuilder::new(
                isa.clone(),
                "roast_module",
                cranelift_module::default_libcall_names(),
            ).map_err(|e| CodegenError::Target(e.to_string()))?;

            let module = ObjectModule::new(object_builder);
            let ctx = module.make_context();
            let func_ctx = FunctionBuilderContext::new();
            let data_desc = DataDescription::new();

            Ok(Self {
                isa,
                module,
                ctx,
                func_ctx,
                _data_desc: data_desc,
                compiled_funcs: HashMap::new(),
                declared_funcs: HashMap::new(),
                _string_constants: HashMap::new(),
                _opt_level: opt_level,
            })
        }

        /// Compile a MIR body to native code.
        pub fn compile_function(&mut self, body: &MirBody) -> CodegenResult<FuncId> {
            // Generate function name from symbol index
            let func_name = format!("roast_fn_{}", body.name.as_raw());

            // Check if already compiled
            if let Some(&id) = self.compiled_funcs.get(&func_name) {
                return Ok(id);
            }

            // Create function signature
            let mut sig = self.module.make_signature();

            // Add parameters
            for param in &body.params {
                let ty = self.roast_type_to_clif(&param.local.ty);
                sig.params.push(AbiParam::new(ty));
            }

            // Add return type
            let ret_ty = self.roast_type_to_clif(&body.return_ty);
            if ret_ty != types::INVALID {
                sig.returns.push(AbiParam::new(ret_ty));
            }

            // Declare function
            let func_id = self.module
                .declare_function(&func_name, Linkage::Export, &sig.clone())
                .map_err(|e| CodegenError::Internal(e.to_string()))?;

            // Set up function
            self.ctx.func.signature = sig.clone();
            self.ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            // Build function body
            {
                let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.func_ctx);

                // Pre-import function references for recursive/mutual calls
                let mut func_refs: HashMap<String, FuncRef> = HashMap::new();

                // Declare this function for recursive calls
                // We need to declare it using declare_func_in_func which properly handles the external name
                let self_func_ref = self.module.declare_func_in_func(func_id, builder.func);
                func_refs.insert(func_name.clone(), self_func_ref);

                // Declare ALL pre-declared functions (including forward references)
                for (other_name, &other_id) in &self.declared_funcs {
                    if other_name != &func_name {
                        let other_ref = self.module.declare_func_in_func(other_id, builder.func);
                        func_refs.insert(other_name.clone(), other_ref);
                    }
                }

                // Now create translator and pass the pre-built func_refs
                let mut translator = FunctionTranslator::new(&mut builder, body);
                translator.func_refs = func_refs;
                translator.translate()?;
                builder.finalize();
            }

            // Compile function
            self.module
                .define_function(func_id, &mut self.ctx)
                .map_err(|e| CodegenError::Internal(format!("Compilation failed: {:?}", e)))?;

            // Clear context for reuse
            self.module.clear_context(&mut self.ctx);

            self.compiled_funcs.insert(func_name.to_string(), func_id);

            Ok(func_id)
        }

        /// Compile all functions in a module using two-pass approach.
        /// Pass 1: Declare all function signatures (enables forward references).
        /// Pass 2: Compile all function bodies.
        pub fn compile_module(&mut self, bodies: &[MirBody]) -> CodegenResult<()> {
            // Pass 1: Declare all functions first
            for body in bodies {
                self.declare_function(body)?;
            }
            
            // Pass 2: Compile all function bodies
            for body in bodies {
                self.compile_function(body)?;
            }
            Ok(())
        }
        
        /// Declare a function signature without compiling the body.
        /// This allows forward references to functions defined later.
        fn declare_function(&mut self, body: &MirBody) -> CodegenResult<FuncId> {
            let func_name = format!("roast_fn_{}", body.name.as_raw());
            
            // Check if already declared
            if let Some(&id) = self.declared_funcs.get(&func_name) {
                return Ok(id);
            }
            
            // Create function signature
            let mut sig = self.module.make_signature();
            
            // Add parameters
            for param in &body.params {
                let ty = self.roast_type_to_clif(&param.local.ty);
                sig.params.push(AbiParam::new(ty));
            }
            
            // Add return type
            let ret_ty = self.roast_type_to_clif(&body.return_ty);
            if ret_ty != types::INVALID {
                sig.returns.push(AbiParam::new(ret_ty));
            }
            
            // Declare function
            let func_id = self.module
                .declare_function(&func_name, Linkage::Export, &sig)
                .map_err(|e| CodegenError::Internal(e.to_string()))?;
            
            self.declared_funcs.insert(func_name, func_id);
            
            Ok(func_id)
        }

        /// Generate a C main() entry point that calls the Roast main function.
        pub fn generate_entry_point(&mut self, roast_main_id: FuncId) -> CodegenResult<FuncId> {
            // Create main signature: int main(int argc, char **argv)
            let mut sig = self.module.make_signature();
            sig.params.push(AbiParam::new(types::I32)); // argc
            sig.params.push(AbiParam::new(self.isa.pointer_type())); // argv
            sig.returns.push(AbiParam::new(types::I32)); // return code

            let func_id = self.module
                .declare_function("main", Linkage::Export, &sig)
                .map_err(|e| CodegenError::Internal(e.to_string()))?;

            self.ctx.func.signature = sig;
            self.ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            {
                let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.func_ctx);

                let entry_block = builder.create_block();
                builder.append_block_params_for_function_params(entry_block);
                builder.switch_to_block(entry_block);
                builder.seal_block(entry_block);

                // Call the Roast main function
                let roast_main_ref = self.module.declare_func_in_func(roast_main_id, builder.func);
                let call = builder.ins().call(roast_main_ref, &[]);
                let results = builder.inst_results(call);

                // Return the result (or 0 if void)
                let ret_val = if results.is_empty() {
                    builder.ins().iconst(types::I32, 0)
                } else {
                    // Convert i64 result to i32
                    let result = results[0];
                    builder.ins().ireduce(types::I32, result)
                };

                builder.ins().return_(&[ret_val]);
                builder.finalize();
            }

            self.module
                .define_function(func_id, &mut self.ctx)
                .map_err(|e| CodegenError::Internal(format!("Failed to compile main: {:?}", e)))?;

            self.module.clear_context(&mut self.ctx);

            Ok(func_id)
        }

        /// Finalize and get the object file.
        pub fn finish(self) -> CodegenResult<ObjectProduct> {
            Ok(self.module.finish())
        }

        /// Convert Roast type to Cranelift type.
        fn roast_type_to_clif(&self, ty: &Type) -> ClifType {
            match ty {
                Type::Bool => types::I8,
                Type::Int | Type::Int64 => types::I64,
                Type::Int8 => types::I8,
                Type::Int16 => types::I16,
                Type::Int32 => types::I32,
                Type::Int128 => types::I128,
                Type::UInt | Type::UInt64 => types::I64,
                Type::UInt8 => types::I8,
                Type::UInt16 => types::I16,
                Type::UInt32 => types::I32,
                Type::UInt128 => types::I128,
                Type::Float | Type::Float64 => types::F64,
                Type::Float32 => types::F32,
                // Pointers for reference types
                Type::Str | Type::Bytes | Type::List(_) | Type::Dict(_, _) |
                Type::Set(_) | Type::Class(_) | Type::Ref { .. } | Type::Owned(_) => {
                    self.isa.pointer_type()
                }
                Type::NoneType | Type::Unknown | Type::Never => types::INVALID,
                Type::Tuple(elts) if elts.is_empty() => types::INVALID,
                _ => self.isa.pointer_type(), // Default to pointer for complex types
            }
        }
    }

    /// Translates a single MIR function to Cranelift IR.
    struct FunctionTranslator<'a, 'b> {
        builder: &'a mut FunctionBuilder<'b>,
        body: &'a MirBody,
        /// Maps MIR locals to Cranelift variables.
        variables: HashMap<LocalId, Variable>,
        /// Maps MIR blocks to Cranelift blocks.
        blocks: HashMap<BlockId, Block>,
        /// Current block being built.
        current_block: Option<Block>,
        /// Declared function references for calling (func_name -> FuncRef).
        func_refs: HashMap<String, cranelift_codegen::ir::FuncRef>,
    }

    impl<'a, 'b> FunctionTranslator<'a, 'b> {
        fn new(builder: &'a mut FunctionBuilder<'b>, body: &'a MirBody) -> Self {
            Self {
                builder,
                body,
                variables: HashMap::new(),
                blocks: HashMap::new(),
                current_block: None,
                func_refs: HashMap::new(),
            }
        }

        fn translate(&mut self) -> CodegenResult<()> {
            // Declare all blocks first
            for block in &self.body.blocks {
                let clif_block = self.builder.create_block();
                self.blocks.insert(block.id, clif_block);
            }

            // Declare variables for locals
            for (idx, local) in self.body.locals.iter().enumerate() {
                let var = Variable::from_u32(idx as u32);
                let ty = self.type_to_clif(&local.ty);
                self.builder.declare_var(var, ty);
                self.variables.insert(local.id, var);
            }

            // Also declare parameters as variables
            for (idx, param) in self.body.params.iter().enumerate() {
                let var = Variable::from_u32((self.body.locals.len() + idx) as u32);
                let ty = self.type_to_clif(&param.local.ty);
                self.builder.declare_var(var, ty);
                self.variables.insert(param.local.id, var);
            }

            // Entry block
            let entry_block = self.blocks[&0];
            self.builder.append_block_params_for_function_params(entry_block);
            self.builder.switch_to_block(entry_block);
            self.builder.seal_block(entry_block);

            // Initialize parameters
            for (idx, param) in self.body.params.iter().enumerate() {
                let param_val = self.builder.block_params(entry_block)[idx];
                let var = self.variables[&param.local.id];
                self.builder.def_var(var, param_val);
            }

            // Translate all blocks
            for mir_block in &self.body.blocks {
                self.translate_block(mir_block)?;
            }

            Ok(())
        }

        fn translate_block(&mut self, block: &MirBlock) -> CodegenResult<()> {
            let clif_block = self.blocks[&block.id];

            // Switch to block if not entry (entry already switched)
            if block.id != 0 {
                self.builder.switch_to_block(clif_block);
            }

            self.current_block = Some(clif_block);

            // Translate statements
            for stmt in &block.stmts {
                self.translate_stmt(stmt)?;
            }

            // Translate terminator
            self.translate_terminator(&block.terminator)?;

            // Seal block
            self.builder.seal_block(clif_block);

            Ok(())
        }

        fn translate_stmt(&mut self, stmt: &MirStmt) -> CodegenResult<()> {
            match &stmt.kind {
                MirStmtKind::Assign { place, value } => {
                    let val = self.translate_rvalue(value)?;
                    self.store_place(place, val)?;
                }
                MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) => {
                    // These are hints, ignore for now
                }
                MirStmtKind::ListAppend { .. } => {
                    // List comprehension append - not supported in cranelift backend yet
                    // Would require runtime list manipulation
                }
                MirStmtKind::SetAttr { .. } => {
                    // Attribute assignment - not supported in cranelift backend yet
                    // Would require runtime object manipulation
                }
                MirStmtKind::SetAttrIndex { .. } => {
                    // Indexed attribute assignment - not supported in cranelift backend yet
                    // Would require runtime object and list manipulation
                }
                MirStmtKind::TryEnd => {
                    // Pop exception frame - not yet supported in cranelift backend
                    // Would require runtime exception handling with setjmp/longjmp
                }
                MirStmtKind::Nop => {}
            }
            Ok(())
        }

        fn translate_rvalue(&mut self, rvalue: &MirRvalue) -> CodegenResult<Value> {
            match rvalue {
                MirRvalue::Use(operand) => self.translate_operand(operand),

                MirRvalue::UnaryOp(op, operand) => {
                    let val = self.translate_operand(operand)?;
                    match op {
                        MirUnaryOp::Neg => {
                            let ty = self.builder.func.dfg.value_type(val);
                            if ty.is_int() {
                                Ok(self.builder.ins().ineg(val))
                            } else {
                                Ok(self.builder.ins().fneg(val))
                            }
                        }
                        MirUnaryOp::Not => {
                            let zero = self.builder.ins().iconst(types::I8, 0);
                            Ok(self.builder.ins().icmp(
                                cranelift_codegen::ir::condcodes::IntCC::Equal,
                                val,
                                zero,
                            ))
                        }
                        MirUnaryOp::BitNot => {
                            Ok(self.builder.ins().bnot(val))
                        }
                    }
                }

                MirRvalue::BinaryOp(op, lhs, rhs) => {
                    let lhs_val = self.translate_operand(lhs)?;
                    let rhs_val = self.translate_operand(rhs)?;
                    self.translate_binop(*op, lhs_val, rhs_val)
                }

                MirRvalue::Ref(place, _mutable) => {
                    // For now, just load the place
                    self.load_place(place)
                }

                MirRvalue::Aggregate(_kind, operands) => {
                    // For now, just return first operand or zero
                    if let Some(first) = operands.first() {
                        self.translate_operand(first)
                    } else {
                        Ok(self.builder.ins().iconst(types::I64, 0))
                    }
                }

                MirRvalue::Len(place) => {
                    // Placeholder: return 0
                    let _ = self.load_place(place)?;
                    Ok(self.builder.ins().iconst(types::I64, 0))
                }

                MirRvalue::Cast(operand, target_ty) => {
                    let val = self.translate_operand(operand)?;
                    let from_ty = self.builder.func.dfg.value_type(val);
                    let to_ty = self.type_to_clif(target_ty);

                    if from_ty == to_ty {
                        Ok(val)
                    } else if from_ty.is_int() && to_ty.is_int() {
                        if to_ty.bits() > from_ty.bits() {
                            Ok(self.builder.ins().sextend(to_ty, val))
                        } else {
                            Ok(self.builder.ins().ireduce(to_ty, val))
                        }
                    } else if from_ty.is_float() && to_ty.is_int() {
                        Ok(self.builder.ins().fcvt_to_sint(to_ty, val))
                    } else if from_ty.is_int() && to_ty.is_float() {
                        Ok(self.builder.ins().fcvt_from_sint(to_ty, val))
                    } else {
                        // Bitcast or pointer conversion
                        Ok(self.builder.ins().bitcast(to_ty, MemFlags::new(), val))
                    }
                }

                MirRvalue::Attr(_operand, _attr) => {
                    // Placeholder: attribute access not yet supported in Cranelift backend
                    Ok(self.builder.ins().iconst(types::I64, 0))
                }

                MirRvalue::Await(operand) => {
                    // Await: for now, just evaluate the operand
                    // Real async support would require runtime integration
                    self.translate_operand(operand)
                }
            }
        }

        fn translate_binop(
            &mut self,
            op: MirBinOp,
            lhs: Value,
            rhs: Value,
        ) -> CodegenResult<Value> {
            use MirBinOp::*;
            use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};

            let ty = self.builder.func.dfg.value_type(lhs);
            let is_float = ty.is_float();

            Ok(match op {
                Add if is_float => self.builder.ins().fadd(lhs, rhs),
                Add => self.builder.ins().iadd(lhs, rhs),

                Sub if is_float => self.builder.ins().fsub(lhs, rhs),
                Sub => self.builder.ins().isub(lhs, rhs),

                Mul if is_float => self.builder.ins().fmul(lhs, rhs),
                Mul => self.builder.ins().imul(lhs, rhs),

                Div if is_float => self.builder.ins().fdiv(lhs, rhs),
                Div => self.builder.ins().sdiv(lhs, rhs),

                FloorDiv if is_float => {
                    // Floor division for floats: floor(lhs / rhs)
                    let div_result = self.builder.ins().fdiv(lhs, rhs);
                    self.builder.ins().floor(div_result)
                }
                FloorDiv => self.builder.ins().sdiv(lhs, rhs),

                Rem => self.builder.ins().srem(lhs, rhs),

                Pow => {
                    // Power requires a function call, return lhs for now
                    // TODO: Call power function
                    lhs
                }

                BitAnd => self.builder.ins().band(lhs, rhs),
                BitOr => self.builder.ins().bor(lhs, rhs),
                BitXor => self.builder.ins().bxor(lhs, rhs),

                Shl => self.builder.ins().ishl(lhs, rhs),
                Shr => self.builder.ins().sshr(lhs, rhs),

                Eq if is_float => {
                    self.builder.ins().fcmp(FloatCC::Equal, lhs, rhs)
                }
                Eq => self.builder.ins().icmp(IntCC::Equal, lhs, rhs),

                Ne if is_float => {
                    self.builder.ins().fcmp(FloatCC::NotEqual, lhs, rhs)
                }
                Ne => self.builder.ins().icmp(IntCC::NotEqual, lhs, rhs),

                Lt if is_float => {
                    self.builder.ins().fcmp(FloatCC::LessThan, lhs, rhs)
                }
                Lt => self.builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs),

                Le if is_float => {
                    self.builder.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs)
                }
                Le => self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs),

                Gt if is_float => {
                    self.builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs)
                }
                Gt => self.builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs),

                Ge if is_float => {
                    self.builder.ins().fcmp(FloatCC::GreaterThanOrEqual, lhs, rhs)
                }
                Ge => self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs),

                // In/NotIn operations - need runtime function calls for proper implementation
                // For now, return false (0) as placeholder
                In | NotIn => {
                    // TODO: Implement proper In/NotIn operations via runtime calls
                    self.builder.ins().iconst(types::I64, 0)
                }
            })
        }

        fn translate_operand(&mut self, operand: &MirOperand) -> CodegenResult<Value> {
            match operand {
                MirOperand::Copy(place) | MirOperand::Move(place) => {
                    self.load_place(place)
                }
                MirOperand::Constant(constant) => self.translate_constant(constant),
                MirOperand::Global(name) => {
                    Err(CodegenError::Unsupported(format!("Global operand '{}' not supported in Cranelift backend", name)))
                }
            }
        }

        fn translate_constant(&mut self, constant: &MirConstant) -> CodegenResult<Value> {
            match constant {
                MirConstant::Int(i) => {
                    Ok(self.builder.ins().iconst(types::I64, *i as i64))
                }
                MirConstant::Float(f) => {
                    Ok(self.builder.ins().f64const(*f))
                }
                MirConstant::Bool(b) => {
                    Ok(self.builder.ins().iconst(types::I8, if *b { 1 } else { 0 }))
                }
                MirConstant::Str(_s) => {
                    // Strings are pointers, return null for now
                    // TODO: Create data section for strings
                    Ok(self.builder.ins().iconst(types::I64, 0))
                }
                MirConstant::Bytes(_b) => {
                    Ok(self.builder.ins().iconst(types::I64, 0))
                }
                MirConstant::None | MirConstant::Unit => {
                    Ok(self.builder.ins().iconst(types::I64, 0))
                }
            }
        }

        fn load_place(&mut self, place: &MirPlace) -> CodegenResult<Value> {
            if let Some(&var) = self.variables.get(&place.local) {
                // Simple local variable
                if place.projections.is_empty() {
                    Ok(self.builder.use_var(var))
                } else {
                    // Handle projections (field access, indexing)
                    let base = self.builder.use_var(var);
                    self.apply_projections(base, &place.projections)
                }
            } else {
                // Unknown local, return zero
                Ok(self.builder.ins().iconst(types::I64, 0))
            }
        }

        fn store_place(&mut self, place: &MirPlace, value: Value) -> CodegenResult<()> {
            if let Some(&var) = self.variables.get(&place.local) {
                if place.projections.is_empty() {
                    self.builder.def_var(var, value);
                } else {
                    // Handle projections (would need memory stores)
                    // For now, just store to base
                    self.builder.def_var(var, value);
                }
            }
            Ok(())
        }

        fn apply_projections(
            &mut self,
            base: Value,
            projections: &[MirProjection],
        ) -> CodegenResult<Value> {
            let mut current = base;

            for proj in projections {
                match proj {
                    MirProjection::Field(idx) => {
                        // Field access: offset from base
                        let offset = (*idx as i64) * 8; // Assume 8-byte fields
                        current = self.builder.ins().iadd_imm(current, offset);
                        current = self.builder.ins().load(
                            types::I64,
                            MemFlags::new(),
                            current,
                            0,
                        );
                    }
                    MirProjection::Index(idx_local) => {
                        if let Some(&var) = self.variables.get(idx_local) {
                            let idx = self.builder.use_var(var);
                            let offset = self.builder.ins().imul_imm(idx, 8);
                            current = self.builder.ins().iadd(current, offset);
                            current = self.builder.ins().load(
                                types::I64,
                                MemFlags::new(),
                                current,
                                0,
                            );
                        }
                    }
                    MirProjection::Slice { lower, upper, step: _ } => {
                        // For Cranelift, we'd need to call a runtime function for slicing
                        // For now, just return the base (placeholder)
                        if let Some(&lower_var) = self.variables.get(lower) {
                            let _lower_val = self.builder.use_var(lower_var);
                        }
                        if let Some(&upper_var) = self.variables.get(upper) {
                            let _upper_val = self.builder.use_var(upper_var);
                        }
                        // TODO: Call runtime slice function
                    }
                    MirProjection::Deref => {
                        current = self.builder.ins().load(
                            types::I64,
                            MemFlags::new(),
                            current,
                            0,
                        );
                    }
                }
            }

            Ok(current)
        }

        fn translate_terminator(&mut self, term: &MirTerminator) -> CodegenResult<()> {
            match term {
                MirTerminator::Return(operand) => {
                    // Get return value from operand if provided, else from local 0
                    if let Some(op) = operand {
                        let ret_val = self.translate_operand(op)?;
                        self.builder.ins().return_(&[ret_val]);
                    } else if let Some(&var) = self.variables.get(&0) {
                        let ret_val = self.builder.use_var(var);
                        self.builder.ins().return_(&[ret_val]);
                    } else {
                        self.builder.ins().return_(&[]);
                    }
                }

                MirTerminator::Goto(target) => {
                    let target_block = self.blocks[target];
                    self.builder.ins().jump(target_block, &[]);
                }

                MirTerminator::SwitchInt { discr, targets, otherwise } => {
                    let val = self.translate_operand(discr)?;
                    let val_ty = self.builder.func.dfg.value_type(val);

                    // Build switch
                    let otherwise_block = self.blocks[otherwise];

                    if targets.len() == 1 {
                        // Simple if-else
                        let (test_val, target) = &targets[0];
                        let target_block = self.blocks[target];

                        // Create constant with matching type
                        let test = self.builder.ins().iconst(val_ty, *test_val as i64);
                        let cmp = self.builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            val,
                            test,
                        );
                        self.builder.ins().brif(cmp, target_block, &[], otherwise_block, &[]);
                    } else {
                        // Multi-way branch
                        // For now, use cascading if-else
                        for (test_val, target) in targets {
                            let target_block = self.blocks[target];
                            let test = self.builder.ins().iconst(val_ty, *test_val as i64);
                            let cmp = self.builder.ins().icmp(
                                cranelift_codegen::ir::condcodes::IntCC::Equal,
                                val,
                                test,
                            );
                            let next_block = self.builder.create_block();
                            self.builder.ins().brif(cmp, target_block, &[], next_block, &[]);
                            self.builder.switch_to_block(next_block);
                            self.builder.seal_block(next_block);
                        }
                        self.builder.ins().jump(otherwise_block, &[]);
                    }
                }

                MirTerminator::Call { func, args, destination, target, unwind: _ } => {
                    // Translate arguments
                    let mut arg_vals = Vec::new();
                    for arg in args {
                        arg_vals.push(self.translate_operand(arg)?);
                    }

                    // Get the function to call
                    let result = match func {
                        MirOperand::Global(func_sym) => {
                            // Check if we have a func_ref for this function
                            let func_name = format!("roast_fn_{}", func_sym.as_raw());

                            if let Some(&func_ref) = self.func_refs.get(&func_name) {
                                // Call the function
                                let call = self.builder.ins().call(func_ref, &arg_vals);
                                let results = self.builder.inst_results(call);
                                if results.is_empty() {
                                    self.builder.ins().iconst(types::I64, 0)
                                } else {
                                    results[0]
                                }
                            } else {
                                // Function not declared yet - for recursive calls,
                                // we should have declared self. Return 0 as fallback.
                                self.builder.ins().iconst(types::I64, 0)
                            }
                        }
                        _ => {
                            // Indirect call through a variable - not yet supported
                            self.builder.ins().iconst(types::I64, 0)
                        }
                    };

                    self.store_place(destination, result)?;

                    // Jump to target block
                    if let Some(target) = target {
                        let target_block = self.blocks[target];
                        self.builder.ins().jump(target_block, &[]);
                    }
                }

                MirTerminator::Assert { cond, expected, target, msg: _ } => {
                    let cond_val = self.translate_operand(cond)?;
                    let target_block = self.blocks[target];

                    if *expected {
                        // Assert true: trap if false
                        let trap_block = self.builder.create_block();
                        self.builder.ins().brif(cond_val, target_block, &[], trap_block, &[]);
                        self.builder.switch_to_block(trap_block);
                        self.builder.seal_block(trap_block);
                        self.builder.ins().trap(cranelift_codegen::ir::TrapCode::User(0));
                    } else {
                        // Assert false: trap if true
                        let trap_block = self.builder.create_block();
                        self.builder.ins().brif(cond_val, trap_block, &[], target_block, &[]);
                        self.builder.switch_to_block(trap_block);
                        self.builder.seal_block(trap_block);
                        self.builder.ins().trap(cranelift_codegen::ir::TrapCode::User(0));
                    }
                }

                MirTerminator::Drop { place: _, target, unwind: _ } => {
                    // For now, just jump to target
                    let target_block = self.blocks[target];
                    self.builder.ins().jump(target_block, &[]);
                }

                MirTerminator::ForIter { iter: _, loop_var: _, body, exit: _ } => {
                    // ForIter is complex - for native codegen we'd need runtime support
                    // For now, just jump to body (single iteration - placeholder)
                    let body_block = self.blocks[body];
                    self.builder.ins().jump(body_block, &[]);
                }

                MirTerminator::Unreachable => {
                    self.builder.ins().trap(cranelift_codegen::ir::TrapCode::User(1));
                }

                MirTerminator::TryBegin { body: try_body, .. } => {
                    // For Cranelift, we can't easily support setjmp/longjmp
                    // For now, just jump to try body (exceptions not caught)
                    let try_block = self.blocks[try_body];
                    self.builder.ins().jump(try_block, &[]);
                }

                MirTerminator::Raise { .. } => {
                    // Raise an exception - for Cranelift, just trap
                    self.builder.ins().trap(cranelift_codegen::ir::TrapCode::User(2));
                }

                MirTerminator::MethodCall { receiver, args, destination, target, .. } => {
                    // For Cranelift, compile method call as regular call
                    // In practice we'd need to handle method lookup, but for now just call
                    let recv_val = self.translate_operand(receiver)?;
                    let mut arg_vals = vec![recv_val];
                    for arg in args {
                        arg_vals.push(self.translate_operand(arg)?);
                    }

                    // We'd need a proper function reference here - just store receiver for now
                    let dest_var = Variable::from_u32(destination.local);
                    self.builder.def_var(dest_var, recv_val);

                    // Jump to continuation
                    if let Some(target_block) = target {
                        let target_blk = self.blocks[target_block];
                        self.builder.ins().jump(target_blk, &[]);
                    }
                }
            }

            Ok(())
        }

        fn type_to_clif(&self, ty: &Type) -> ClifType {
            match ty {
                Type::Bool => types::I8,
                Type::Int | Type::Int64 => types::I64,
                Type::Int8 => types::I8,
                Type::Int16 => types::I16,
                Type::Int32 => types::I32,
                Type::Int128 => types::I128,
                Type::UInt | Type::UInt64 => types::I64,
                Type::UInt8 => types::I8,
                Type::UInt16 => types::I16,
                Type::UInt32 => types::I32,
                Type::UInt128 => types::I128,
                Type::Float | Type::Float64 => types::F64,
                Type::Float32 => types::F32,
                Type::NoneType | Type::Unknown | Type::Never => types::I64,
                _ => types::I64, // Default to i64 for complex types
            }
        }
    }

    /// Write object file to bytes.
    pub fn write_object(product: ObjectProduct) -> CodegenResult<Vec<u8>> {
        Ok(product.emit().map_err(|e| CodegenError::Internal(e.to_string()))?)
    }

    /// Link object files into an executable.
    pub fn link_executable(
        objects: &[Vec<u8>],
        output_path: &str,
        link_args: &[String],
    ) -> CodegenResult<()> {
        use std::process::Command;
        use std::io::Write;

        // Write objects to temp files
        let temp_dir = std::env::temp_dir();
        let mut obj_paths = Vec::new();

        for (i, obj) in objects.iter().enumerate() {
            let path = temp_dir.join(format!("roast_obj_{}.o", i));
            let mut file = std::fs::File::create(&path)
                .map_err(|e| CodegenError::Internal(e.to_string()))?;
            file.write_all(obj)
                .map_err(|e| CodegenError::Internal(e.to_string()))?;
            obj_paths.push(path);
        }

        // Invoke system linker
        let linker = if cfg!(target_os = "macos") {
            "clang"
        } else if cfg!(target_os = "windows") {
            "link.exe"
        } else {
            "cc"
        };

        let mut cmd = Command::new(linker);

        for path in &obj_paths {
            cmd.arg(path);
        }

        cmd.arg("-o").arg(output_path);

        for arg in link_args {
            cmd.arg(arg);
        }

        let status = cmd.status()
            .map_err(|e| CodegenError::Internal(format!("Linker failed: {}", e)))?;

        // Clean up temp files
        for path in obj_paths {
            let _ = std::fs::remove_file(path);
        }

        if !status.success() {
            return Err(CodegenError::Internal("Linking failed".to_string()));
        }

        Ok(())
    }
}

#[cfg(feature = "cranelift")]
pub use backend::*;

// Fallback when Cranelift is not enabled
#[cfg(not(feature = "cranelift"))]
pub struct CraneliftBackend;

#[cfg(not(feature = "cranelift"))]
impl CraneliftBackend {
    pub fn new(_: Option<&str>, _: crate::OptLevel) -> Result<Self, crate::CodegenError> {
        Err(crate::CodegenError::Unsupported(
            "Cranelift not enabled. Rebuild with --features cranelift".to_string()
        ))
    }
}
