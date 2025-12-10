//! LLVM code generation backend for Roast.
//!
//! Generates LLVM IR and compiles to native code using system LLVM tools.
//! Works with any LLVM version (tested with LLVM 17-21).
//!
//! This backend supports the full Roast language including:
//! - All primitive types (int, float, bool, str)
//! - Collections (list, dict, set, tuple)
//! - Classes and objects with methods
//! - Closures and lambdas
//! - Iterators and for loops
//! - Exception handling (try/except)
//! - Async/await

pub mod codegen;
pub mod ir;
pub mod runtime;
pub mod types;

use std::collections::HashMap;
use roast_common::{Symbol, Interner};
use roast_mir::{MirBody, MirBlock, MirStmt, MirStmtKind, MirTerminator, MirRvalue, MirOperand, MirConstant, MirBinOp, MirUnaryOp, MirPlace, MirAggregateKind, MirProjection};
use roast_typer::Type;

/// Check if a function name is a builtin.
fn is_builtin(name: &str) -> bool {
    matches!(name,
        "print" | "len" | "type" | "int" | "float" | "str" | "bool" |
        "list" | "dict" | "set" | "tuple" | "range" | "abs" | "min" |
        "max" | "sum" | "sorted" | "reversed" | "enumerate" | "zip" |
        "map" | "filter" | "input" | "ord" | "chr" | "repr" | "hash" |
        "id" | "isinstance" | "issubclass" | "hasattr" | "getattr" | "setattr" |
        "all" | "any" | "pow" | "super"
    )
}

/// Check if a function name is super().
fn is_super(name: &str) -> bool {
    name == "super"
}

/// Check if a name is an exception type.
fn is_exception_type(name: &str) -> bool {
    matches!(name,
        "Exception" | "ValueError" | "TypeError" | "IndexError" | "KeyError" |
        "AttributeError" | "RuntimeError" | "NotImplementedError" | "StopIteration" |
        "AssertionError" | "ZeroDivisionError" | "OverflowError" | "NameError" |
        "ImportError" | "ModuleNotFoundError" | "FileNotFoundError" | "IOError" |
        "OSError" | "MemoryError" | "RecursionError" | "BaseException"
    )
}

/// Get exception type code for runtime.
fn get_exception_type_code(name: &str) -> i64 {
    match name {
        "Exception" | "BaseException" => 0,
        "ValueError" => 1,
        "TypeError" => 2,
        "RuntimeError" => 3,
        "IndexError" => 4,
        "KeyError" => 5,
        "AttributeError" => 6,
        "NotImplementedError" => 7,
        "StopIteration" => 8,
        "AssertionError" => 9,
        "ZeroDivisionError" => 10,
        "OverflowError" => 11,
        "NameError" => 12,
        "ImportError" => 13,
        "ModuleNotFoundError" => 14,
        "FileNotFoundError" => 15,
        "IOError" | "OSError" => 16,
        "MemoryError" => 17,
        "RecursionError" => 18,
        _ => 0,
    }
}

/// Check if a method name is a builtin method for a given collection kind.
/// Returns Some(runtime_function_name) if it's a builtin method.
fn get_builtin_method_by_kind(collection_kind: &str, method_name: &str) -> Option<&'static str> {
    match collection_kind {
        "list" => match method_name {
            "append" => Some("roast_list_append"),
            "pop" => Some("roast_list_pop"),
            "len" => Some("roast_list_len"),
            "clear" => Some("roast_list_clear"),
            "copy" => Some("roast_list_copy"),
            "count" => Some("roast_list_count"),
            "extend" => Some("roast_list_extend"),
            "index" => Some("roast_list_index"),
            "insert" => Some("roast_list_insert"),
            "remove" => Some("roast_list_remove"),
            "reverse" => Some("roast_list_reverse"),
            "sort" => Some("roast_list_sort"),
            _ => None,
        },
        "dict" => match method_name {
            "get" => Some("roast_dict_get"),
            "set" => Some("roast_dict_set"),
            "keys" => Some("roast_dict_keys"),
            "values" => Some("roast_dict_values"),
            "items" => Some("roast_dict_items"),
            "clear" => Some("roast_dict_clear"),
            "copy" => Some("roast_dict_copy"),
            "pop" => Some("roast_dict_pop"),
            "update" => Some("roast_dict_update"),
            "setdefault" => Some("roast_dict_setdefault"),
            _ => None,
        },
        "str" => match method_name {
            "upper" => Some("roast_str_upper"),
            "lower" => Some("roast_str_lower"),
            "strip" => Some("roast_str_strip"),
            "lstrip" => Some("roast_str_lstrip"),
            "rstrip" => Some("roast_str_rstrip"),
            "split" => Some("roast_str_split"),
            "join" => Some("roast_str_join"),
            "find" => Some("roast_str_find"),
            "rfind" => Some("roast_str_rfind"),
            "replace" => Some("roast_str_replace"),
            "startswith" => Some("roast_str_startswith"),
            "endswith" => Some("roast_str_endswith"),
            "count" => Some("roast_str_count"),
            "index" => Some("roast_str_find"),
            "len" => Some("roast_str_len"),
            _ => None,
        },
        "set" => match method_name {
            "add" => Some("roast_set_add"),
            "remove" => Some("roast_set_remove"),
            "contains" => Some("roast_set_contains"),
            "clear" => Some("roast_set_clear"),
            "copy" => Some("roast_set_copy"),
            _ => None,
        },
        _ => None,
    }
}

/// Escape a string for LLVM IR string constants.
fn escape_string_ir(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\5C"),
            '\n' => result.push_str("\\0A"),
            '\r' => result.push_str("\\0D"),
            '\t' => result.push_str("\\09"),
            '"' => result.push_str("\\22"),
            '\0' => result.push_str("\\00"),
            c if c.is_ascii() && !c.is_control() => result.push(c),
            c => {
                // Non-ASCII characters - encode as UTF-8 bytes
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("\\{:02X}", b));
                }
            }
        }
    }
    result
}

/// LLVM backend configuration.
#[derive(Clone, Debug)]
pub struct LlvmConfig {
    pub opt_level: u32,
    pub target_triple: Option<String>,
    pub cpu: String,
    pub features: String,
}

impl Default for LlvmConfig {
    fn default() -> Self {
        Self {
            opt_level: 3,
            target_triple: None,
            cpu: "native".to_string(),
            features: String::new(),
        }
    }
}

/// Output type for code generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputType {
    /// LLVM IR (.ll)
    LlvmIr,
    /// LLVM bitcode (.bc)
    Bitcode,
    /// Assembly (.s)
    Assembly,
    /// Object file (.o)
    Object,
    /// Executable
    Executable,
}

/// LLVM code generation error.
#[derive(Debug)]
pub enum LlvmError {
    /// IR generation error
    IrGen(String),
    /// LLVM tool execution error
    Tool(String),
    /// Linking error
    Link(String),
    /// IO error
    Io(std::io::Error),
    /// Codegen error
    CodegenError(String),
}

impl std::fmt::Display for LlvmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlvmError::IrGen(msg) => write!(f, "IR generation error: {}", msg),
            LlvmError::Tool(msg) => write!(f, "LLVM tool error: {}", msg),
            LlvmError::Link(msg) => write!(f, "Linking error: {}", msg),
            LlvmError::Io(e) => write!(f, "IO error: {}", e),
            LlvmError::CodegenError(msg) => write!(f, "Codegen error: {}", msg),
        }
    }
}

impl std::error::Error for LlvmError {}

impl From<std::io::Error> for LlvmError {
    fn from(e: std::io::Error) -> Self {
        LlvmError::Io(e)
    }
}

pub type LlvmResult<T> = Result<T, LlvmError>;

/// Property info for attribute access resolution
#[derive(Clone, Debug)]
pub struct PropertyInfo {
    pub has_setter: bool,
}

/// Class information for method inheritance resolution.
#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    /// Methods defined directly in this class (method symbol ID)
    pub methods: std::collections::HashSet<u32>,
    /// Method names defined in this class (for magic method lookups)
    pub method_names: std::collections::HashSet<String>,
    /// Method Resolution Order - list of class names for inheritance lookup
    pub mro: Vec<String>,
    /// Class variables (name -> (type tag, initial value as i64))
    /// Type tags: 0=int, 1=float, 2=str, 3=bool
    pub class_variables: HashMap<String, (i32, i64)>,
}

/// High-level LLVM backend interface for CLI integration.
pub struct LlvmBackend {
    codegen: LlvmCodeGen,
    /// Map of user-friendly function names to internal names
    func_names: HashMap<String, String>,
    /// Map of symbol IDs to resolved names (for builtins)
    symbol_names: HashMap<u32, String>,
    /// Map of class.property -> PropertyInfo for property access
    properties: HashMap<String, PropertyInfo>,
    /// Set of abstract class names that cannot be instantiated
    abstract_classes: std::collections::HashSet<String>,
    /// Class information for method inheritance
    class_info: HashMap<String, ClassInfo>,
}

impl LlvmBackend {
    /// Create a new LLVM backend with the given optimization level.
    pub fn new(opt_level: u32) -> Self {
        let config = LlvmConfig {
            opt_level,
            ..Default::default()
        };
        Self {
            codegen: LlvmCodeGen::new(config),
            func_names: HashMap::new(),
            symbol_names: HashMap::new(),
            properties: HashMap::new(),
            abstract_classes: std::collections::HashSet::new(),
            class_info: HashMap::new(),
        }
    }

    /// Register class information including MRO for inheritance.
    pub fn register_class_info(&mut self, class_name: &str, mro: Vec<String>) {
        let info = self.class_info.entry(class_name.to_string()).or_default();
        info.mro = mro;
    }

    /// Register that a method is defined in a class (by symbol ID and name).
    pub fn register_class_method(&mut self, class_name: &str, method_sym: u32, method_name: &str) {
        let info = self.class_info.entry(class_name.to_string()).or_default();
        info.methods.insert(method_sym);
        info.method_names.insert(method_name.to_string());
    }

    /// Register a class variable with its initial value.
    pub fn register_class_variable(&mut self, class_name: &str, var_name: &str, type_tag: i32, initial_value: i64) {
        let info = self.class_info.entry(class_name.to_string()).or_default();
        info.class_variables.insert(var_name.to_string(), (type_tag, initial_value));
    }

    /// Check if a class (or any parent in MRO) has a method by name.
    pub fn class_has_method(&self, class_name: &str, method_name: &str) -> bool {
        if let Some(info) = self.class_info.get(class_name) {
            // Check if method is defined in this class
            if info.method_names.contains(method_name) {
                return true;
            }
            // Walk up the MRO to check parents
            for parent in &info.mro {
                if parent == class_name {
                    continue; // Skip self
                }
                if let Some(parent_info) = self.class_info.get(parent) {
                    if parent_info.method_names.contains(method_name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Find which class in the MRO defines a method.
    /// Returns the class name that has the method, walking up the MRO.
    pub fn find_method_class(&self, class_name: &str, method_sym: u32) -> Option<String> {
        if let Some(info) = self.class_info.get(class_name) {
            // Check if method is defined in this class
            if info.methods.contains(&method_sym) {
                return Some(class_name.to_string());
            }
            // Walk up the MRO to find parent with this method
            for parent in &info.mro {
                if parent == class_name {
                    continue; // Skip self
                }
                if let Some(parent_info) = self.class_info.get(parent) {
                    if parent_info.methods.contains(&method_sym) {
                        return Some(parent.clone());
                    }
                }
            }
        }
        None
    }

    /// Register a class as abstract (cannot be instantiated).
    pub fn register_abstract_class(&mut self, class_name: &str) {
        self.abstract_classes.insert(class_name.to_string());
    }

    /// Check if a class is abstract.
    pub fn is_abstract_class(&self, class_name: &str) -> bool {
        self.abstract_classes.contains(class_name)
    }

    /// Register a property for a class.
    pub fn register_property(&mut self, class_name: &str, prop_name: &str, has_setter: bool) {
        let key = format!("{}.{}", class_name, prop_name);
        self.properties.insert(key, PropertyInfo { has_setter });
    }

    /// Check if an attribute is a property.
    pub fn is_property(&self, class_name: &str, prop_name: &str) -> Option<&PropertyInfo> {
        let key = format!("{}.{}", class_name, prop_name);
        self.properties.get(&key)
    }

    /// Register a symbol name (for resolving builtins).
    pub fn register_symbol(&mut self, sym: Symbol, name: &str) {
        self.symbol_names.insert(sym.as_raw(), name.to_string());
        self.codegen.register_symbol(sym.as_raw(), name);
    }

    /// Register all symbols from an interner.
    /// This ensures all attribute names, variable names, etc. can be resolved during codegen.
    pub fn register_all_symbols(&mut self, interner: &Interner) {
        for i in 0..interner.len() {
            let sym = Symbol::from_raw(i as u32);
            if let Some(name) = interner.resolve(sym) {
                self.register_symbol(sym, name);
            }
        }
    }

    /// Compile a MIR function to LLVM IR.
    pub fn compile_function(&mut self, body: &MirBody, name: &str) -> Result<(), String> {
        // Register the function's symbol so calls to it can be resolved
        self.register_symbol(body.name, name);

        self.codegen.compile_function(body, None).map_err(|e| e.to_string())?;

        // Track the function name mapping
        let internal_name = format!("roast_fn_{}", body.name.as_raw());
        self.func_names.insert(name.to_string(), internal_name);

        Ok(())
    }

    /// Compile a class method to LLVM IR with a class-qualified name.
    /// This ensures methods with the same name in different classes don't collide.
    pub fn compile_method(&mut self, body: &MirBody, class_name: &str, method_name: &str) -> Result<(), String> {
        // Register the method with a qualified name like "Dog.speak"
        let qualified_name = format!("{}.{}", class_name, method_name);
        self.register_symbol(body.name, &qualified_name);

        // Compile with class prefix to create unique function names
        self.codegen.compile_function(body, Some(class_name)).map_err(|e| e.to_string())?;

        // Track the function name mapping with qualified name
        let internal_name = format!("roast_fn_{}_{}", class_name, body.name.as_raw());
        self.func_names.insert(qualified_name, internal_name);

        Ok(())
    }

    /// Compile to native executable.
    pub fn finish(&mut self, output_path: &str) -> Result<(), String> {
        // Find the main function by user name
        let main_internal = self.func_names.get("main").cloned();

        if let Some(main_name) = main_internal {
            // Assume main returns void for now in this context
            self.codegen.generate_main(&main_name, true).map_err(|e| e.to_string())?;
        } else {
            return Err("No 'main' function found".to_string());
        }

        self.codegen.compile_to_executable(output_path).map_err(|e| e.to_string())
    }

    /// Generate a class constructor wrapper.
    /// This creates a function that allocates an object and calls __init__.
    pub fn compile_class_constructor(
        &mut self,
        class_sym: Symbol,
        init_sym: Symbol,
        num_params: usize,  // Number of params to __init__ (excluding self)
        class_name: &str,
    ) -> Result<(), String> {
        self.codegen.generate_class_constructor(class_sym.as_raw(), init_sym.as_raw(), num_params, class_name)
            .map_err(|e| e.to_string())
    }

    /// Get the generated LLVM IR (for debugging).
    pub fn get_ir(&self) -> String {
        self.codegen.get_ir()
    }

    /// Pass class info to codegen for method inheritance resolution.
    pub fn sync_class_info(&mut self) {
        self.codegen.class_info = self.class_info.clone();
    }
}

/// Main LLVM code generator.
pub struct LlvmCodeGen {
    config: LlvmConfig,
    /// Generated LLVM IR
    ir: String,
    /// Function declarations
    declarations: Vec<String>,
    /// Function definitions
    definitions: Vec<String>,
    /// Global string constants
    strings: HashMap<String, usize>,
    /// Next string ID
    next_string_id: usize,
    /// Compiled functions (name -> IR)
    pub functions: HashMap<String, String>,
    /// Map of symbol IDs to resolved names (for function calls)
    symbol_names: HashMap<u32, String>,
    /// Class information for method inheritance (synced from LlvmBackend)
    class_info: HashMap<String, ClassInfo>,
    /// Class global variable names (class_name -> global_var_name)
    class_globals: HashMap<String, String>,
    /// Function signatures for forward declarations (name -> (return_type, param_types))
    function_signatures: HashMap<String, (String, Vec<String>)>,
}

impl LlvmCodeGen {
    /// Create a new LLVM code generator.
    pub fn new(config: LlvmConfig) -> Self {
        Self {
            config,
            ir: String::new(),
            declarations: Vec::new(),
            definitions: Vec::new(),
            strings: HashMap::new(),
            next_string_id: 0,
            functions: HashMap::new(),
            symbol_names: HashMap::new(),
            class_info: HashMap::new(),
            class_globals: HashMap::new(),
            function_signatures: HashMap::new(),
        }
    }

    /// Check if a class (or any parent in MRO) has a method by name.
    pub fn class_has_method(&self, class_name: &str, method_name: &str) -> bool {
        if let Some(info) = self.class_info.get(class_name) {
            // Check if method is defined in this class
            if info.method_names.contains(method_name) {
                return true;
            }
            // Walk up the MRO to check parents
            for parent in &info.mro {
                if parent == class_name {
                    continue; // Skip self
                }
                if let Some(parent_info) = self.class_info.get(parent) {
                    if parent_info.method_names.contains(method_name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Find which class in the MRO defines a method.
    pub fn find_method_class(&self, class_name: &str, method_sym: u32) -> Option<String> {
        if let Some(info) = self.class_info.get(class_name) {
            // Check if method is defined in this class
            if info.methods.contains(&method_sym) {
                return Some(class_name.to_string());
            }
            // Walk up the MRO to find parent with this method
            for parent in &info.mro {
                if parent == class_name {
                    continue; // Skip self
                }
                if let Some(parent_info) = self.class_info.get(parent) {
                    if parent_info.methods.contains(&method_sym) {
                        return Some(parent.clone());
                    }
                }
            }
        }
        None
    }

    /// Register a symbol name for resolution.
    pub fn register_symbol(&mut self, sym_id: u32, name: &str) {
        self.symbol_names.insert(sym_id, name.to_string());
    }

    /// Resolve a symbol to its name.
    pub fn resolve_symbol(&self, sym_id: u32) -> Option<&str> {
        self.symbol_names.get(&sym_id).map(|s| s.as_str())
    }

    /// Register a class so that class globals (@.class.X) are generated in IR.
    /// This must be called for each class before get_ir() is called.
    pub fn register_class(&mut self, class_name: &str, mro: Vec<String>) {
        let info = self.class_info.entry(class_name.to_string()).or_default();
        info.mro = mro;
    }

    /// Compile a MIR function to LLVM IR.
    /// If class_prefix is provided, the function name will be prefixed with the class name.
    pub fn compile_function(&mut self, body: &MirBody, class_prefix: Option<&str>) -> LlvmResult<()> {
        // Record function signature for forward declarations
        let func_name = if let Some(prefix) = class_prefix {
            format!("roast_fn_{}_{}", prefix, body.name.as_raw())
        } else {
            format!("roast_fn_{}", body.name.as_raw())
        };
        
        let ret_type = FunctionGen::type_to_llvm(&body.return_ty).to_string();
        let param_types: Vec<String> = body.params.iter()
            .map(|p| FunctionGen::type_to_llvm(&p.local.ty).to_string())
            .collect();
        
        self.function_signatures.insert(func_name.clone(), (ret_type, param_types));
        
        let mut gen = FunctionGen::new(self, body, class_prefix);
        let ir = gen.generate()?;
        self.functions.insert(func_name, ir);
        Ok(())
    }

    /// Generate a class constructor wrapper function.
    /// Creates a function @roast_fn_CLASS that allocates an object and calls __init__.
    /// The class_name is used to generate the prefixed __init__ function name.
    pub fn generate_class_constructor(
        &mut self,
        class_sym_id: u32,
        init_sym_id: u32,
        num_params: usize,
        class_name: &str,
    ) -> LlvmResult<()> {
        let func_name = format!("roast_fn_{}", class_sym_id);
        // Use class-prefixed init name to match how methods are compiled
        let init_name = format!("roast_fn_{}_{}", class_name, init_sym_id);

        // Generate parameter list: (i64 %arg0, i64 %arg1, ...)
        let params: Vec<String> = (0..num_params)
            .map(|i| format!("i64 %arg{}", i))
            .collect();
        let params_str = params.join(", ");

        // Generate argument list for __init__ call: (i64 %self, i64 %arg0, i64 %arg1, ...)
        let init_args: Vec<String> = std::iter::once("i64 %self_i64".to_string())
            .chain((0..num_params).map(|i| format!("i64 %arg{}", i)))
            .collect();
        let init_args_str = init_args.join(", ");

        let constructor_ir = format!(
            r#"
define i64 @{func_name}({params_str}) {{
entry:
    ; Load class pointer from global
    %class_ptr = load i8*, i8** @.class.{class_name}
    ; Create a new object with the class pointer
    %obj = call i8* @roast_object_new(i8* %class_ptr)

    ; Convert object pointer to i64 for storage and passing
    %self_i64 = ptrtoint i8* %obj to i64

    ; Call __init__ with self and all arguments
    call i64 @{init_name}({init_args_str})

    ; Return the object (as i64)
    ret i64 %self_i64
}}
"#,
            func_name = func_name,
            class_name = class_name,
            params_str = params_str,
            init_name = init_name,
            init_args_str = init_args_str,
        );

        self.functions.insert(func_name, constructor_ir);
        Ok(())
    }

    /// Generate a main entry point that calls the Roast main function.
    pub fn generate_main(&mut self, roast_main: &str, return_void: bool) -> LlvmResult<()> {
        // Check if we have classes to initialize
        let has_classes = !self.class_info.is_empty();

        let init_call = if has_classes {
            "    call void @roast_init_classes()\n"
        } else {
            ""
        };

        let main_ir = if return_void {
            format!(
                r#"
define i32 @main(i32 %argc, i8** %argv) {{
entry:
{}    call void @{}()
    ret i32 0
}}
"#,
                init_call, roast_main
            )
        } else {
            format!(
                r#"
define i32 @main(i32 %argc, i8** %argv) {{
entry:
{}    %result = call i64 @{}()
    %exit_code = trunc i64 %result to i32
    ret i32 %exit_code
}}
"#,
                init_call, roast_main
            )
        };
        self.functions.insert("main".to_string(), main_ir);
        Ok(())
    }

    /// Get the complete LLVM IR module.
    pub fn get_ir(&self) -> String {
        let mut ir = String::new();

        // Module header
        ir.push_str("; Roast LLVM IR - Generated by roastc\n");
        ir.push_str("target datalayout = \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128\"\n");

        if let Some(ref triple) = self.config.target_triple {
            ir.push_str(&format!("target triple = \"{}\"\n", triple));
        } else {
            ir.push_str("target triple = \"x86_64-pc-linux-gnu\"\n");
        }
        ir.push('\n');

        // String constants
        for (s, id) in &self.strings {
            let escaped = escape_string_ir(s);
            ir.push_str(&format!(
                "@.str.{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
                id,
                s.len() + 1,
                escaped
            ));
        }
        if !self.strings.is_empty() {
            ir.push('\n');
        }

        // Class name strings (for isinstance)
        for class_name in self.class_info.keys() {
            let escaped = escape_string_ir(class_name);
            ir.push_str(&format!(
                "@.classname.{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
                class_name,
                class_name.len() + 1,
                escaped
            ));
        }
        if !self.class_info.is_empty() {
            ir.push('\n');
        }

        // Class global variables (initialized to null, set during module init)
        for class_name in self.class_info.keys() {
            ir.push_str(&format!(
                "@.class.{} = global i8* null\n",
                class_name
            ));
        }
        if !self.class_info.is_empty() {
            ir.push('\n');
        }

        // Class variable globals (each class variable gets a global with initial value)
        for (class_name, info) in &self.class_info {
            for (var_name, (_type_tag, initial_value)) in &info.class_variables {
                // Class variables are stored as i64 (boxed values)
                ir.push_str(&format!(
                    "@.classvar.{}.{} = global i64 {}\n",
                    class_name, var_name, initial_value
                ));
            }
        }
        if self.class_info.values().any(|i| !i.class_variables.is_empty()) {
            ir.push('\n');
        }

        // External declarations - C standard library
        ir.push_str("; C standard library\n");
        ir.push_str("declare i32 @printf(i8*, ...) nounwind\n");
        ir.push_str("declare i32 @puts(i8*) nounwind\n");
        ir.push_str("declare i8* @malloc(i64) nounwind\n");
        ir.push_str("declare i8* @realloc(i8*, i64) nounwind\n");
        ir.push_str("declare void @free(i8*) nounwind\n");
        ir.push_str("declare i8* @memcpy(i8*, i8*, i64) nounwind\n");
        ir.push_str("declare i32 @memcmp(i8*, i8*, i64) nounwind\n");
        ir.push_str("declare i64 @strlen(i8*) nounwind\n");
        ir.push_str("declare double @pow(double, double) nounwind\n");
        ir.push_str("declare double @sqrt(double) nounwind\n");
        ir.push_str("declare double @floor(double) nounwind\n");
        ir.push_str("declare double @ceil(double) nounwind\n");
        ir.push_str("declare double @sin(double) nounwind\n");
        ir.push_str("declare double @cos(double) nounwind\n");
        ir.push('\n');

        // Roast runtime declarations
        ir.push_str("; Roast runtime library\n");
        ir.push_str("declare void @roast_print_int(i64) nounwind\n");
        ir.push_str("declare void @roast_print_float(double) nounwind\n");
        ir.push_str("declare void @roast_print_str(i8*) nounwind\n");
        ir.push_str("declare void @roast_print_roast_str(i8*) nounwind\n");
        ir.push_str("declare void @roast_print_bool(i1) nounwind\n");
        ir.push_str("declare void @roast_print_newline() nounwind\n");
        ir.push_str("declare void @roast_print_space() nounwind\n");
        ir.push('\n');

        // Memory management
        ir.push_str("; Memory management\n");
        ir.push_str("declare i8* @roast_alloc(i64) nounwind\n");
        ir.push_str("declare void @roast_free(i8*) nounwind\n");
        ir.push_str("declare void @roast_incref(i8*) nounwind\n");
        ir.push_str("declare void @roast_decref(i8*) nounwind\n");
        ir.push('\n');

        // String operations
        ir.push_str("; String operations\n");
        ir.push_str("declare i8* @roast_str_new(i8*, i64) nounwind\n");
        ir.push_str("declare i8* @roast_str_concat(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_str_len(i8*) nounwind\n");
        ir.push_str("declare i1 @roast_str_eq(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_int_to_str(i64) nounwind\n");
        ir.push_str("declare i8* @roast_float_to_str(double) nounwind\n");
        ir.push_str("declare i64 @roast_str_to_int(i8*) nounwind\n");
        ir.push_str("declare double @roast_str_to_float(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_slice(i8*, i64, i64) nounwind\n");
        ir.push_str("declare i8* @roast_str_upper(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_lower(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_strip(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_lstrip(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_rstrip(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_split(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_join(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_str_find(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_str_rfind(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_replace(i8*, i8*, i8*) nounwind\n");
        ir.push_str("declare i1 @roast_str_startswith(i8*, i8*) nounwind\n");
        ir.push_str("declare i1 @roast_str_endswith(i8*, i8*) nounwind\n");
        ir.push_str("declare i1 @roast_str_contains(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_index(i8*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_str_count(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_str_repeat(i8*, i64) nounwind\n");
        ir.push_str("declare i8* @roast_str_format(i8*, i8*) nounwind\n");
        ir.push('\n');

        // List operations
        ir.push_str("; List operations\n");
        ir.push_str("declare i8* @roast_list_new(i64) nounwind\n");
        ir.push_str("declare void @roast_list_append(i8*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_list_get(i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_list_set(i8*, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_list_len(i8*) nounwind\n");
        ir.push_str("declare i64 @roast_list_pop(i8*) nounwind\n");
        ir.push_str("declare i1 @roast_list_contains(i8*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_subscript_get(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_list_count(i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_list_sort(i8*) nounwind\n");
        ir.push_str("declare i64 @roast_list_pop_at(i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_list_insert(i8*, i64, i64) nounwind\n");
        ir.push_str("declare void @roast_list_remove(i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_list_clear(i8*) nounwind\n");
        ir.push_str("declare void @roast_list_reverse(i8*) nounwind\n");
        ir.push_str("declare i64 @roast_list_index(i8*, i64) nounwind\n");
        ir.push_str("declare i8* @roast_list_copy(i8*) nounwind\n");
        ir.push_str("declare void @roast_list_extend(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_list_slice(i8*, i64, i64, i64) nounwind\n");
        ir.push('\n');

        // Dict operations
        ir.push_str("; Dict operations\n");
        ir.push_str("declare i8* @roast_dict_new() nounwind\n");
        ir.push_str("declare void @roast_dict_set(i8*, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_dict_get(i8*, i64) nounwind\n");
        ir.push_str("declare i1 @roast_dict_contains(i8*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_dict_len(i8*) nounwind\n");
        ir.push_str("declare void @roast_dict_delete(i8*, i64) nounwind\n");
        ir.push_str("declare i8* @roast_dict_keys(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_dict_values(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_dict_items(i8*) nounwind\n");
        ir.push_str("declare void @roast_dict_clear(i8*) nounwind\n");
        ir.push_str("declare i64 @roast_dict_get_default(i8*, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_dict_pop(i8*, i64, i64) nounwind\n");
        ir.push_str("declare void @roast_dict_update(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_dict_copy(i8*) nounwind\n");
        ir.push_str("declare i64 @roast_dict_setdefault(i8*, i64, i64) nounwind\n");
        ir.push('\n');

        // Set operations
        ir.push_str("; Set operations\n");
        ir.push_str("declare i8* @roast_set_new() nounwind\n");
        ir.push_str("declare void @roast_set_add(i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_set_remove(i8*, i64) nounwind\n");
        ir.push_str("declare i1 @roast_set_contains(i8*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_set_len(i8*) nounwind\n");
        ir.push('\n');

        // Tuple operations
        ir.push_str("; Tuple operations\n");
        ir.push_str("declare i8* @roast_tuple_new(i64) nounwind\n");
        ir.push_str("declare void @roast_tuple_set(i8*, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_tuple_get(i8*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_tuple_len(i8*) nounwind\n");
        ir.push('\n');

        // Iterator operations
        ir.push_str("; Iterator operations\n");
        ir.push_str("declare i8* @roast_iter_new(i8*) nounwind\n");
        ir.push_str("declare i64 @roast_iter_next(i8*, i1*) nounwind\n");
        ir.push_str("declare i8* @roast_range_new(i64, i64, i64) nounwind\n");
        ir.push('\n');

        // Object/class operations
        ir.push_str("; Object operations\n");
        ir.push_str("declare i8* @roast_object_new(i8*) nounwind\n");
        ir.push_str("declare i8* @roast_class_new(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_object_getattr(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_object_getattr_auto(i8*, i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_object_setattr(i8*, i8*, i64) nounwind\n");
        ir.push_str("declare i1 @roast_object_hasattr(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_object_call_method0(i64, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_object_call_method1(i64, i8*, i64) nounwind\n");
        ir.push('\n');

        // Dunder method operations
        ir.push_str("; Dunder method operations\n");
        ir.push_str("declare i8* @roast_object_str(i64) nounwind\n");
        ir.push_str("declare i8* @roast_object_repr(i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_eq(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_hash(i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_add(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_sub(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_mul(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_lt(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_le(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_gt(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_ge(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_bool(i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_len(i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_getitem(i64, i64) nounwind\n");
        ir.push_str("declare void @roast_object_setitem(i64, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_object_contains(i64, i64) nounwind\n");
        ir.push_str("declare i8* @roast_object_iter(i64) nounwind\n");
        ir.push('\n');

        // Closure operations
        ir.push_str("; Closure operations\n");
        ir.push_str("declare i8* @roast_closure_new(i8*, i64, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_closure_call(i8*, i64*, i64) nounwind\n");
        ir.push('\n');

        // Math operations
        ir.push_str("; Math operations\n");
        ir.push_str("declare i64 @roast_pow_int(i64, i64) nounwind\n");
        ir.push_str("declare double @roast_pow_float(double, double) nounwind\n");
        ir.push_str("declare i64 @roast_abs_int(i64) nounwind\n");
        ir.push_str("declare double @roast_abs_float(double) nounwind\n");
        ir.push('\n');

        // Async operations
        ir.push_str("; Async operations\n");
        ir.push_str("declare i64 @roast_await(i8*) nounwind\n");
        ir.push('\n');

        // Error handling with setjmp/longjmp
        ir.push_str("; Error handling\n");
        ir.push_str("declare i32 @setjmp(i8*) nounwind returns_twice\n");
        ir.push_str("declare void @longjmp(i8*, i32) noreturn nounwind\n");
        ir.push_str("declare i8* @roast_exception_push_frame() nounwind\n");
        ir.push_str("declare void @roast_exception_pop_frame() nounwind\n");
        ir.push_str("declare void @roast_assert(i1, i8*) nounwind\n");
        ir.push_str("declare void @roast_raise(i64)\n"); // Not noreturn - uses longjmp which returns via setjmp
        ir.push_str("declare void @roast_raise_with_message(i64, i64)\n");
        ir.push_str("declare void @roast_reraise()\n");
        ir.push_str("declare i64 @roast_try_begin() nounwind\n");
        ir.push_str("declare void @roast_try_end() nounwind\n");
        ir.push_str("declare i64 @roast_get_exception() nounwind\n");
        ir.push_str("declare i64 @roast_get_exception_type() nounwind\n");
        ir.push_str("declare i64 @roast_exception_matches(i64, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_exception_new(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_exception_get_type(i64) nounwind\n");
        ir.push_str("declare i8* @roast_exception_create(i64, i64) nounwind\n");
        ir.push('\n');

        // super() and varargs support
        ir.push_str("; super() and varargs\n");
        ir.push_str("declare i8* @roast_super_new(i8*, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_super_getattr(i8*, i8*) nounwind\n");
        ir.push_str("declare i8* @roast_pack_varargs(i64*, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_pack_kwargs(i8**, i64*, i64) nounwind\n");
        ir.push_str("declare i64 @roast_args_len(i8*) nounwind\n");
        ir.push('\n');

        // Property support
        ir.push_str("; Property support\n");
        ir.push_str("declare i8* @roast_property_new(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_property_get(i8*, i64) nounwind\n");
        ir.push_str("declare void @roast_property_set(i8*, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_is_property(i64) nounwind\n");
        ir.push('\n');

        // Type constants (external globals from runtime)
        ir.push_str("; Type constants\n");
        ir.push_str("@roast_type_int = external global i64\n");
        ir.push_str("@roast_type_float = external global i64\n");
        ir.push_str("@roast_type_str = external global i64\n");
        ir.push_str("@roast_type_bool = external global i64\n");
        ir.push_str("@roast_type_list = external global i64\n");
        ir.push_str("@roast_type_dict = external global i64\n");
        ir.push_str("@roast_type_none = external global i64\n");
        ir.push('\n');

        // Builtin functions (for direct calls)
        ir.push_str("; Builtin functions\n");
        ir.push_str("declare i64 @roast_print(i64) nounwind\n");
        ir.push_str("declare i64 @roast_len(i64) nounwind\n");
        ir.push_str("declare i64 @roast_type(i64) nounwind\n");
        ir.push_str("declare i64 @roast_int(i64) nounwind\n");
        ir.push_str("declare i64 @roast_float(i64) nounwind\n");
        ir.push_str("declare i64 @roast_str(i64) nounwind\n");
        ir.push_str("declare i64 @roast_bool(i64) nounwind\n");
        ir.push_str("declare i64 @roast_list(i64) nounwind\n");
        ir.push_str("declare i64 @roast_dict(i64) nounwind\n");
        ir.push_str("declare i64 @roast_set(i64) nounwind\n");
        ir.push_str("declare i64 @roast_tuple(i64) nounwind\n");
        ir.push_str("declare i64 @roast_range(i64, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_abs(i64) nounwind\n");
        ir.push_str("declare i64 @roast_min(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_max(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_min_list(i64) nounwind\n");
        ir.push_str("declare i64 @roast_max_list(i64) nounwind\n");
        ir.push_str("declare i64 @roast_sum(i64) nounwind\n");
        ir.push_str("declare i64 @roast_sorted(i64) nounwind\n");
        ir.push_str("declare i64 @roast_reversed(i64) nounwind\n");
        ir.push_str("declare i64 @roast_enumerate(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_zip(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_map(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_filter(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_input(i64) nounwind\n");
        ir.push_str("declare i64 @roast_ord(i64) nounwind\n");
        ir.push_str("declare i64 @roast_chr(i64) nounwind\n");
        ir.push_str("declare i64 @roast_repr(i64) nounwind\n");
        ir.push_str("declare i64 @roast_hash(i64) nounwind\n");
        ir.push_str("declare i64 @roast_id(i64) nounwind\n");
        ir.push_str("declare i64 @roast_isinstance(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_isinstance_by_name(i64, i8*) nounwind\n");
        ir.push_str("declare i64 @roast_issubclass(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_hasattr(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_getattr(i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_setattr(i64, i64, i64) nounwind\n");
        ir.push_str("declare i64 @roast_all(i64) nounwind\n");
        ir.push_str("declare i64 @roast_any(i64) nounwind\n");
        ir.push_str("declare i64 @roast_pow(i64, i64) nounwind\n");
        ir.push('\n');

        // Generate class initialization function
        if !self.class_info.is_empty() {
            ir.push_str("; Class initialization\n");
            ir.push_str("define internal void @roast_init_classes() {\n");
            ir.push_str("entry:\n");

            // Sort classes to create parents before children
            let mut sorted_classes: Vec<&String> = self.class_info.keys().collect();
            sorted_classes.sort_by(|a, b| {
                // Get MRO lengths - classes with shorter MRO should be created first
                let mro_a = self.class_info.get(*a).map(|i| i.mro.len()).unwrap_or(0);
                let mro_b = self.class_info.get(*b).map(|i| i.mro.len()).unwrap_or(0);
                mro_a.cmp(&mro_b)
            });

            for class_name in sorted_classes {
                // Get parent class name from MRO (first non-self entry)
                let parent_global = self.class_info.get(class_name)
                    .and_then(|info| info.mro.iter().skip(1).next())
                    .map(|parent| format!("@.class.{}", parent))
                    .unwrap_or_else(|| "null".to_string());

                let parent_load = if parent_global == "null" {
                    "null".to_string()
                } else {
                    format!("%parent.{}", class_name)
                };

                if parent_global != "null" {
                    ir.push_str(&format!(
                        "  %parent.{} = load i8*, i8** {}\n",
                        class_name, parent_global
                    ));
                }

                ir.push_str(&format!(
                    "  %class.{} = call i8* @roast_class_new(i8* getelementptr ([{} x i8], [{} x i8]* @.classname.{}, i32 0, i32 0), i8* {})\n",
                    class_name,
                    class_name.len() + 1,
                    class_name.len() + 1,
                    class_name,
                    parent_load
                ));
                ir.push_str(&format!(
                    "  store i8* %class.{}, i8** @.class.{}\n",
                    class_name, class_name
                ));
            }

            ir.push_str("  ret void\n");
            ir.push_str("}\n\n");
        }
        // Note: Forward declarations are NOT needed for functions that have definitions in this module.
        // LLVM handles forward references automatically within a module.
        // Only external functions (from runtime library) need declare statements.

        // Function definitions
        for func_ir in self.functions.values() {
            ir.push_str(func_ir);
            ir.push('\n');
        }

        ir
    }

    /// Compile to object file using LLVM tools.
    pub fn compile_to_object(&self, output_path: &str) -> LlvmResult<()> {
        use std::process::Command;
        use std::io::Write;

        let ir = self.get_ir();

        // Write IR to temp file
        let temp_dir = std::env::temp_dir();
        let ir_path = temp_dir.join("roast_module.ll");
        let obj_path = temp_dir.join("roast_module.o");

        std::fs::write(&ir_path, &ir)?;

        // Use clang to compile (it handles LLVM IR)
        let opt_flag = format!("-O{}", self.config.opt_level);
        let status = Command::new("clang")
            .args([
                "-c",
                &opt_flag,
                "-o", obj_path.to_str().unwrap(),
                ir_path.to_str().unwrap(),
            ])
            .status()?;

        if !status.success() {
            return Err(LlvmError::Tool("clang compilation failed".to_string()));
        }

        // Copy object file to output
        std::fs::copy(&obj_path, output_path)?;

        // Cleanup
        let _ = std::fs::remove_file(&ir_path);
        let _ = std::fs::remove_file(&obj_path);

        Ok(())
    }

    /// Compile to executable.
    pub fn compile_to_executable(&self, output_path: &str) -> LlvmResult<()> {
        use std::process::Command;

        let ir = self.get_ir();

        // Write IR to temp file
        let temp_dir = std::env::temp_dir();
        let ir_path = temp_dir.join("roast_module.ll");

        // Also save a copy that won't be deleted for debugging
        let debug_ir_path = temp_dir.join("roast_module_debug.ll");
        std::fs::write(&debug_ir_path, &ir)?;

        std::fs::write(&ir_path, &ir)?;

        // Find the Roast runtime library
        // Search order:
        // 1. ROAST_RUNTIME_LIB env var
        // 2. ../target/release/libroast_runtime.a (relative to executable)
        // 3. /usr/local/lib/libroast_runtime.a
        // 4. Compile without runtime (will fail if runtime functions are used)

        let mut runtime_lib: Option<String> = None;

        // Check environment variable
        if let Ok(lib_path) = std::env::var("ROAST_RUNTIME_LIB") {
            if std::path::Path::new(&lib_path).exists() {
                runtime_lib = Some(lib_path);
            }
        }

        // Check relative to executable
        if runtime_lib.is_none() {
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    // Development: look in same directory as executable (target/debug or target/release)
                    // and in workspace root/target paths
                    for rel_path in [
                        // Same directory as executable
                        "libroast_runtime.a",
                        // Up from target/debug to target/release
                        "../release/libroast_runtime.a",
                        "../debug/libroast_runtime.a",
                        // Up from target/debug to workspace root
                        "../../target/release/libroast_runtime.a",
                        "../../target/debug/libroast_runtime.a",
                        "../../../target/release/libroast_runtime.a",
                        "../../../target/debug/libroast_runtime.a",
                        "../lib/libroast_runtime.a",
                    ] {
                        let candidate = exe_dir.join(rel_path);
                        if candidate.exists() {
                            runtime_lib = Some(candidate.to_string_lossy().to_string());
                            break;
                        }
                    }
                }
            }
        }

        // Check workspace target directory
        if runtime_lib.is_none() {
            // Try to find workspace root by looking for Cargo.toml
            if let Ok(cwd) = std::env::current_dir() {
                let mut dir = cwd.as_path();
                while let Some(parent) = dir.parent() {
                    // Check release first, then debug
                    for build_type in ["release", "debug"] {
                        let candidate = dir.join(format!("target/{}/libroast_runtime.a", build_type));
                        if candidate.exists() {
                            runtime_lib = Some(candidate.to_string_lossy().to_string());
                            break;
                        }
                    }
                    if runtime_lib.is_some() {
                        break;
                    }
                    dir = parent;
                }
            }
        }

        // Check system paths
        if runtime_lib.is_none() {
            for path in ["/usr/local/lib/libroast_runtime.a", "/usr/lib/libroast_runtime.a"] {
                if std::path::Path::new(path).exists() {
                    runtime_lib = Some(path.to_string());
                    break;
                }
            }
        }

        // Use clang to compile and link
        let opt_flag = format!("-O{}", self.config.opt_level);

        let mut args = vec![
            opt_flag.as_str(),
            "-o", output_path,
            ir_path.to_str().unwrap(),
        ];

        // If we found the runtime library, link it
        // We need to add both the library and its dependencies
        let runtime_lib_str: String;
        if let Some(ref lib) = runtime_lib {
            runtime_lib_str = lib.clone();
            args.push(&runtime_lib_str);
            // Link pthread and dl which are needed by Rust's std
            args.push("-lpthread");
            args.push("-ldl");
            args.push("-lm");
        }

        let status = Command::new("clang")
            .args(&args)
            .status()?;

        if !status.success() {
            return Err(LlvmError::Link("clang linking failed".to_string()));
        }

        // Cleanup
        let _ = std::fs::remove_file(&ir_path);

        Ok(())
    }

    /// Add a string constant and return its global name.
    fn add_string(&mut self, s: &str) -> String {
        if let Some(&id) = self.strings.get(s) {
            format!("@.str.{}", id)
        } else {
            let id = self.next_string_id;
            self.next_string_id += 1;
            self.strings.insert(s.to_string(), id);
            format!("@.str.{}", id)
        }
    }
}

/// Function-level code generator.
struct FunctionGen<'a> {
    codegen: &'a mut LlvmCodeGen,
    body: &'a MirBody,
    /// Next SSA value ID
    next_value: usize,
    /// Maps MIR locals to LLVM SSA values
    locals: HashMap<u32, String>,
    /// Generated IR for this function
    ir: String,
    /// Optional class prefix for method names
    class_prefix: Option<String>,
}

impl<'a> FunctionGen<'a> {
    fn new(codegen: &'a mut LlvmCodeGen, body: &'a MirBody, class_prefix: Option<&str>) -> Self {
        Self {
            codegen,
            body,
            next_value: 0,
            locals: HashMap::new(),
            ir: String::new(),
            class_prefix: class_prefix.map(|s| s.to_string()),
        }
    }

    fn fresh_value(&mut self) -> String {
        let v = self.next_value;
        self.next_value += 1;
        format!("%v{}", v)
    }

    fn fresh_label(&mut self) -> usize {
        let l = self.next_value;
        self.next_value += 1;
        l
    }

    /// Check if a type is Copy (doesn't need deallocation)
    fn is_copy_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Int | Type::Float | Type::Bool | Type::NoneType => true,
            Type::Tuple(elems) => elems.iter().all(|e| self.is_copy_type(e)),
            // References are Copy (the reference itself, not what it points to)
            Type::Ref { .. } => true,
            // Everything else needs deallocation
            _ => false,
        }
    }

    /// Get the type of a local variable by its ID.
    fn get_local_type(&self, local_id: u32) -> Option<&Type> {
        self.body.locals.iter()
            .find(|l| l.id == local_id)
            .map(|l| &l.ty)
            .or_else(|| {
                self.body.params.iter()
                    .find(|p| p.local.id == local_id)
                    .map(|p| &p.local.ty)
            })
    }

    /// Check if an operand represents a collection type and return which kind.
    fn get_operand_collection_kind(&self, operand: &MirOperand) -> Option<&'static str> {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                // For locals, look up their type
                let local_ty = self.body.locals.iter()
                    .find(|l| l.id == place.local)
                    .map(|l| &l.ty);
                let param_ty = self.body.params.iter()
                    .find(|p| p.local.id == place.local)
                    .map(|p| &p.local.ty);
                local_ty.or(param_ty).and_then(|ty| {
                    match ty {
                        Type::List(_) => Some("list"),
                        Type::Dict(_, _) => Some("dict"),
                        Type::Str => Some("str"),
                        Type::Set(_) => Some("set"),
                        _ => None,
                    }
                })
            }
            MirOperand::Constant(constant) => {
                // For constants, infer type from the constant kind
                match constant {
                    MirConstant::Str(_) => Some("str"),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Get the class name from an operand if it's a class type.
    /// The class name in MIR may be stored as a symbol ID (string representation).
    fn get_operand_class_name(&self, operand: &MirOperand) -> Option<String> {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                // For locals, look up their type
                let ty = self.body.locals.iter()
                    .find(|l| l.id == place.local)
                    .map(|l| &l.ty)
                    .or_else(|| {
                        self.body.params.iter()
                            .find(|p| p.local.id == place.local)
                            .map(|p| &p.local.ty)
                    });

                if let Some(Type::Class(cls)) = ty {
                    // The class name may be stored as a symbol ID (from MIR)
                    // Try to parse as symbol ID and resolve to actual name
                    if let Ok(sym_id) = cls.name.parse::<u32>() {
                        self.codegen.resolve_symbol(sym_id).map(|s| s.to_string())
                    } else {
                        Some(cls.name.clone())
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Convert an i64 value to an i8* pointer.
    /// This generates a separate inttoptr instruction to avoid LLVM verifier issues.
    fn i64_to_ptr(&mut self, val: &str) -> String {
        let ptr = self.fresh_value();
        self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", ptr, val));
        ptr
    }

    /// Get the signature for a builtin method call.
    /// Returns (return_type, call_arguments_string).
    fn get_builtin_method_signature(&mut self, func_name: &str, arg_vals: &[String]) -> LlvmResult<(&'static str, String)> {
        if arg_vals.is_empty() {
            return Err(LlvmError::IrGen("builtin method requires at least a receiver".to_string()));
        }

        // arg_vals[0] is already a pointer (i8*)
        let receiver_ptr = &arg_vals[0];

        let signature = match func_name {
            // List methods - void returning
            "roast_list_append" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_list_clear" | "roast_list_reverse" | "roast_list_sort" => ("void", format!("i8* {}", receiver_ptr)),
            "roast_list_insert" => ("void", format!("i8* {}, i64 {}, i64 {}", receiver_ptr,
                arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"),
                arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_list_remove" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_list_extend" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("void", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },

            // List methods - i64 returning
            "roast_list_pop" => ("i64", format!("i8* {}", receiver_ptr)),
            "roast_list_len" => ("i64", format!("i8* {}", receiver_ptr)),
            "roast_list_get" | "roast_list_count" | "roast_list_index" =>
                ("i64", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),

            // List methods - i8* returning
            "roast_list_copy" => ("i8*", format!("i8* {}", receiver_ptr)),

            // Dict methods
            "roast_dict_clear" => ("void", format!("i8* {}", receiver_ptr)),
            "roast_dict_get" => ("i64", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_dict_len" => ("i64", format!("i8* {}", receiver_ptr)),

            // Set methods
            "roast_set_add" | "roast_set_remove" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_set_len" => ("i64", format!("i8* {}", receiver_ptr)),
            "roast_set_clear" => ("void", format!("i8* {}", receiver_ptr)),

            // String methods
            "roast_str_len" => ("i64", format!("i8* {}", receiver_ptr)),
            "roast_str_upper" | "roast_str_lower" | "roast_str_strip" | "roast_str_lstrip" | "roast_str_rstrip" =>
                ("i8*", format!("i8* {}", receiver_ptr)),
            "roast_str_split" | "roast_str_join" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i8*", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "roast_str_find" | "roast_str_rfind" | "roast_str_count" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i64", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "roast_str_replace" => {
                let old_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                let new_ptr = self.i64_to_ptr(arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"));
                ("i8*", format!("i8* {}, i8* {}, i8* {}", receiver_ptr, old_ptr, new_ptr))
            },
            "roast_str_startswith" | "roast_str_endswith" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i1", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "roast_str_index" =>
                ("i8*", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_str_repeat" =>
                ("i8*", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "roast_str_format" => {
                let args_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i8*", format!("i8* {}, i8* {}", receiver_ptr, args_ptr))
            },

            // Default - assume i64 return
            _ => {
                let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                ("i64", args_str)
            }
        };

        Ok(signature)
    }

    /// Generate a builtin method call with proper type conversions.
    /// The first argument (receiver) is converted to a pointer for collection methods.
    fn generate_builtin_method_call(&mut self, func_name: &str, arg_vals: &[String], result: &str) -> LlvmResult<()> {
        if arg_vals.is_empty() {
            return Err(LlvmError::IrGen("builtin method requires at least a receiver".to_string()));
        }

        // Convert the receiver (first arg) to a pointer
        let receiver_ptr = self.i64_to_ptr(&arg_vals[0]);

        // Determine the return type and call signature based on the function
        let (ret_type, call_sig) = match func_name {
            // Void-returning methods
            "@roast_list_append" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_list_clear" | "@roast_list_reverse" | "@roast_list_sort" => ("void", format!("i8* {}", receiver_ptr)),
            "@roast_list_insert" => ("void", format!("i8* {}, i64 {}, i64 {}", receiver_ptr,
                arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"),
                arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_list_remove" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_list_extend" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("void", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "@roast_list_set" => ("void", format!("i8* {}, i64 {}, i64 {}", receiver_ptr,
                arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"),
                arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"))),

            // i64-returning methods
            "@roast_list_pop" | "@roast_list_len" => ("i64", format!("i8* {}", receiver_ptr)),
            "@roast_list_get" | "@roast_list_count" | "@roast_list_index" | "@roast_list_pop_at" =>
                ("i64", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),

            // i1-returning methods
            "@roast_list_contains" => ("i1", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),

            // Pointer-returning methods (return i8* which we convert to i64)
            "@roast_list_copy" => ("i8*", format!("i8* {}", receiver_ptr)),

            // Dict methods
            "@roast_dict_clear" => ("void", format!("i8* {}", receiver_ptr)),
            "@roast_dict_set" => ("void", format!("i8* {}, i64 {}, i64 {}", receiver_ptr,
                arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"),
                arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_dict_update" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("void", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "@roast_dict_delete" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_dict_get" | "@roast_dict_len" => ("i64", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_dict_get_default" | "@roast_dict_pop" | "@roast_dict_setdefault" =>
                ("i64", format!("i8* {}, i64 {}, i64 {}", receiver_ptr,
                    arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"),
                    arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_dict_contains" => ("i1", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_dict_keys" | "@roast_dict_values" | "@roast_dict_items" | "@roast_dict_copy" =>
                ("i8*", format!("i8* {}", receiver_ptr)),

            // String methods
            "@roast_str_upper" | "@roast_str_lower" | "@roast_str_strip" | "@roast_str_lstrip" | "@roast_str_rstrip" =>
                ("i8*", format!("i8* {}", receiver_ptr)),
            "@roast_str_len" => ("i64", format!("i8* {}", receiver_ptr)),
            "@roast_str_split" | "@roast_str_join" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i8*", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "@roast_str_find" | "@roast_str_rfind" | "@roast_str_count" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i64", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "@roast_str_replace" => {
                let old_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                let new_ptr = self.i64_to_ptr(arg_vals.get(2).map(|s| s.as_str()).unwrap_or("0"));
                ("i8*", format!("i8* {}, i8* {}, i8* {}", receiver_ptr, old_ptr, new_ptr))
            },
            "@roast_str_startswith" | "@roast_str_endswith" => {
                let other_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i1", format!("i8* {}, i8* {}", receiver_ptr, other_ptr))
            },
            "@roast_str_index" | "@roast_str_repeat" =>
                ("i8*", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_str_format" => {
                let args_ptr = self.i64_to_ptr(arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"));
                ("i8*", format!("i8* {}, i8* {}", receiver_ptr, args_ptr))
            },

            // Set methods
            "@roast_set_add" | "@roast_set_remove" => ("void", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_set_contains" => ("i1", format!("i8* {}, i64 {}", receiver_ptr, arg_vals.get(1).map(|s| s.as_str()).unwrap_or("0"))),
            "@roast_set_len" => ("i64", format!("i8* {}", receiver_ptr)),
            "@roast_set_clear" => ("void", format!("i8* {}", receiver_ptr)),
            "@roast_set_copy" => ("i8*", format!("i8* {}", receiver_ptr)),

            // Default case - just call with i64 args
            _ => {
                let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, args_str));
                return Ok(());
            }
        };

        // Generate the call
        if ret_type == "void" {
            self.ir.push_str(&format!("  call void {}({})\n", func_name, call_sig));
            // Store 0 as the result for void methods
            self.ir.push_str(&format!("  {} = add i64 0, 0\n", result));
        } else if ret_type == "i8*" {
            let ptr_result = self.fresh_value();
            self.ir.push_str(&format!("  {} = call i8* {}({})\n", ptr_result, func_name, call_sig));
            // Convert pointer to i64
            self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, ptr_result));
        } else if ret_type == "i1" {
            let bool_result = self.fresh_value();
            self.ir.push_str(&format!("  {} = call i1 {}({})\n", bool_result, func_name, call_sig));
            // Zero-extend bool to i64
            self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, bool_result));
        } else {
            // i64 return
            self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, call_sig));
        }

        Ok(())
    }

    fn type_to_llvm(ty: &Type) -> &'static str {
        match ty {
            // All values stored as i64 for consistency (bools are zext'd)
            Type::Bool => "i64",
            Type::Int | Type::Int64 => "i64",
            Type::Int8 => "i8",
            Type::Int16 => "i16",
            Type::Int32 => "i32",
            Type::Float | Type::Float64 => "double",
            Type::Float32 => "float",
            // NoneType: use i64 for variables (0 = None), void only for function returns
            Type::NoneType => "i64",
            // Strings and all complex types are represented as i64 (tagged pointers)
            Type::Str => "i64",
            _ => "i64", // Default to i64 for complex types
        }
    }

    fn generate(&mut self) -> LlvmResult<String> {
        // Generate function name with optional class prefix
        let func_name = if let Some(ref prefix) = self.class_prefix {
            format!("roast_fn_{}_{}", prefix, self.body.name.as_raw())
        } else {
            format!("roast_fn_{}", self.body.name.as_raw())
        };
        let ret_type = Self::type_to_llvm(&self.body.return_ty);

        // Build parameter list - use temporary names, we'll copy them to allocas
        let mut param_names: Vec<(String, String, u32)> = Vec::new(); // (arg_name, type, local_id)
        let params: Vec<String> = self.body.params.iter().enumerate().map(|(i, p)| {
            let ty = Self::type_to_llvm(&p.local.ty);
            let name = format!("%arg{}", i);
            param_names.push((name.clone(), ty.to_string(), p.local.id));
            format!("{} {}", ty, name)
        }).collect();

        // Function header
        self.ir.push_str(&format!(
            "define {} @{}({}) {{\n",
            ret_type,
            func_name,
            params.join(", ")
        ));

        // Entry block starts immediately
        self.ir.push_str("entry:\n");

        // Allocate space for all locals (including parameters)
        // Skip void types - they can't be allocated
        for local in &self.body.locals {
            let ty = Self::type_to_llvm(&local.ty);
            // Skip void types - they can't be stored on stack
            if ty == "void" {
                continue;
            }
            let ptr = self.fresh_value();
            self.ir.push_str(&format!("  {} = alloca {}\n", ptr, ty));
            self.locals.insert(local.id, ptr);
        }

        // Also allocate for parameters (they need their own stack slots)
        for (arg_name, ty, id) in &param_names {
            // Skip void types
            if ty == "void" {
                continue;
            }
            if !self.locals.contains_key(id) {
                let ptr = self.fresh_value();
                self.ir.push_str(&format!("  {} = alloca {}\n", ptr, ty));
                self.locals.insert(*id, ptr.clone());
            }
            // Store parameter into the allocated slot
            let ptr = self.locals.get(id).cloned().unwrap();
            self.ir.push_str(&format!("  store {} {}, {}* {}\n", ty, arg_name, ty, ptr));
        }

        // Now emit the first basic block's statements (skip bb0's label)
        if let Some(first_block) = self.body.blocks.first() {
            self.generate_block_body(first_block)?;
        }

        // Generate remaining blocks
        for block in self.body.blocks.iter().skip(1) {
            self.ir.push_str(&format!("bb{}:\n", block.id));
            self.generate_block_body(block)?;
        }

        self.ir.push_str("}\n");
        Ok(self.ir.clone())
    }

    fn generate_block_body(&mut self, block: &MirBlock) -> LlvmResult<()> {
        for stmt in &block.stmts {
            self.generate_stmt(stmt)?;
        }
        self.generate_terminator(&block.terminator)?;
        Ok(())
    }

    fn generate_block(&mut self, block: &MirBlock) -> LlvmResult<()> {
        self.generate_block_body(block)
    }

    fn generate_stmt(&mut self, stmt: &MirStmt) -> LlvmResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { place, value } => {
                // Handle projections (field access, indexing)
                if !place.projections.is_empty() {
                    self.generate_assign_projection(place, value)?;
                } else {
                    let val = self.generate_rvalue(value)?;
                    let ptr = self.get_place_ptr(place);
                    let ty = Self::type_to_llvm(&self.get_place_type(place));
                    self.ir.push_str(&format!("  store {} {}, {}* {}\n", ty, val, ty, ptr));
                }
            }
            MirStmtKind::StorageLive(_) => {
                // Storage live - no action needed (allocation happens at declaration)
            }
            MirStmtKind::StorageDead(local_id) => {
                // Automatic memory deallocation for owned values
                // This implements Rust-like RAII without garbage collection
                if let Some(local_ty) = self.get_local_type(*local_id) {
                    let local_ty = local_ty.clone();
                    if !self.is_copy_type(&local_ty) {
                        // For non-Copy types (lists, objects, strings), decrement refcount
                        // The runtime handles null pointers gracefully
                        let ptr = self.locals.get(local_id).cloned()
                            .unwrap_or_else(|| format!("%v{}", local_id));
                        let val = self.fresh_value();
                        self.ir.push_str(&format!("  {} = load i64, i64* {}\n", val, ptr));
                        // Call decref - runtime checks for null
                        self.ir.push_str(&format!("  call void @roast_decref(i8* inttoptr (i64 {} to i8*))\n", val));
                    }
                }
            }
            MirStmtKind::ListAppend { list, value } => {
                // Get the pointer to the list storage location
                let list_storage = self.get_place_ptr(list);
                // Load the list pointer value (i64)
                let list_val = self.fresh_value();
                self.ir.push_str(&format!("  {} = load i64, i64* {}\n", list_val, list_storage));
                // Convert to i8* for the function call
                let list_i8ptr = self.i64_to_ptr(&list_val);
                let val = self.generate_operand(value)?;
                self.ir.push_str(&format!(
                    "  call void @roast_list_append(i8* {}, i64 {})\n",
                    list_i8ptr, val
                ));
            }
            MirStmtKind::SetAttr { object, attr, value } => {
                // Resolve attribute name through interner
                let attr_name = self.codegen.resolve_symbol(attr.as_raw())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("attr_{}", attr.as_raw()));

                // Check if this is a class variable assignment (ClassName.attr = value)
                if let MirOperand::Global(class_sym) = object {
                    if let Some(class_name) = self.codegen.resolve_symbol(class_sym.as_raw()).map(|s| s.to_string()) {
                        // Check if this class has this class variable
                        if let Some(info) = self.codegen.class_info.get(&class_name) {
                            if info.class_variables.contains_key(&attr_name) {
                                // Store to the class variable global
                                let val = self.generate_operand(value)?;
                                self.ir.push_str(&format!(
                                    "  store i64 {}, i64* @.classvar.{}.{}\n",
                                    val, class_name, attr_name
                                ));
                                return Ok(());
                            }
                        }
                    }
                }

                let obj = self.generate_operand(object)?;
                let obj_ptr = self.i64_to_ptr(&obj);
                let val = self.generate_operand(value)?;
                let attr_global = self.codegen.add_string(&attr_name);

                let attr_ptr = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                    attr_ptr, attr_name.len() + 1, attr_name.len() + 1, attr_global
                ));

                self.ir.push_str(&format!(
                    "  call void @roast_object_setattr(i8* {}, i8* {}, i64 {})\n",
                    obj_ptr, attr_ptr, val
                ));
            }
            MirStmtKind::SetAttrIndex { object, attr, index, value } => {
                // Handle obj.attr[idx] = value
                // This gets the attribute (a list/dict), then sets the index on it
                let attr_name = self.codegen.resolve_symbol(attr.as_raw())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("attr_{}", attr.as_raw()));

                let obj = self.generate_operand(object)?;
                let obj_ptr = self.i64_to_ptr(&obj);
                let idx = self.generate_operand(index)?;
                let val = self.generate_operand(value)?;

                // Get the attribute string pointer
                let attr_global = self.codegen.add_string(&attr_name);
                let attr_ptr = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                    attr_ptr, attr_name.len() + 1, attr_name.len() + 1, attr_global
                ));

                // Get the attribute value (the list/dict)
                let list_val = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = call i64 @roast_object_getattr(i8* {}, i8* {})\n",
                    list_val, obj_ptr, attr_ptr
                ));

                // Set the index on the list
                let list_ptr = self.i64_to_ptr(&list_val);
                self.ir.push_str(&format!(
                    "  call void @roast_list_set(i8* {}, i64 {}, i64 {})\n",
                    list_ptr, idx, val
                ));
            }
            MirStmtKind::TryEnd => {
                // Pop the exception frame after try body or handler completes successfully
                self.ir.push_str("  call void @roast_exception_pop_frame()\n");
            }
            MirStmtKind::Nop => {}
        }
        Ok(())
    }

    /// Handle assignment with projections (field access, indexing).
    fn generate_assign_projection(&mut self, place: &MirPlace, value: &MirRvalue) -> LlvmResult<()> {
        let val = self.generate_rvalue(value)?;
        let base_ptr = self.get_place_ptr(&MirPlace::local(place.local));
        let base_ty = self.get_place_type(&MirPlace::local(place.local));

        // Load the base value
        let base = self.fresh_value();
        let llvm_ty = Self::type_to_llvm(&base_ty);
        self.ir.push_str(&format!("  {} = load {}, {}* {}\n", base, llvm_ty, llvm_ty, base_ptr));

        // Apply projections
        for proj in &place.projections {
            match proj {
                MirProjection::Field(idx) => {
                    // Object field access
                    let field_name = format!("{}", idx);
                    let field_global = self.codegen.add_string(&field_name);

                    let field_ptr = self.fresh_value();
                    self.ir.push_str(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                        field_ptr, field_name.len() + 1, field_name.len() + 1, field_global
                    ));

                    let base_ptr = self.i64_to_ptr(&base);
                    self.ir.push_str(&format!(
                        "  call void @roast_object_setattr(i8* {}, i8* {}, i64 {})\n",
                        base_ptr, field_ptr, val
                    ));
                }
                MirProjection::Index(idx_local) => {
                    // List/dict index assignment
                    let idx_ptr = self.locals.get(idx_local).cloned().unwrap_or_else(|| format!("%local{}", idx_local));
                    let idx = self.fresh_value();
                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", idx, idx_ptr));

                    let base_ptr = self.i64_to_ptr(&base);
                    match &base_ty {
                        Type::List(_) => {
                            self.ir.push_str(&format!(
                                "  call void @roast_list_set(i8* {}, i64 {}, i64 {})\n",
                                base_ptr, idx, val
                            ));
                        }
                        Type::Dict(_, _) => {
                            self.ir.push_str(&format!(
                                "  call void @roast_dict_set(i8* {}, i64 {}, i64 {})\n",
                                base_ptr, idx, val
                            ));
                        }
                        Type::Class(cls) => {
                            // Check if class has __setitem__ method
                            let class_name = &cls.name;
                            if self.codegen.class_has_method(class_name, "__setitem__") {
                                // Find the class that defines __setitem__ (could be parent)
                                let setitem_class = self.codegen.class_info.get(class_name)
                                    .and_then(|info| {
                                        if info.method_names.contains("__setitem__") {
                                            Some(class_name.clone())
                                        } else {
                                            info.mro.iter()
                                                .find(|p| {
                                                    self.codegen.class_info.get(*p)
                                                        .map(|pi| pi.method_names.contains("__setitem__"))
                                                        .unwrap_or(false)
                                                })
                                                .cloned()
                                        }
                                    })
                                    .unwrap_or(class_name.clone());

                                // Find the __setitem__ symbol ID
                                let setitem_sym_id = self.codegen.class_info.get(&setitem_class)
                                    .and_then(|info| {
                                        info.methods.iter()
                                            .find(|&&id| {
                                                self.codegen.resolve_symbol(id)
                                                    .map(|name| name == "__setitem__" || name.ends_with(".__setitem__"))
                                                    .unwrap_or(false)
                                            })
                                            .copied()
                                    })
                                    .unwrap_or(0);

                                // Call __setitem__(self, key, value) - returns void but we use i64 anyway
                                let func = format!("@roast_fn_{}_{}", setitem_class, setitem_sym_id);
                                let _result = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = call i64 {}(i64 {}, i64 {}, i64 {})\n",
                                    _result, func, base, idx, val
                                ));
                            } else {
                                // Fall back to list set (may fail at runtime)
                                self.ir.push_str(&format!(
                                    "  call void @roast_list_set(i8* {}, i64 {}, i64 {})\n",
                                    base_ptr, idx, val
                                ));
                            }
                        }
                        _ => {
                            // Assume list-like
                            self.ir.push_str(&format!(
                                "  call void @roast_list_set(i8* {}, i64 {}, i64 {})\n",
                                base_ptr, idx, val
                            ));
                        }
                    }
                }
                MirProjection::Slice { .. } => {
                    // Slice assignment not currently supported
                    // Would need to implement list[1:3] = [x, y] style assignment
                    return Err(LlvmError::CodegenError("Slice assignment not yet supported".to_string()));
                }
                MirProjection::Deref => {
                    // Dereference pointer - store to pointed location
                    let ptr = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i64*\n", ptr, base));
                    self.ir.push_str(&format!("  store i64 {}, i64* {}\n", val, ptr));
                }
            }
        }

        Ok(())
    }

    fn generate_rvalue(&mut self, rvalue: &MirRvalue) -> LlvmResult<String> {
        match rvalue {
            MirRvalue::Use(operand) => self.generate_operand(operand),

            MirRvalue::BinaryOp(op, lhs, rhs) => {
                self.generate_binary_op(op, lhs, rhs)
            }

            MirRvalue::UnaryOp(op, operand) => {
                self.generate_unary_op(op, operand)
            }

            MirRvalue::Aggregate(kind, operands) => {
                self.generate_aggregate(kind, operands)
            }

            MirRvalue::Ref(place, _mutable) => {
                // Create a reference to the place
                // The place pointer needs to be converted to i64 for storage
                let ptr = self.get_place_ptr(place);
                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", result, ptr));
                Ok(result)
            }

            MirRvalue::Len(place) => {
                // Get length of a collection
                let ptr = self.get_place_ptr(place);
                let ty = self.get_place_type(place);
                self.generate_len(&ptr, &ty)
            }

            MirRvalue::Cast(operand, target_ty) => {
                self.generate_cast(operand, target_ty)
            }

            MirRvalue::Attr(operand, attr) => {
                // Get attribute from object
                self.generate_attr_get(operand, attr)
            }

            MirRvalue::Await(operand) => {
                // Await an async value
                let val = self.generate_operand(operand)?;
                let val_ptr = self.i64_to_ptr(&val);
                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = call i64 @roast_await(i8* {})\n", result, val_ptr));
                Ok(result)
            }
        }
    }

    /// Generate binary operation.
    fn generate_binary_op(&mut self, op: &MirBinOp, lhs: &MirOperand, rhs: &MirOperand) -> LlvmResult<String> {
        // For In/NotIn operators, check if the container (rhs) has __contains__
        if matches!(op, MirBinOp::In | MirBinOp::NotIn) {
            if let Some(class_name) = self.get_operand_class_name(rhs) {
                if self.codegen.class_has_method(&class_name, "__contains__") {
                    return self.generate_class_contains_op(op, lhs, rhs, &class_name);
                }
            }
        }

        // Check for class type with magic methods for operators
        if let Some(class_name) = self.get_operand_class_name(lhs) {
            // Map operators to magic method names
            let magic_method = match op {
                MirBinOp::Eq => Some("__eq__"),
                MirBinOp::Ne => Some("__ne__"),
                MirBinOp::Lt => Some("__lt__"),
                MirBinOp::Le => Some("__le__"),
                MirBinOp::Gt => Some("__gt__"),
                MirBinOp::Ge => Some("__ge__"),
                MirBinOp::Add => Some("__add__"),
                MirBinOp::Sub => Some("__sub__"),
                MirBinOp::Mul => Some("__mul__"),
                MirBinOp::Div => Some("__truediv__"),
                MirBinOp::FloorDiv => Some("__floordiv__"),
                MirBinOp::Rem => Some("__mod__"),
                MirBinOp::Pow => Some("__pow__"),
                MirBinOp::BitAnd => Some("__and__"),
                MirBinOp::BitOr => Some("__or__"),
                MirBinOp::BitXor => Some("__xor__"),
                MirBinOp::Shl => Some("__lshift__"),
                MirBinOp::Shr => Some("__rshift__"),
                _ => None,
            };

            if let Some(method_name) = magic_method {
                if self.codegen.class_has_method(&class_name, method_name) {
                    return self.generate_class_binary_op(op, lhs, rhs, &class_name, method_name);
                }
                // For != without __ne__, try __eq__ and negate
                if matches!(op, MirBinOp::Ne) && self.codegen.class_has_method(&class_name, "__eq__") {
                    return self.generate_class_eq_op(op, lhs, rhs, &class_name);
                }
            }
        }

        let l = self.generate_operand(lhs)?;
        let r = self.generate_operand(rhs)?;
        let result = self.fresh_value();

        // Determine types for proper instruction selection
        let lhs_ty = self.operand_type(lhs);
        let rhs_ty = self.operand_type(rhs);
        let is_float = matches!(lhs_ty, Type::Float | Type::Float32 | Type::Float64) 
                    || matches!(rhs_ty, Type::Float | Type::Float32 | Type::Float64);
        // Check either side for string type - handles method call results where LHS type might be Unknown
        let is_string = matches!(lhs_ty, Type::Str) || matches!(rhs_ty, Type::Str);

        // String operations
        if is_string {
            match op {
                MirBinOp::Add => {
                    // String concatenation
                    let l_ptr = self.fresh_value();
                    let r_ptr = self.fresh_value();
                    let concat_result = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", l_ptr, l));
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", r_ptr, r));
                    self.ir.push_str(&format!("  {} = call i8* @roast_str_concat(i8* {}, i8* {})\n", concat_result, l_ptr, r_ptr));
                    self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, concat_result));
                    return Ok(result);
                }
                MirBinOp::Mul => {
                    // String repetition (str * n)
                    let l_ptr = self.fresh_value();
                    let repeat_result = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", l_ptr, l));
                    self.ir.push_str(&format!("  {} = call i8* @roast_str_repeat(i8* {}, i64 {})\n", repeat_result, l_ptr, r));
                    self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, repeat_result));
                    return Ok(result);
                }
                MirBinOp::Eq => {
                    // String equality
                    let l_ptr = self.fresh_value();
                    let r_ptr = self.fresh_value();
                    let bool_result = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", l_ptr, l));
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", r_ptr, r));
                    self.ir.push_str(&format!("  {} = call i1 @roast_str_eq(i8* {}, i8* {})\n", bool_result, l_ptr, r_ptr));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, bool_result));
                    return Ok(result);
                }
                MirBinOp::Ne => {
                    // String inequality
                    let l_ptr = self.fresh_value();
                    let r_ptr = self.fresh_value();
                    let tmp = self.fresh_value();
                    let not_result = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", l_ptr, l));
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", r_ptr, r));
                    self.ir.push_str(&format!("  {} = call i1 @roast_str_eq(i8* {}, i8* {})\n", tmp, l_ptr, r_ptr));
                    self.ir.push_str(&format!("  {} = xor i1 {}, true\n", not_result, tmp));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, not_result));
                    return Ok(result);
                }
                _ => {
                    // Unsupported string operation, fall through to integer
                }
            }
        }

        if is_float {
            // Floating point operations
            // Convert integer operands to double if needed
            let lhs_is_float = matches!(lhs_ty, Type::Float | Type::Float32 | Type::Float64);
            let rhs_is_float = matches!(rhs_ty, Type::Float | Type::Float32 | Type::Float64);
            
            let l_float = if !lhs_is_float {
                // Left operand is integer, convert to double
                let converted = self.fresh_value();
                self.ir.push_str(&format!("  {} = sitofp i64 {} to double\n", converted, l));
                converted
            } else {
                l.clone()
            };
            
            let r_float = if !rhs_is_float {
                // Right operand is integer, convert to double
                let converted = self.fresh_value();
                self.ir.push_str(&format!("  {} = sitofp i64 {} to double\n", converted, r));
                converted
            } else {
                r.clone()
            };

            let instr = match op {
                MirBinOp::Add => "fadd",
                MirBinOp::Sub => "fsub",
                MirBinOp::Mul => "fmul",
                MirBinOp::Div => "fdiv",
                MirBinOp::Rem => "frem",
                MirBinOp::Pow => {
                    // Call power function
                    self.ir.push_str(&format!("  {} = call double @roast_pow_float(double {}, double {})\n", result, l_float, r_float));
                    return Ok(result);
                }
                MirBinOp::Lt => {
                    let cmp = self.fresh_value();
                    self.ir.push_str(&format!("  {} = fcmp olt double {}, {}\n", cmp, l_float, r_float));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                MirBinOp::Le => {
                    let cmp = self.fresh_value();
                    self.ir.push_str(&format!("  {} = fcmp ole double {}, {}\n", cmp, l_float, r_float));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                MirBinOp::Gt => {
                    let cmp = self.fresh_value();
                    self.ir.push_str(&format!("  {} = fcmp ogt double {}, {}\n", cmp, l_float, r_float));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                MirBinOp::Ge => {
                    let cmp = self.fresh_value();
                    self.ir.push_str(&format!("  {} = fcmp oge double {}, {}\n", cmp, l_float, r_float));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                MirBinOp::Eq => {
                    let cmp = self.fresh_value();
                    self.ir.push_str(&format!("  {} = fcmp oeq double {}, {}\n", cmp, l_float, r_float));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                MirBinOp::Ne => {
                    let cmp = self.fresh_value();
                    self.ir.push_str(&format!("  {} = fcmp one double {}, {}\n", cmp, l_float, r_float));
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                _ => "fadd", // Default
            };
            // Float arithmetic produces double, but we store as i64 - need bitcast
            let float_result = self.fresh_value();
            self.ir.push_str(&format!("  {} = {} double {}, {}\n", float_result, instr, l_float, r_float));
            self.ir.push_str(&format!("  {} = bitcast double {} to i64\n", result, float_result));
        } else {
            // Integer operations
            let (instr, needs_cmp) = match op {
                MirBinOp::Add => ("add", false),
                MirBinOp::Sub => ("sub", false),
                MirBinOp::Mul => ("mul", false),
                MirBinOp::Div => ("sdiv", false),
                MirBinOp::FloorDiv => ("sdiv", false),
                MirBinOp::Rem => ("srem", false),
                MirBinOp::Pow => {
                    self.ir.push_str(&format!("  {} = call i64 @roast_pow_int(i64 {}, i64 {})\n", result, l, r));
                    return Ok(result);
                }
                MirBinOp::BitAnd => ("and", false),
                MirBinOp::BitOr => ("or", false),
                MirBinOp::BitXor => ("xor", false),
                MirBinOp::Shl => ("shl", false),
                MirBinOp::Shr => ("ashr", false),
                MirBinOp::Lt => ("icmp slt", true),
                MirBinOp::Le => ("icmp sle", true),
                MirBinOp::Gt => ("icmp sgt", true),
                MirBinOp::Ge => ("icmp sge", true),
                MirBinOp::Eq => ("icmp eq", true),
                MirBinOp::Ne => ("icmp ne", true),
                MirBinOp::In => {
                    // Check if value is in collection
                    // First check the type of the container (rhs)
                    let rhs_ty = self.operand_type(rhs);
                    let ptr = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", ptr, r));
                    let cmp = self.fresh_value();
                    
                    match rhs_ty {
                        Type::Str => {
                            // String contains - lhs is also a string
                            let l_ptr = self.fresh_value();
                            self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", l_ptr, l));
                            self.ir.push_str(&format!("  {} = call i1 @roast_str_contains(i8* {}, i8* {})\n", cmp, ptr, l_ptr));
                        }
                        Type::Dict(_, _) => {
                            self.ir.push_str(&format!("  {} = call i1 @roast_dict_contains(i8* {}, i64 {})\n", cmp, ptr, l));
                        }
                        Type::Set(_) => {
                            self.ir.push_str(&format!("  {} = call i1 @roast_set_contains(i8* {}, i64 {})\n", cmp, ptr, l));
                        }
                        _ => {
                            // Default to list contains
                            self.ir.push_str(&format!("  {} = call i1 @roast_list_contains(i8* {}, i64 {})\n", cmp, ptr, l));
                        }
                    }
                    // Extend i1 to i64
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
                    return Ok(result);
                }
                MirBinOp::NotIn => {
                    // Check the type of the container (rhs)
                    let rhs_ty = self.operand_type(rhs);
                    let ptr = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", ptr, r));
                    let tmp = self.fresh_value();
                    
                    match rhs_ty {
                        Type::Str => {
                            // String contains - lhs is also a string
                            let l_ptr = self.fresh_value();
                            self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", l_ptr, l));
                            self.ir.push_str(&format!("  {} = call i1 @roast_str_contains(i8* {}, i8* {})\n", tmp, ptr, l_ptr));
                        }
                        Type::Dict(_, _) => {
                            self.ir.push_str(&format!("  {} = call i1 @roast_dict_contains(i8* {}, i64 {})\n", tmp, ptr, l));
                        }
                        Type::Set(_) => {
                            self.ir.push_str(&format!("  {} = call i1 @roast_set_contains(i8* {}, i64 {})\n", tmp, ptr, l));
                        }
                        _ => {
                            // Default to list contains
                            self.ir.push_str(&format!("  {} = call i1 @roast_list_contains(i8* {}, i64 {})\n", tmp, ptr, l));
                        }
                    }
                    let neg = self.fresh_value();
                    self.ir.push_str(&format!("  {} = xor i1 {}, true\n", neg, tmp));
                    // Extend i1 to i64 for consistent storage
                    self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, neg));
                    return Ok(result);
                }
            };

            if needs_cmp {
                // Comparison ops return i1, need to extend to i64
                let cmp_result = self.fresh_value();
                self.ir.push_str(&format!("  {} = {} i64 {}, {}\n", cmp_result, instr, l, r));
                self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp_result));
            } else {
                self.ir.push_str(&format!("  {} = {} i64 {}, {}\n", result, instr, l, r));
            }
        }

        Ok(result)
    }

    /// Generate equality comparison for class objects using __eq__ method.
    fn generate_class_eq_op(&mut self, op: &MirBinOp, lhs: &MirOperand, rhs: &MirOperand, class_name: &str) -> LlvmResult<String> {
        let l = self.generate_operand_as_i64(lhs)?;
        let r = self.generate_operand_as_i64(rhs)?;

        // Look up which class defines __eq__ (could be parent)
        let eq_class = self.codegen.class_info.get(class_name)
            .and_then(|info| {
                if info.method_names.contains("__eq__") {
                    Some(class_name.to_string())
                } else {
                    info.mro.iter()
                        .find(|p| {
                            self.codegen.class_info.get(*p)
                                .map(|pi| pi.method_names.contains("__eq__"))
                                .unwrap_or(false)
                        })
                        .cloned()
                }
            })
            .unwrap_or_else(|| class_name.to_string());

        // Find the __eq__ symbol ID
        let eq_sym_id = self.codegen.class_info.get(&eq_class)
            .and_then(|info| {
                info.methods.iter()
                    .find(|&&id| {
                        self.codegen.resolve_symbol(id)
                            .map(|name| name == "__eq__" || name.ends_with(".__eq__"))
                            .unwrap_or(false)
                    })
                    .copied()
            })
            .unwrap_or(0);

        // Call __eq__ method: self.__eq__(other)
        let eq_result = self.fresh_value();
        let func = format!("@roast_fn_{}_{}", eq_class, eq_sym_id);
        self.ir.push_str(&format!("  {} = call i64 {}(i64 {}, i64 {})\n", eq_result, func, l, r));

        // For != operator, negate the result
        if matches!(op, MirBinOp::Ne) {
            let neg_result = self.fresh_value();
            // Convert i64 to i1 (0 = false, non-0 = true)
            let bool_val = self.fresh_value();
            self.ir.push_str(&format!("  {} = icmp eq i64 {}, 0\n", bool_val, eq_result));
            // Extend back to i64
            self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", neg_result, bool_val));
            Ok(neg_result)
        } else {
            Ok(eq_result)
        }
    }

    /// Generate binary operation using class magic method (generic version).
    fn generate_class_binary_op(&mut self, _op: &MirBinOp, lhs: &MirOperand, rhs: &MirOperand, class_name: &str, method_name: &str) -> LlvmResult<String> {
        let l = self.generate_operand_as_i64(lhs)?;
        let r = self.generate_operand_as_i64(rhs)?;

        // Look up which class defines this method (could be parent)
        let method_class = self.codegen.class_info.get(class_name)
            .and_then(|info| {
                if info.method_names.contains(method_name) {
                    Some(class_name.to_string())
                } else {
                    info.mro.iter()
                        .find(|p| {
                            self.codegen.class_info.get(*p)
                                .map(|pi| pi.method_names.contains(method_name))
                                .unwrap_or(false)
                        })
                        .cloned()
                }
            })
            .unwrap_or_else(|| class_name.to_string());

        // Find the method symbol ID
        let sym_id = self.codegen.class_info.get(&method_class)
            .and_then(|info| {
                info.methods.iter()
                    .find(|&&id| {
                        self.codegen.resolve_symbol(id)
                            .map(|name| name == method_name || name.ends_with(&format!(".{}", method_name)))
                            .unwrap_or(false)
                    })
                    .copied()
            })
            .unwrap_or(0);

        // Call the magic method: self.__method__(other)
        let method_result = self.fresh_value();
        let func = format!("@roast_fn_{}_{}", method_class, sym_id);
        self.ir.push_str(&format!("  {} = call i64 {}(i64 {}, i64 {})\n", method_result, func, l, r));

        Ok(method_result)
    }

    /// Generate 'in' / 'not in' operation using __contains__ magic method.
    /// For `item in container`, calls `container.__contains__(item)`.
    fn generate_class_contains_op(&mut self, op: &MirBinOp, item: &MirOperand, container: &MirOperand, class_name: &str) -> LlvmResult<String> {
        let item_val = self.generate_operand_as_i64(item)?;
        let container_val = self.generate_operand_as_i64(container)?;

        // Look up which class defines __contains__ (could be parent)
        let contains_class = self.codegen.class_info.get(class_name)
            .and_then(|info| {
                if info.method_names.contains("__contains__") {
                    Some(class_name.to_string())
                } else {
                    info.mro.iter()
                        .find(|p| {
                            self.codegen.class_info.get(*p)
                                .map(|pi| pi.method_names.contains("__contains__"))
                                .unwrap_or(false)
                        })
                        .cloned()
                }
            })
            .unwrap_or_else(|| class_name.to_string());

        // Find the __contains__ symbol ID
        let sym_id = self.codegen.class_info.get(&contains_class)
            .and_then(|info| {
                info.methods.iter()
                    .find(|&&id| {
                        self.codegen.resolve_symbol(id)
                            .map(|name| name == "__contains__" || name.ends_with(".__contains__"))
                            .unwrap_or(false)
                    })
                    .copied()
            })
            .unwrap_or(0);

        // Call __contains__(self, item) -> bool
        let contains_result = self.fresh_value();
        let func = format!("@roast_fn_{}_{}", contains_class, sym_id);
        self.ir.push_str(&format!("  {} = call i64 {}(i64 {}, i64 {})\n", contains_result, func, container_val, item_val));

        // For 'not in', negate the result
        if matches!(op, MirBinOp::NotIn) {
            let neg_result = self.fresh_value();
            // Result is i64 (0 or 1), so xor with 1 to negate
            self.ir.push_str(&format!("  {} = xor i64 {}, 1\n", neg_result, contains_result));
            Ok(neg_result)
        } else {
            Ok(contains_result)
        }
    }

    /// Generate unary operation.
    fn generate_unary_op(&mut self, op: &MirUnaryOp, operand: &MirOperand) -> LlvmResult<String> {
        let val = self.generate_operand(operand)?;
        let result = self.fresh_value();
        let ty = self.operand_type(operand);
        let is_float = matches!(ty, Type::Float | Type::Float32 | Type::Float64);

        match op {
            MirUnaryOp::Neg => {
                if is_float {
                    self.ir.push_str(&format!("  {} = fneg double {}\n", result, val));
                } else {
                    self.ir.push_str(&format!("  {} = sub i64 0, {}\n", result, val));
                }
            }
            MirUnaryOp::Not => {
                // Boolean values are stored as i64, so xor with 1 to negate
                self.ir.push_str(&format!("  {} = xor i64 {}, 1\n", result, val));
            }
            MirUnaryOp::BitNot => {
                self.ir.push_str(&format!("  {} = xor i64 {}, -1\n", result, val));
            }
        }
        Ok(result)
    }

    /// Generate aggregate value (list, dict, tuple, etc.).
    fn generate_aggregate(&mut self, kind: &MirAggregateKind, operands: &[MirOperand]) -> LlvmResult<String> {
        match kind {
            MirAggregateKind::List => {
                // Create new list
                let list = self.fresh_value();
                self.ir.push_str(&format!("  {} = call i8* @roast_list_new(i64 {})\n", list, operands.len()));

                // Append each element
                for op in operands {
                    let val = self.generate_operand(op)?;
                    self.ir.push_str(&format!("  call void @roast_list_append(i8* {}, i64 {})\n", list, val));
                }

                // Convert to i64 for tagged pointer
                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, list));
                Ok(result)
            }
            MirAggregateKind::Tuple => {
                // Create new tuple
                let tuple = self.fresh_value();
                self.ir.push_str(&format!("  {} = call i8* @roast_tuple_new(i64 {})\n", tuple, operands.len()));

                // Set each element (tuples are immutable, set during creation)
                for (i, op) in operands.iter().enumerate() {
                    let val = self.generate_operand(op)?;
                    self.ir.push_str(&format!("  call void @roast_tuple_set(i8* {}, i64 {}, i64 {})\n", tuple, i, val));
                }

                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, tuple));
                Ok(result)
            }
            MirAggregateKind::Dict => {
                // Create new dict
                let dict = self.fresh_value();
                self.ir.push_str(&format!("  {} = call i8* @roast_dict_new()\n", dict));

                // Operands come in key-value pairs
                for chunk in operands.chunks(2) {
                    if let [key, val] = chunk {
                        let k = self.generate_operand(key)?;
                        let v = self.generate_operand(val)?;
                        self.ir.push_str(&format!("  call void @roast_dict_set(i8* {}, i64 {}, i64 {})\n", dict, k, v));
                    }
                }

                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, dict));
                Ok(result)
            }
            MirAggregateKind::Set => {
                // Create new set
                let set = self.fresh_value();
                self.ir.push_str(&format!("  {} = call i8* @roast_set_new()\n", set));

                for op in operands {
                    let val = self.generate_operand(op)?;
                    self.ir.push_str(&format!("  call void @roast_set_add(i8* {}, i64 {})\n", set, val));
                }

                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, set));
                Ok(result)
            }
            MirAggregateKind::Struct(name) => {
                // Create new class instance
                let class_name = format!("{}", name.as_raw());
                let obj = self.fresh_value();

                // Load the class pointer from the global
                let class_ptr = self.fresh_value();
                self.ir.push_str(&format!("  {} = load i8*, i8** @.class.{}\n", class_ptr, class_name));
                self.ir.push_str(&format!("  {} = call i8* @roast_object_new(i8* {}) ; class {}\n", obj, class_ptr, class_name));

                // Set fields - would need class info for field names
                // For now just return the object pointer
                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, obj));
                Ok(result)
            }
            MirAggregateKind::Slice => {
                // Create slice object (start, stop, step)
                let slice = self.fresh_value();
                let start = if operands.len() > 0 { self.generate_operand(&operands[0])? } else { "0".to_string() };
                let stop = if operands.len() > 1 { self.generate_operand(&operands[1])? } else { "9223372036854775807".to_string() };
                let step = if operands.len() > 2 { self.generate_operand(&operands[2])? } else { "1".to_string() };

                self.ir.push_str(&format!("  {} = call i8* @roast_range_new(i64 {}, i64 {}, i64 {})\n", slice, start, stop, step));

                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, slice));
                Ok(result)
            }
            MirAggregateKind::Lambda { params, body } => {
                // Create closure - would need to compile the body as a separate function
                // For now, return a placeholder
                let closure = self.fresh_value();
                self.ir.push_str(&format!("  {} = call i8* @roast_closure_new(i8* null, i64 {}, i8* null)\n",
                    closure, params.len()));

                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, closure));
                Ok(result)
            }
        }
    }

    /// Generate length operation.
    fn generate_len(&mut self, ptr: &str, ty: &Type) -> LlvmResult<String> {
        let result = self.fresh_value();

        match ty {
            Type::List(_) => {
                self.ir.push_str(&format!("  {} = call i64 @roast_list_len(i8* inttoptr (i64 {} to i8*))\n", result, ptr));
            }
            Type::Str => {
                self.ir.push_str(&format!("  {} = call i64 @roast_str_len(i8* inttoptr (i64 {} to i8*))\n", result, ptr));
            }
            Type::Dict(_, _) => {
                self.ir.push_str(&format!("  {} = call i64 @roast_dict_len(i8* inttoptr (i64 {} to i8*))\n", result, ptr));
            }
            Type::Set(_) => {
                self.ir.push_str(&format!("  {} = call i64 @roast_set_len(i8* inttoptr (i64 {} to i8*))\n", result, ptr));
            }
            Type::Tuple(_) => {
                self.ir.push_str(&format!("  {} = call i64 @roast_tuple_len(i8* inttoptr (i64 {} to i8*))\n", result, ptr));
            }
            _ => {
                // Default: assume it's a collection
                self.ir.push_str(&format!("  {} = call i64 @roast_list_len(i8* inttoptr (i64 {} to i8*))\n", result, ptr));
            }
        }

        Ok(result)
    }

    /// Generate cast operation.
    fn generate_cast(&mut self, operand: &MirOperand, target_ty: &Type) -> LlvmResult<String> {
        let val = self.generate_operand(operand)?;
        let src_ty = self.operand_type(operand);
        let result = self.fresh_value();

        match (&src_ty, target_ty) {
            (Type::Int | Type::Int64, Type::Float | Type::Float64) => {
                self.ir.push_str(&format!("  {} = sitofp i64 {} to double\n", result, val));
            }
            (Type::Float | Type::Float64, Type::Int | Type::Int64) => {
                self.ir.push_str(&format!("  {} = fptosi double {} to i64\n", result, val));
            }
            (Type::Int | Type::Int64, Type::Bool) => {
                // Convert int to bool: non-zero = 1, zero = 0
                // First compare, then extend result to i64 (since Bool is stored as i64)
                let cmp = self.fresh_value();
                self.ir.push_str(&format!("  {} = icmp ne i64 {}, 0\n", cmp, val));
                self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, cmp));
            }
            (Type::Bool, Type::Int | Type::Int64) => {
                // Bool is already stored as i64, so just return the value
                return Ok(val);
            }
            (Type::Int | Type::Int64, Type::Str) => {
                self.ir.push_str(&format!("  {} = call i8* @roast_int_to_str(i64 {})\n", result, val));
            }
            (Type::Float | Type::Float64, Type::Str) => {
                self.ir.push_str(&format!("  {} = call i8* @roast_float_to_str(double {})\n", result, val));
            }
            (Type::Str, Type::Int | Type::Int64) => {
                self.ir.push_str(&format!("  {} = call i64 @roast_str_to_int(i8* inttoptr (i64 {} to i8*))\n", result, val));
            }
            _ => {
                // No cast needed or unsupported - just return the value
                return Ok(val);
            }
        }

        Ok(result)
    }

    /// Generate attribute access with property support.
    /// If the attribute is a property descriptor, this calls the getter.
    fn generate_attr_get(&mut self, operand: &MirOperand, attr: &Symbol) -> LlvmResult<String> {
        // Get attribute name as string constant - resolve through interner
        let attr_name = self.codegen.resolve_symbol(attr.as_raw())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Symbol not registered but may still be valid (e.g., mangled names)
                format!("attr_{}", attr.as_raw())
            });

        // Check if this is a class variable access (ClassName.attr)
        if let MirOperand::Global(class_sym) = operand {
            if let Some(class_name) = self.codegen.resolve_symbol(class_sym.as_raw()).map(|s| s.to_string()) {
                // Check if this class has this class variable
                if let Some(info) = self.codegen.class_info.get(&class_name) {
                    if info.class_variables.contains_key(&attr_name) {
                        // Load from the class variable global
                        let result = self.fresh_value();
                        self.ir.push_str(&format!(
                            "  {} = load i64, i64* @.classvar.{}.{}\n",
                            result, class_name, attr_name
                        ));
                        return Ok(result);
                    }
                }
            }
        }

        let obj = self.generate_operand(operand)?;
        let attr_global = self.codegen.add_string(&attr_name);

        let attr_ptr = self.fresh_value();
        self.ir.push_str(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
            attr_ptr, attr_name.len() + 1, attr_name.len() + 1, attr_global
        ));

        // Convert object to pointer separately (fixes LLVM verification issues)
        let obj_ptr = self.fresh_value();
        self.ir.push_str(&format!(
            "  {} = inttoptr i64 {} to i8*\n",
            obj_ptr, obj
        ));

        // Use roast_object_getattr_auto which handles both regular attrs and properties
        let result = self.fresh_value();
        self.ir.push_str(&format!(
            "  {} = call i64 @roast_object_getattr_auto(i8* {}, i8* {}, i64 {})\n",
            result, obj_ptr, attr_ptr, obj
        ));

        Ok(result)
    }

    /// Get the type of an operand.
    fn operand_type(&self, operand: &MirOperand) -> Type {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                self.get_place_type(place)
            }
            MirOperand::Constant(c) => match c {
                MirConstant::Int(_) => Type::Int,
                MirConstant::Float(_) => Type::Float,
                MirConstant::Bool(_) => Type::Bool,
                MirConstant::Str(_) => Type::Str,
                MirConstant::None => Type::NoneType,
                _ => Type::Int,
            },
            MirOperand::Global(sym) => {
                // Check for magic variables with known types
                if let Some(name) = self.codegen.resolve_symbol(sym.as_raw()) {
                    if name == "__name__" {
                        return Type::Str;
                    }
                }
                Type::Int // Functions are represented as pointers
            }
        }
    }

    /// Get the class name from a place if it's a class type.
    fn get_place_class_name(&self, place: &MirPlace) -> Option<String> {
        let ty = self.get_place_type(place);
        if let Type::Class(class_type) = ty {
            Some(class_type.name.clone())
        } else {
            None
        }
    }

    fn generate_operand(&mut self, operand: &MirOperand) -> LlvmResult<String> {
        match operand {
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                // Check if place has projections (field access, indexing)
                if !place.projections.is_empty() {
                    return self.generate_place_read(place);
                }

                let ptr = self.get_place_ptr(place);
                let ty = Self::type_to_llvm(&self.get_place_type(place));
                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = load {}, {}* {}\n", result, ty, ty, ptr));
                Ok(result)
            }
            MirOperand::Constant(constant) => {
                self.generate_constant(constant)
            }
            MirOperand::Global(sym) => {
                // Check for magic variables like __name__
                if let Some(name) = self.codegen.resolve_symbol(sym.as_raw()) {
                    if name == "__name__" {
                        // __name__ is always "__main__" when running as script
                        let str_ptr = self.codegen.add_string("__main__");
                        let str_gep = self.fresh_value();
                        self.ir.push_str(&format!(
                            "  {} = getelementptr [9 x i8], [9 x i8]* {}, i64 0, i64 0\n",
                            str_gep, str_ptr
                        ));
                        let result = self.fresh_value();
                        self.ir.push_str(&format!(
                            "  {} = call i8* @roast_str_new(i8* {}, i64 8)\n",
                            result, str_gep
                        ));
                        let final_result = self.fresh_value();
                        self.ir.push_str(&format!(
                            "  {} = ptrtoint i8* {} to i64\n",
                            final_result, result
                        ));
                        return Ok(final_result);
                    }
                }
                // Convert function pointer to i64 for use as value
                let func_name = format!("@roast_fn_{}", sym.as_raw());
                let result = self.fresh_value();
                self.ir.push_str(&format!("  {} = ptrtoint i8* bitcast (i64 (...)* {} to i8*) to i64\n", result, func_name));
                Ok(result)
            }
        }
    }

    /// Generate code to read from a place with projections (field access, indexing).
    fn generate_place_read(&mut self, place: &MirPlace) -> LlvmResult<String> {
        // Load the base value
        let base_ptr = self.get_place_ptr(&MirPlace::local(place.local));
        let base = self.fresh_value();
        self.ir.push_str(&format!("  {} = load i64, i64* {}\n", base, base_ptr));

        let base_ty = self.get_place_type(&MirPlace::local(place.local));

        // Apply projections to get the final value
        let mut current = base;
        let mut current_ty = base_ty;

        for proj in &place.projections {
            match proj {
                MirProjection::Field(idx) => {
                    // Get field from object
                    let field_name = format!("field_{}", idx);
                    let field_ptr = self.codegen.add_string(&field_name);
                    let field_ptr_val = self.fresh_value();
                    self.ir.push_str(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                        field_ptr_val, field_name.len() + 1, field_name.len() + 1, field_ptr
                    ));

                    let obj_ptr = self.i64_to_ptr(&current);
                    let result = self.fresh_value();
                    self.ir.push_str(&format!(
                        "  {} = call i64 @roast_object_getattr(i8* {}, i8* {})\n",
                        result, obj_ptr, field_ptr_val
                    ));
                    current = result;
                    current_ty = Type::Unknown; // Field type is unknown
                }
                MirProjection::Index(idx_local) => {
                    // Get element from list/dict
                    let idx_ptr = self.locals.get(idx_local).cloned().unwrap_or_else(|| format!("%local{}", idx_local));
                    let idx = self.fresh_value();
                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", idx, idx_ptr));

                    let result = self.fresh_value();

                    match &current_ty {
                        Type::List(_) => {
                            let base_ptr = self.i64_to_ptr(&current);
                            self.ir.push_str(&format!(
                                "  {} = call i64 @roast_list_get(i8* {}, i64 {})\n",
                                result, base_ptr, idx
                            ));
                        }
                        Type::Dict(_, _) => {
                            let base_ptr = self.i64_to_ptr(&current);
                            self.ir.push_str(&format!(
                                "  {} = call i64 @roast_dict_get(i8* {}, i64 {})\n",
                                result, base_ptr, idx
                            ));
                        }
                        Type::Class(cls) => {
                            // Check if class has __getitem__ method
                            let class_name = &cls.name;
                            if self.codegen.class_has_method(class_name, "__getitem__") {
                                // Find the class that defines __getitem__ (could be parent)
                                let getitem_class = self.codegen.class_info.get(class_name)
                                    .and_then(|info| {
                                        if info.method_names.contains("__getitem__") {
                                            Some(class_name.clone())
                                        } else {
                                            info.mro.iter()
                                                .find(|p| {
                                                    self.codegen.class_info.get(*p)
                                                        .map(|pi| pi.method_names.contains("__getitem__"))
                                                        .unwrap_or(false)
                                                })
                                                .cloned()
                                        }
                                    })
                                    .unwrap_or(class_name.clone());

                                // Find the __getitem__ symbol ID
                                let getitem_sym_id = self.codegen.class_info.get(&getitem_class)
                                    .and_then(|info| {
                                        info.methods.iter()
                                            .find(|&&id| {
                                                self.codegen.resolve_symbol(id)
                                                    .map(|name| name == "__getitem__" || name.ends_with(".__getitem__"))
                                                    .unwrap_or(false)
                                            })
                                            .copied()
                                    })
                                    .unwrap_or(0);

                                // Call __getitem__(self, key)
                                let func = format!("@roast_fn_{}_{}", getitem_class, getitem_sym_id);
                                self.ir.push_str(&format!(
                                    "  {} = call i64 {}(i64 {}, i64 {})\n",
                                    result, func, current, idx
                                ));
                            } else {
                                // Fall back to runtime subscript
                                self.ir.push_str(&format!(
                                    "  {} = call i64 @roast_subscript_get(i64 {}, i64 {})\n",
                                    result, current, idx
                                ));
                            }
                        }
                        Type::Str => {
                            // String indexing - returns a single-character string
                            let base_ptr = self.i64_to_ptr(&current);
                            let str_result = self.fresh_value();
                            self.ir.push_str(&format!(
                                "  {} = call i8* @roast_str_index(i8* {}, i64 {})\n",
                                str_result, base_ptr, idx
                            ));
                            self.ir.push_str(&format!(
                                "  {} = ptrtoint i8* {} to i64\n",
                                result, str_result
                            ));
                        }
                        _ => {
                            // Use runtime-typed subscript function
                            self.ir.push_str(&format!(
                                "  {} = call i64 @roast_subscript_get(i64 {}, i64 {})\n",
                                result, current, idx
                            ));
                        }
                    }
                    current = result;
                    current_ty = Type::Unknown; // Element type
                }
                MirProjection::Slice { lower, upper, step } => {
                    // Load slice bounds
                    let lower_ptr = self.locals.get(lower).cloned().unwrap_or_else(|| format!("%local{}", lower));
                    let upper_ptr = self.locals.get(upper).cloned().unwrap_or_else(|| format!("%local{}", upper));
                    let step_ptr = self.locals.get(step).cloned().unwrap_or_else(|| format!("%local{}", step));

                    let lower_val = self.fresh_value();
                    let upper_val = self.fresh_value();
                    let step_val = self.fresh_value();

                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", lower_val, lower_ptr));
                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", upper_val, upper_ptr));
                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", step_val, step_ptr));

                    let result = self.fresh_value();
                    let base_ptr = self.i64_to_ptr(&current);

                    match current_ty {
                        Type::List(_) => {
                            // Call roast_list_slice(list, start, stop, step)
                            let slice_ptr = self.fresh_value();
                            self.ir.push_str(&format!(
                                "  {} = call i8* @roast_list_slice(i8* {}, i64 {}, i64 {}, i64 {})\n",
                                slice_ptr, base_ptr, lower_val, upper_val, step_val
                            ));
                            self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, slice_ptr));
                        }
                        Type::Str => {
                            // Call roast_str_slice(str, start, stop) - note: no step support
                            let slice_ptr = self.fresh_value();
                            self.ir.push_str(&format!(
                                "  {} = call i8* @roast_str_slice(i8* {}, i64 {}, i64 {})\n",
                                slice_ptr, base_ptr, lower_val, upper_val
                            ));
                            self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, slice_ptr));
                        }
                        _ => {
                            // Generic slice - use runtime function
                            let slice_ptr = self.fresh_value();
                            self.ir.push_str(&format!(
                                "  {} = call i8* @roast_slice_get(i64 {}, i64 {}, i64 {}, i64 {})\n",
                                slice_ptr, current, lower_val, upper_val, step_val
                            ));
                            self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, slice_ptr));
                        }
                    }
                    current = result;
                    // Slicing a list returns a list, string returns string
                    // current_ty stays the same for now
                }
                MirProjection::Deref => {
                    // Dereference pointer
                    let ptr = self.fresh_value();
                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i64*\n", ptr, current));
                    let result = self.fresh_value();
                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", result, ptr));
                    current = result;
                }
            }
        }

        Ok(current)
    }

    /// Generate an operand and ensure it's an i64.
    /// Bool is already stored as i64, so no extension needed.
    fn generate_operand_as_i64(&mut self, operand: &MirOperand) -> LlvmResult<String> {
        let val = self.generate_operand(operand)?;
        // Bool is now stored as i64, so no extension needed
        Ok(val)
    }

    fn generate_constant(&mut self, constant: &MirConstant) -> LlvmResult<String> {
        match constant {
            MirConstant::Int(n) => Ok(format!("{}", n)),
            MirConstant::Float(f) => {
                // LLVM requires explicit decimal point for floating point constants
                // Format with enough precision and ensure decimal point exists
                let formatted = format!("{:.15e}", f);
                Ok(formatted)
            }
            MirConstant::Bool(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
            MirConstant::None | MirConstant::Unit => Ok("0".to_string()),
            MirConstant::Str(s) => {
                // Create a string constant and call roast_str_new
                let str_global = self.codegen.add_string(s);

                // Get pointer to the string data
                let str_ptr = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                    str_ptr, s.len() + 1, s.len() + 1, str_global
                ));

                // Create a RoastString object
                let roast_str = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = call i8* @roast_str_new(i8* {}, i64 {})\n",
                    roast_str, str_ptr, s.len()
                ));

                // Convert to i64 for uniform value handling
                let result = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = ptrtoint i8* {} to i64\n",
                    result, roast_str
                ));

                Ok(result)
            }
            _ => Ok("0".to_string()),
        }
    }

    fn generate_terminator(&mut self, term: &MirTerminator) -> LlvmResult<()> {
        match term {
            MirTerminator::Return(Some(operand)) => {
                let val = self.generate_operand(operand)?;
                let ty = Self::type_to_llvm(&self.body.return_ty);
                self.ir.push_str(&format!("  ret {} {}\n", ty, val));
            }
            MirTerminator::Return(None) => {
                // NoneType returns i64 0 (None sentinel value)
                let ty = Self::type_to_llvm(&self.body.return_ty);
                if self.body.return_ty == Type::NoneType {
                    self.ir.push_str(&format!("  ret {} 0\n", ty));
                } else {
                    // Return from local 0
                    let ptr = self.locals.get(&0).cloned();
                    if let Some(ptr) = ptr {
                        let val = self.fresh_value();
                        self.ir.push_str(&format!("  {} = load {}, {}* {}\n", val, ty, ty, ptr));
                        self.ir.push_str(&format!("  ret {} {}\n", ty, val));
                    } else {
                        self.ir.push_str("  ret i64 0\n");
                    }
                }
            }
            MirTerminator::Goto(target) => {
                self.ir.push_str(&format!("  br label %bb{}\n", target));
            }
            MirTerminator::SwitchInt { discr, targets, otherwise } => {
                // Check if discriminant is a class type with __bool__ method
                let class_with_bool = match discr {
                    MirOperand::Copy(place) | MirOperand::Move(place) => {
                        if let Type::Class(cls) = self.get_place_type(place) {
                            if self.codegen.class_has_method(&cls.name, "__bool__") {
                                Some(cls.name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // If it's a class with __bool__, call it to get the boolean value
                let val = if let Some(class_name) = class_with_bool {
                    let obj_val = self.generate_operand(discr)?;

                    // Find the __bool__ method
                    let bool_class = self.codegen.class_info.get(&class_name)
                        .and_then(|info| {
                            if info.method_names.contains("__bool__") {
                                Some(class_name.clone())
                            } else {
                                info.mro.iter()
                                    .find(|p| {
                                        self.codegen.class_info.get(*p)
                                            .map(|pi| pi.method_names.contains("__bool__"))
                                            .unwrap_or(false)
                                    })
                                    .cloned()
                            }
                        })
                        .unwrap_or(class_name.clone());

                    let bool_sym_id = self.codegen.class_info.get(&bool_class)
                        .and_then(|info| {
                            info.methods.iter()
                                .find(|&&id| {
                                    self.codegen.resolve_symbol(id)
                                        .map(|name| name == "__bool__" || name.ends_with(".__bool__"))
                                        .unwrap_or(false)
                                })
                                .copied()
                        })
                        .unwrap_or(0);

                    // Call __bool__(self)
                    let func = format!("@roast_fn_{}_{}", bool_class, bool_sym_id);
                    let result = self.fresh_value();
                    self.ir.push_str(&format!(
                        "  {} = call i64 {}(i64 {})\n",
                        result, func, obj_val
                    ));
                    result
                } else {
                    self.generate_operand(discr)?
                };

                // Determine the type of the discriminant
                let discr_type = match discr {
                    MirOperand::Copy(place) | MirOperand::Move(place) => {
                        let ty = self.get_place_type(place);
                        // If we called __bool__, the result is i64 (bool as int)
                        if matches!(ty, Type::Class(_)) {
                            "i64"
                        } else {
                            Self::type_to_llvm(&ty)
                        }
                    }
                    MirOperand::Constant(c) => match c {
                        MirConstant::Bool(_) => "i1",
                        MirConstant::Int(_) => "i64",
                        _ => "i64",
                    },
                    _ => "i64",
                };

                if targets.len() == 1 {
                    // Simple branch - likely a boolean condition
                    let (test_val, target) = &targets[0];

                    if discr_type == "i1" {
                        // Boolean - direct branch
                        if *test_val == 1 {
                            self.ir.push_str(&format!("  br i1 {}, label %bb{}, label %bb{}\n", val, target, otherwise));
                        } else {
                            // test_val == 0, flip the branches
                            self.ir.push_str(&format!("  br i1 {}, label %bb{}, label %bb{}\n", val, otherwise, target));
                        }
                    } else {
                        // Integer comparison
                        let cmp = self.fresh_value();
                        self.ir.push_str(&format!("  {} = icmp eq {} {}, {}\n", cmp, discr_type, val, test_val));
                        self.ir.push_str(&format!("  br i1 {}, label %bb{}, label %bb{}\n", cmp, target, otherwise));
                    }
                } else {
                    // Switch statement (requires integer)
                    self.ir.push_str(&format!("  switch {} {}, label %bb{} [\n", discr_type, val, otherwise));
                    for (test_val, target) in targets {
                        self.ir.push_str(&format!("    {} {}, label %bb{}\n", discr_type, test_val, target));
                    }
                    self.ir.push_str("  ]\n");
                }
            }
            MirTerminator::Call { func, args, destination, target, unwind } => {
                // Generate call
                let func_name = match func {
                    MirOperand::Global(sym) => {
                        let sym_id = sym.as_raw();

                        // Check for special intrinsic symbols first
                        if *sym == Symbol::MAKE_ITER {
                            // Handle iterator creation intrinsic
                            let arg_val = self.generate_operand(&args[0])?;
                            let arg_ptr = self.i64_to_ptr(&arg_val);
                            let result = self.fresh_value();
                            self.ir.push_str(&format!(
                                "  {} = call i8* @roast_iter_new(i8* {})\n",
                                result, arg_ptr
                            ));
                            // Convert result pointer to i64 and store
                            let result_i64 = self.fresh_value();
                            self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result_i64, result));
                            let ptr = self.get_place_ptr(destination);
                            self.ir.push_str(&format!("  store i64 {}, i64* {}\n", result_i64, ptr));
                            if let Some(target) = target {
                                self.ir.push_str(&format!("  br label %bb{}\n", target));
                            }
                            return Ok(());
                        }

                        // Try to resolve the symbol name
                        let resolved_name = self.codegen.resolve_symbol(sym_id)
                            .map(|s| s.to_string());

                        if let Some(ref name) = resolved_name {
                            // Check if it's an exception type constructor
                            if is_exception_type(name) {
                                // Generate exception object creation
                                let exc_type_code = get_exception_type_code(name);
                                let msg_val = if args.is_empty() {
                                    "0".to_string() // No message
                                } else {
                                    self.generate_operand_as_i64(&args[0])?
                                };
                                let result = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = call i64 @roast_exception_new(i64 {}, i64 {})\n",
                                    result, exc_type_code, msg_val
                                ));
                                let ptr = self.get_place_ptr(destination);
                                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", result, ptr));
                                if let Some(target) = target {
                                    self.ir.push_str(&format!("  br label %bb{}\n", target));
                                }
                                return Ok(());
                            }
                            // Check if it's super() call
                            if is_super(name) {
                                // super() returns a proxy object for parent class method lookup
                                // In a method context, super() gets self (implicit) and the current class
                                // For now, we pass self as the first implicit argument
                                // The runtime will use reflection to find the parent class

                                // Get the 'self' parameter - it's always the first argument/local (%v0)
                                // In our MIR, arg0 is stored to %v0 at entry, so we load from there
                                let self_val = {
                                    let self_ptr = "%v0";
                                    let tmp = self.fresh_value();
                                    self.ir.push_str(&format!("  {} = load i64, i64* {}\n", tmp, self_ptr));
                                    tmp
                                };

                                // Convert self to pointer
                                let self_ptr = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = inttoptr i64 {} to i8*\n",
                                    self_ptr, self_val
                                ));

                                // Create super proxy: roast_super_new(self, class_ptr)
                                // For now, pass null as class - runtime will determine from self
                                let result = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = call i8* @roast_super_new(i8* {}, i8* null)\n",
                                    result, self_ptr
                                ));

                                // Convert pointer to i64 and store
                                let result_i64 = self.fresh_value();
                                self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result_i64, result));
                                let ptr = self.get_place_ptr(destination);
                                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", result_i64, ptr));

                                if let Some(target) = target {
                                    self.ir.push_str(&format!("  br label %bb{}\n", target));
                                }
                                return Ok(());
                            }
                            // Check if it's a builtin function
                            if is_builtin(name) {
                                // Handle min/max specially based on argument count
                                if (name == "min" || name == "max") && args.len() == 1 {
                                    // Single argument means it's a list
                                    format!("@roast_{}_list", name)
                                } else if name == "print" && args.len() > 1 {
                                    // Multi-argument print - handle specially
                                    "@@multi_print@@".to_string()
                                } else if name == "str" && args.len() == 1 {
                                    // Check if argument is a float - use roast_float_to_str
                                    let arg_ty = self.operand_type(&args[0]);
                                    if matches!(arg_ty, Type::Float | Type::Float32 | Type::Float64) {
                                        // Will be handled specially below
                                        "@@float_to_str@@".to_string() // Special marker
                                    } else {
                                        format!("@roast_{}", name)
                                    }
                                } else if name == "type" && args.len() == 1 {
                                    // Check if argument is a float - use compile-time type knowledge
                                    let arg_ty = self.operand_type(&args[0]);
                                    if matches!(arg_ty, Type::Float | Type::Float32 | Type::Float64) {
                                        // Will be handled specially below - return roast_type_float constant
                                        "@@type_of_float@@".to_string() // Special marker
                                    } else {
                                        format!("@roast_{}", name)
                                    }
                                } else {
                                    format!("@roast_{}", name)
                                }
                            } else {
                                // Check if this is a method call on a builtin type
                                // For method calls, the first argument is 'self'
                                if !args.is_empty() {
                                    if let Some(collection_kind) = self.get_operand_collection_kind(&args[0]) {
                                        if let Some(runtime_fn) = get_builtin_method_by_kind(collection_kind, name) {
                                            format!("@{}", runtime_fn)
                                        } else {
                                            format!("@roast_fn_{}", sym_id)
                                        }
                                    } else {
                                        format!("@roast_fn_{}", sym_id)
                                    }
                                } else {
                                    format!("@roast_fn_{}", sym_id)
                                }
                            }
                        } else {
                            // Fallback to raw ID
                            format!("@roast_fn_{}", sym_id)
                        }
                    }
                    MirOperand::Copy(place) | MirOperand::Move(place) => {
                        // Indirect call through function pointer
                        let ptr = self.get_place_ptr(place);
                        let func_ptr = self.fresh_value();
                        self.ir.push_str(&format!("  {} = load i64, i64* {}\n", func_ptr, ptr));
                        func_ptr
                    }
                    _ => return Err(LlvmError::IrGen("unsupported call target".to_string())),
                };

                // Special handling for print() - check if argument is a class with __str__
                // Special handling for len() - check if argument is a class with __len__
                let arg_vals: Vec<String> = if func_name == "@roast_print" && args.len() == 1 {
                    // Check if the argument is a class type with __str__ method
                    if let Some(class_name) = self.get_operand_class_name(&args[0]) {
                        if self.codegen.class_has_method(&class_name, "__str__") {
                            // Generate code to call __str__() first
                            let obj_val = self.generate_operand_as_i64(&args[0])?;

                            // Look up which class defines __str__ (could be parent)
                            let str_class = self.codegen.class_info.get(&class_name)
                                .and_then(|info| {
                                    if info.method_names.contains("__str__") {
                                        Some(class_name.clone())
                                    } else {
                                        info.mro.iter()
                                            .find(|p| {
                                                self.codegen.class_info.get(*p)
                                                    .map(|pi| pi.method_names.contains("__str__"))
                                                    .unwrap_or(false)
                                            })
                                            .cloned()
                                    }
                                })
                                .unwrap_or(class_name.clone());

                            // Find the __str__ symbol ID by looking at which method symbol resolves to "__str__"
                            // Note: symbol may resolve to "ClassName.__str__" or "__str__"
                            let str_sym_id = self.codegen.class_info.get(&str_class)
                                .and_then(|info| {
                                    info.methods.iter()
                                        .find(|&&id| {
                                            self.codegen.resolve_symbol(id)
                                                .map(|name| name == "__str__" || name.ends_with(".__str__"))
                                                .unwrap_or(false)
                                        })
                                        .copied()
                                })
                                .unwrap_or(0);

                            // Call __str__ method
                            let str_result = self.fresh_value();
                            let func = format!("@roast_fn_{}_{}", str_class, str_sym_id);
                            self.ir.push_str(&format!("  {} = call i64 {}(i64 {})\n", str_result, func, obj_val));

                            vec![str_result]
                        } else {
                            // No __str__, use the raw value
                            args.iter()
                                .map(|a| self.generate_operand_as_i64(a))
                                .collect::<Result<_, _>>()?
                        }
                    } else {
                        // Not a class type
                        args.iter()
                            .map(|a| self.generate_operand_as_i64(a))
                            .collect::<Result<_, _>>()?
                    }
                } else if func_name == "@roast_len" && args.len() == 1 {
                    // Check if the argument is a class type with __len__ method
                    if let Some(class_name) = self.get_operand_class_name(&args[0]) {
                        if self.codegen.class_has_method(&class_name, "__len__") {
                            // Generate code to call __len__() and return directly
                            let obj_val = self.generate_operand_as_i64(&args[0])?;

                            // Look up which class defines __len__ (could be parent)
                            let len_class = self.codegen.class_info.get(&class_name)
                                .and_then(|info| {
                                    if info.method_names.contains("__len__") {
                                        Some(class_name.clone())
                                    } else {
                                        info.mro.iter()
                                            .find(|p| {
                                                self.codegen.class_info.get(*p)
                                                    .map(|pi| pi.method_names.contains("__len__"))
                                                    .unwrap_or(false)
                                            })
                                            .cloned()
                                    }
                                })
                                .unwrap_or(class_name.clone());

                            // Find the __len__ symbol ID
                            let len_sym_id = self.codegen.class_info.get(&len_class)
                                .and_then(|info| {
                                    info.methods.iter()
                                        .find(|&&id| {
                                            self.codegen.resolve_symbol(id)
                                                .map(|name| name == "__len__" || name.ends_with(".__len__"))
                                                .unwrap_or(false)
                                        })
                                        .copied()
                                })
                                .unwrap_or(0);

                            // Call __len__ method directly and store result
                            let len_result = self.fresh_value();
                            let func = format!("@roast_fn_{}_{}", len_class, len_sym_id);
                            self.ir.push_str(&format!("  {} = call i64 {}(i64 {})\n", len_result, func, obj_val));

                            // Store and return early - don't call @roast_len
                            if self.locals.contains_key(&destination.local) {
                                let ptr = self.get_place_ptr(destination);
                                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", len_result, ptr));
                            }
                            if let Some(target) = target {
                                self.ir.push_str(&format!("  br label %bb{}\n", target));
                            }
                            return Ok(());
                        } else {
                            // No __len__, use the raw value
                            args.iter()
                                .map(|a| self.generate_operand_as_i64(a))
                                .collect::<Result<_, _>>()?
                        }
                    } else {
                        // Not a class type
                        args.iter()
                            .map(|a| self.generate_operand_as_i64(a))
                            .collect::<Result<_, _>>()?
                    }
                } else if func_name == "@roast_repr" && args.len() == 1 {
                    // Check if the argument is a class type with __repr__ method
                    if let Some(class_name) = self.get_operand_class_name(&args[0]) {
                        if self.codegen.class_has_method(&class_name, "__repr__") {
                            // Generate code to call __repr__() and return directly
                            let obj_val = self.generate_operand_as_i64(&args[0])?;

                            // Look up which class defines __repr__ (could be parent)
                            let repr_class = self.codegen.class_info.get(&class_name)
                                .and_then(|info| {
                                    if info.method_names.contains("__repr__") {
                                        Some(class_name.clone())
                                    } else {
                                        info.mro.iter()
                                            .find(|p| {
                                                self.codegen.class_info.get(*p)
                                                    .map(|pi| pi.method_names.contains("__repr__"))
                                                    .unwrap_or(false)
                                            })
                                            .cloned()
                                    }
                                })
                                .unwrap_or(class_name.clone());

                            // Find the __repr__ symbol ID
                            let repr_sym_id = self.codegen.class_info.get(&repr_class)
                                .and_then(|info| {
                                    info.methods.iter()
                                        .find(|&&id| {
                                            self.codegen.resolve_symbol(id)
                                                .map(|name| name == "__repr__" || name.ends_with(".__repr__"))
                                                .unwrap_or(false)
                                        })
                                        .copied()
                                })
                                .unwrap_or(0);

                            // Call __repr__ method directly and store result
                            let repr_result = self.fresh_value();
                            let func = format!("@roast_fn_{}_{}", repr_class, repr_sym_id);
                            self.ir.push_str(&format!("  {} = call i64 {}(i64 {})\n", repr_result, func, obj_val));

                            // Store and return early - don't call @roast_repr
                            if self.locals.contains_key(&destination.local) {
                                let ptr = self.get_place_ptr(destination);
                                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", repr_result, ptr));
                            }
                            if let Some(target) = target {
                                self.ir.push_str(&format!("  br label %bb{}\n", target));
                            }
                            return Ok(());
                        } else {
                            // No __repr__, use the raw value
                            args.iter()
                                .map(|a| self.generate_operand_as_i64(a))
                                .collect::<Result<_, _>>()?
                        }
                    } else {
                        // Not a class type
                        args.iter()
                            .map(|a| self.generate_operand_as_i64(a))
                            .collect::<Result<_, _>>()?
                    }
                } else if func_name == "@roast_str" && args.len() == 1 {
                    // Check if the argument is a class type with __str__ method
                    if let Some(class_name) = self.get_operand_class_name(&args[0]) {
                        if self.codegen.class_has_method(&class_name, "__str__") {
                            // Generate code to call __str__() and return directly
                            let obj_val = self.generate_operand_as_i64(&args[0])?;

                            // Look up which class defines __str__ (could be parent)
                            let str_class = self.codegen.class_info.get(&class_name)
                                .and_then(|info| {
                                    if info.method_names.contains("__str__") {
                                        Some(class_name.clone())
                                    } else {
                                        info.mro.iter()
                                            .find(|p| {
                                                self.codegen.class_info.get(*p)
                                                    .map(|pi| pi.method_names.contains("__str__"))
                                                    .unwrap_or(false)
                                            })
                                            .cloned()
                                    }
                                })
                                .unwrap_or(class_name.clone());

                            // Find the __str__ symbol ID
                            let str_sym_id = self.codegen.class_info.get(&str_class)
                                .and_then(|info| {
                                    info.methods.iter()
                                        .find(|&&id| {
                                            self.codegen.resolve_symbol(id)
                                                .map(|name| name == "__str__" || name.ends_with(".__str__"))
                                                .unwrap_or(false)
                                        })
                                        .copied()
                                })
                                .unwrap_or(0);

                            // Call __str__ method directly and store result
                            let str_result = self.fresh_value();
                            let func = format!("@roast_fn_{}_{}", str_class, str_sym_id);
                            self.ir.push_str(&format!("  {} = call i64 {}(i64 {})\n", str_result, func, obj_val));

                            // Store and return early - don't call @roast_str
                            if self.locals.contains_key(&destination.local) {
                                let ptr = self.get_place_ptr(destination);
                                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", str_result, ptr));
                            }
                            if let Some(target) = target {
                                self.ir.push_str(&format!("  br label %bb{}\n", target));
                            }
                            return Ok(());
                        } else {
                            // No __str__, use the raw value
                            args.iter()
                                .map(|a| self.generate_operand_as_i64(a))
                                .collect::<Result<_, _>>()?
                        }
                    } else {
                        // Not a class type
                        args.iter()
                            .map(|a| self.generate_operand_as_i64(a))
                            .collect::<Result<_, _>>()?
                    }
                } else if func_name == "@roast_bool" && args.len() == 1 {
                    // Check if the argument is a class type with __bool__ method
                    if let Some(class_name) = self.get_operand_class_name(&args[0]) {
                        if self.codegen.class_has_method(&class_name, "__bool__") {
                            // Generate code to call __bool__() and return directly
                            let obj_val = self.generate_operand_as_i64(&args[0])?;

                            // Look up which class defines __bool__ (could be parent)
                            let bool_class = self.codegen.class_info.get(&class_name)
                                .and_then(|info| {
                                    if info.method_names.contains("__bool__") {
                                        Some(class_name.clone())
                                    } else {
                                        info.mro.iter()
                                            .find(|p| {
                                                self.codegen.class_info.get(*p)
                                                    .map(|pi| pi.method_names.contains("__bool__"))
                                                    .unwrap_or(false)
                                            })
                                            .cloned()
                                    }
                                })
                                .unwrap_or(class_name.clone());

                            // Find the __bool__ symbol ID
                            let bool_sym_id = self.codegen.class_info.get(&bool_class)
                                .and_then(|info| {
                                    info.methods.iter()
                                        .find(|&&id| {
                                            self.codegen.resolve_symbol(id)
                                                .map(|name| name == "__bool__" || name.ends_with(".__bool__"))
                                                .unwrap_or(false)
                                        })
                                        .copied()
                                })
                                .unwrap_or(0);

                            // Call __bool__ method directly and store result
                            let bool_result = self.fresh_value();
                            let func = format!("@roast_fn_{}_{}", bool_class, bool_sym_id);
                            self.ir.push_str(&format!("  {} = call i64 {}(i64 {})\n", bool_result, func, obj_val));

                            // Store and return early - don't call @roast_bool
                            if self.locals.contains_key(&destination.local) {
                                let ptr = self.get_place_ptr(destination);
                                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", bool_result, ptr));
                            }
                            if let Some(target) = target {
                                self.ir.push_str(&format!("  br label %bb{}\n", target));
                            }
                            return Ok(());
                        } else {
                            // No __bool__, use the raw value
                            args.iter()
                                .map(|a| self.generate_operand_as_i64(a))
                                .collect::<Result<_, _>>()?
                        }
                    } else {
                        // Not a class type
                        args.iter()
                            .map(|a| self.generate_operand_as_i64(a))
                            .collect::<Result<_, _>>()?
                    }
                } else {
                    args.iter()
                        .map(|a| self.generate_operand_as_i64(a))
                        .collect::<Result<_, _>>()?
                };

                let result = self.fresh_value();

                // Builtin collection/string methods need pointer signatures regardless of '@'
                let is_builtin_method = func_name.contains("roast_list_")
                    || func_name.contains("roast_dict_")
                    || func_name.contains("roast_str_")
                    || func_name.contains("roast_set_");

                // Check if this is a direct or indirect call
                if func_name.starts_with("@") {
                    // Special handling for range() to provide defaults
                    if func_name == "@roast_range" {
                        // range(stop) -> range(0, stop, 1)
                        // range(start, stop) -> range(start, stop, 1)
                        // range(start, stop, step) -> range(start, stop, step)
                        let (start, stop, step) = match arg_vals.len() {
                            1 => ("0".to_string(), arg_vals[0].clone(), "1".to_string()),
                            2 => (arg_vals[0].clone(), arg_vals[1].clone(), "1".to_string()),
                            3 => (arg_vals[0].clone(), arg_vals[1].clone(), arg_vals[2].clone()),
                            _ => return Err(LlvmError::IrGen("range() takes 1-3 arguments".to_string())),
                        };
                        self.ir.push_str(&format!(
                            "  {} = call i64 @roast_range(i64 {}, i64 {}, i64 {})\n",
                            result, start, stop, step
                        ));
                    } else if func_name == "@roast_isinstance" && args.len() == 2 {
                        // Special handling for isinstance(obj, Class)
                        // The second argument is the class type - we need to get the class pointer
                        let obj_val = &arg_vals[0];

                        // Try to get the class name from the type of the second argument
                        // The second operand should be a class type reference
                        let class_name_opt: Option<String> = match &args[1] {
                            MirOperand::Constant(c) => {
                                // If it's a function reference (class constructor), extract from the call
                                if let MirConstant::Int(id) = c {
                                    // Try to resolve as symbol ID
                                    self.codegen.resolve_symbol(*id as u32).map(|s| s.to_string())
                                }
                                else {
                                    None
                                }
                            }
                            MirOperand::Copy(place) | MirOperand::Move(place) => {
                                // Get the type from the local and see if it's a class
                                self.get_place_class_name(place)
                            }
                            MirOperand::Global(sym) => {
                                // This is likely a class constructor reference
                                self.codegen.resolve_symbol(sym.as_raw()).map(|s| s.to_string())
                            }
                        };

                        if let Some(class_name) = class_name_opt {
                            if self.codegen.class_info.contains_key(&class_name) {
                                // Load class pointer from global
                                let class_ptr = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = load i8*, i8** @.class.{}\n",
                                    class_ptr, class_name
                                ));
                                let class_i64 = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = ptrtoint i8* {} to i64\n",
                                    class_i64, class_ptr
                                ));
                                self.ir.push_str(&format!(
                                    "  {} = call i64 @roast_isinstance(i64 {}, i64 {})\n",
                                    result, obj_val, class_i64
                                ));
                            } else {
                                // Unknown class, return 0
                                self.ir.push_str(&format!("  {} = add i64 0, 0\n", result));
                            }
                        } else {
                            // Can't determine class, use default behavior
                            let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                            self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, args_str));
                        }
                    } else if func_name == "@roast_issubclass" && args.len() == 2 {
                        // Special handling for issubclass(SubClass, SuperClass)
                        // Both arguments are class types - we need to get the class pointers

                        // Helper to get class name from operand
                        let get_class_name = |arg: &MirOperand| -> Option<String> {
                            match arg {
                                MirOperand::Constant(c) => {
                                    if let MirConstant::Int(id) = c {
                                        self.codegen.resolve_symbol(*id as u32).map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                }
                                MirOperand::Copy(place) | MirOperand::Move(place) => {
                                    self.get_place_class_name(place)
                                }
                                MirOperand::Global(sym) => {
                                    self.codegen.resolve_symbol(sym.as_raw()).map(|s| s.to_string())
                                }
                            }
                        };

                        let subclass_name = get_class_name(&args[0]);
                        let superclass_name = get_class_name(&args[1]);

                        if let (Some(sub_name), Some(super_name)) = (subclass_name, superclass_name) {
                            if self.codegen.class_info.contains_key(&sub_name) && self.codegen.class_info.contains_key(&super_name) {
                                // Load both class pointers from globals
                                let sub_ptr = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = load i8*, i8** @.class.{}\n",
                                    sub_ptr, sub_name
                                ));
                                let sub_i64 = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = ptrtoint i8* {} to i64\n",
                                    sub_i64, sub_ptr
                                ));

                                let super_ptr = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = load i8*, i8** @.class.{}\n",
                                    super_ptr, super_name
                                ));
                                let super_i64 = self.fresh_value();
                                self.ir.push_str(&format!(
                                    "  {} = ptrtoint i8* {} to i64\n",
                                    super_i64, super_ptr
                                ));

                                self.ir.push_str(&format!(
                                    "  {} = call i64 @roast_issubclass(i64 {}, i64 {})\n",
                                    result, sub_i64, super_i64
                                ));
                            } else {
                                // Unknown class, return 0
                                self.ir.push_str(&format!("  {} = add i64 0, 0\n", result));
                            }
                        } else {
                            // Can't determine classes, use default behavior
                            let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                            self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, args_str));
                        }
                    } else if is_builtin_method {
                        // Builtin method call - first arg needs to be converted to pointer
                        self.generate_builtin_method_call(&func_name, &arg_vals, &result)?;
                    } else if func_name == "@@float_to_str@@" {
                        // Special handling for str(float_value)
                        // Generate the float argument directly (not as i64)
                        let float_val = self.generate_operand(&args[0])?;
                        let str_ptr = self.fresh_value();
                        self.ir.push_str(&format!(
                            "  {} = call i8* @roast_float_to_str(double {})\n",
                            str_ptr, float_val
                        ));
                        // Convert string pointer to i64
                        self.ir.push_str(&format!(
                            "  {} = ptrtoint i8* {} to i64\n",
                            result, str_ptr
                        ));
                    } else if func_name == "@@type_of_float@@" {
                        // Special handling for type(float_value)
                        // We know at compile time that this is a float, so just load the constant
                        self.ir.push_str(&format!(
                            "  {} = load i64, i64* @roast_type_float\n",
                            result
                        ));
                    } else if func_name == "@@multi_print@@" {
                        // Special handling for print with multiple arguments
                        // Print each argument with a space between, newline at end
                        for (i, arg) in args.iter().enumerate() {
                            if i > 0 {
                                // Print space between arguments
                                self.ir.push_str("  call void @roast_print_space()\n");
                            }
                            // Check the type of the argument to call the correct print function
                            let arg_ty = self.operand_type(arg);
                            let arg_val = self.generate_operand(arg)?;
                            let _ = self.fresh_value();
                            match arg_ty {
                                Type::Float | Type::Float32 | Type::Float64 => {
                                    self.ir.push_str(&format!("  call void @roast_print_float(double {})\n", arg_val));
                                }
                                Type::Bool => {
                                    // Bool is stored as i64, need to truncate to i1
                                    let bool_val = self.fresh_value();
                                    self.ir.push_str(&format!("  {} = trunc i64 {} to i1\n", bool_val, arg_val));
                                    self.ir.push_str(&format!("  call void @roast_print_bool(i1 {})\n", bool_val));
                                }
                                Type::Str => {
                                    // String is a RoastString pointer (i64) - convert back to pointer
                                    let ptr = self.fresh_value();
                                    self.ir.push_str(&format!("  {} = inttoptr i64 {} to i8*\n", ptr, arg_val));
                                    self.ir.push_str(&format!("  call void @roast_print_roast_str(i8* {})\n", ptr));
                                }
                                Type::Int | Type::Int32 | Type::Int64 => {
                                    // Use roast_print_int for integers (no newline)
                                    self.ir.push_str(&format!("  call void @roast_print_int(i64 {})\n", arg_val));
                                }
                                _ => {
                                    // Default to roast_print for other values (might add newline)
                                    self.ir.push_str(&format!("  call i64 @roast_print(i64 {})\n", arg_val));
                                }
                            }
                        }
                        // Print newline at end
                        self.ir.push_str("  call void @roast_print_newline()\n");
                        // Result is 0 (None)
                        self.ir.push_str(&format!("  {} = add i64 0, 0\n", result));
                    } else {
                        let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                        self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, args_str));
                    }
                } else {
                    if is_builtin_method {
                        // Even without '@', treat as builtin method with pointer signature
                        self.generate_builtin_method_call(&func_name, &arg_vals, &result)?;
                    } else {
                        // Indirect call through function pointer
                        let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                        let func_ty = format!("i64 ({})*", vec!["i64"; args.len()].join(", "));
                        let cast = self.fresh_value();
                        self.ir.push_str(&format!("  {} = inttoptr i64 {} to {}\n", cast, func_name, func_ty));
                        self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, cast, args_str));
                    }
                }

                // Store result only if destination has storage allocated (not void)
                if self.locals.contains_key(&destination.local) {
                    let ptr = self.get_place_ptr(destination);
                    self.ir.push_str(&format!("  store i64 {}, i64* {}\n", result, ptr));
                }

                // Jump to continuation
                if let Some(target) = target {
                    self.ir.push_str(&format!("  br label %bb{}\n", target));
                }
            }
            MirTerminator::MethodCall { receiver, receiver_class, receiver_type, method, args, destination, target } => {
                // Method dispatch - use static dispatch if receiver class is known
                let receiver_val = self.generate_operand_as_i64(receiver)?;

                let result = self.fresh_value();

                // First, check if this is a method call on a builtin type (list, dict, etc.)
                // Use receiver_type from MIR if available (more accurate), fall back to operand_type
                let actual_receiver_type = receiver_type.clone().unwrap_or_else(|| self.operand_type(receiver));
                let method_name = self.codegen.resolve_symbol(method.as_raw())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("method_{}", method.as_raw()));

                // Strip class prefix if present
                let method_name = if let Some(dot_pos) = method_name.rfind('.') {
                    method_name[dot_pos + 1..].to_string()
                } else {
                    method_name
                };

                // Check for list/dict builtin methods
                if let Some(collection_kind) = match &actual_receiver_type {
                    Type::List(_) => Some("list"),
                    Type::Dict(_, _) => Some("dict"),
                    Type::Set(_) => Some("set"),
                    Type::Str => Some("str"),
                    _ => None,
                } {
                    if let Some(runtime_fn) = get_builtin_method_by_kind(collection_kind, &method_name) {
                        // Call the builtin method
                        let receiver_ptr = self.i64_to_ptr(&receiver_val);

                        // Build args for the runtime function
                        let mut arg_vals: Vec<String> = vec![receiver_ptr.clone()];
                        for arg in args {
                            arg_vals.push(self.generate_operand_as_i64(arg)?);
                        }

                        // Get the return type and signature for the runtime function
                        let (return_type, call_str) = self.get_builtin_method_signature(runtime_fn, &arg_vals)?;

                        if return_type == "void" {
                            self.ir.push_str(&format!("  call {} {}({})\n", return_type, format!("@{}", runtime_fn), call_str));
                            // Store 0 as result for void methods
                            self.ir.push_str(&format!("  {} = add i64 0, 0\n", result));
                        } else if return_type == "i8*" {
                            let ptr_result = self.fresh_value();
                            self.ir.push_str(&format!("  {} = call {} {}({})\n", ptr_result, return_type, format!("@{}", runtime_fn), call_str));
                            // Convert pointer to i64
                            self.ir.push_str(&format!("  {} = ptrtoint i8* {} to i64\n", result, ptr_result));
                        } else if return_type == "i1" {
                            let bool_result = self.fresh_value();
                            self.ir.push_str(&format!("  {} = call {} {}({})\n", bool_result, return_type, format!("@{}", runtime_fn), call_str));
                            // Convert i1 to i64
                            self.ir.push_str(&format!("  {} = zext i1 {} to i64\n", result, bool_result));
                        } else {
                            self.ir.push_str(&format!("  {} = call {} {}({})\n", result, return_type, format!("@{}", runtime_fn), call_str));
                        }

                        // Store result
                        if self.locals.contains_key(&destination.local) {
                            let ptr = self.get_place_ptr(destination);
                            self.ir.push_str(&format!("  store i64 {}, i64* {}\n", result, ptr));
                        }

                        // Jump to continuation
                        if let Some(target) = target {
                            self.ir.push_str(&format!("  br label %bb{}\n", target));
                        }
                        return Ok(());
                    }
                }

                // Check if we can use static dispatch
                if let Some(class_name_or_id) = receiver_class {
                    // The class_name might be a symbol ID (from MIR type tracking) or actual class name
                    // Try to parse as symbol ID and resolve, otherwise use as-is
                    let class_name = if let Ok(sym_id) = class_name_or_id.parse::<u32>() {
                        self.codegen.resolve_symbol(sym_id)
                            .map(|s| s.to_string())
                            .unwrap_or(class_name_or_id.clone())
                    } else {
                        class_name_or_id.clone()
                    };

                    // Look up which class actually defines this method (inheritance)
                    // This walks up the MRO to find the class that has the method
                    let actual_class = self.codegen.find_method_class(&class_name, method.as_raw())
                        .unwrap_or_else(|| class_name.clone());

                    // Static dispatch: use the class that defines the method
                    let func_name = format!("@roast_fn_{}_{}", actual_class, method.as_raw());

                    // Build argument list: self followed by other args
                    let mut arg_vals = vec![receiver_val.clone()];
                    for arg in args {
                        arg_vals.push(self.generate_operand_as_i64(arg)?);
                    }
                    let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                    self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, args_str));
                } else {
                    // Dynamic method dispatch - use runtime method lookup
                    // Get method name as C string
                    // Note: resolve_symbol might return qualified name like "Child.__init__"
                    // For dynamic dispatch, we need just the method name part
                    let resolved = self.codegen.resolve_symbol(method.as_raw())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("method_{}", method.as_raw()));

                    // Strip class prefix if present (e.g., "Child.__init__" -> "__init__")
                    let method_name = if let Some(dot_pos) = resolved.rfind('.') {
                        resolved[dot_pos + 1..].to_string()
                    } else {
                        resolved
                    };

                    let method_global = self.codegen.add_string(&method_name);
                    let method_ptr = self.fresh_value();
                    self.ir.push_str(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                        method_ptr, method_name.len() + 1, method_name.len() + 1, method_global
                    ));

                    // Call appropriate runtime function based on argument count
                    match args.len() {
                        0 => {
                            self.ir.push_str(&format!(
                                "  {} = call i64 @roast_object_call_method0(i64 {}, i8* {})\n",
                                result, receiver_val, method_ptr
                            ));
                        }
                        1 => {
                            let arg1 = self.generate_operand_as_i64(&args[0])?;
                            self.ir.push_str(&format!(
                                "  {} = call i64 @roast_object_call_method1(i64 {}, i8* {}, i64 {})\n",
                                result, receiver_val, method_ptr, arg1
                            ));
                        }
                        _ => {
                            // Fall back to static dispatch with non-prefixed name
                            let func_name = format!("@roast_fn_{}", method.as_raw());
                            let mut arg_vals = vec![receiver_val.clone()];
                            for arg in args {
                                arg_vals.push(self.generate_operand_as_i64(arg)?);
                            }
                            let args_str = arg_vals.iter().map(|a| format!("i64 {}", a)).collect::<Vec<_>>().join(", ");
                            self.ir.push_str(&format!("  {} = call i64 {}({})\n", result, func_name, args_str));
                        }
                    }
                }

                // Store result
                if self.locals.contains_key(&destination.local) {
                    let ptr = self.get_place_ptr(destination);
                    self.ir.push_str(&format!("  store i64 {}, i64* {}\n", result, ptr));
                }

                // Jump to continuation
                if let Some(target) = target {
                    self.ir.push_str(&format!("  br label %bb{}\n", target));
                }
            }
            MirTerminator::ForIter { iter, loop_var, body, exit } => {
                // Get next value from iterator
                let iter_ptr = self.get_place_ptr(iter);
                let iter_val = self.fresh_value();
                self.ir.push_str(&format!("  {} = load i64, i64* {}\n", iter_val, iter_ptr));

                // Convert iterator to pointer
                let iter_i8ptr = self.i64_to_ptr(&iter_val);

                // Check if iterator is exhausted
                let done_ptr = self.fresh_value();
                self.ir.push_str(&format!("  {} = alloca i1\n", done_ptr));

                let next_val = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = call i64 @roast_iter_next(i8* {}, i1* {})\n",
                    next_val, iter_i8ptr, done_ptr
                ));

                let is_done = self.fresh_value();
                self.ir.push_str(&format!("  {} = load i1, i1* {}\n", is_done, done_ptr));

                // Store next value to loop variable
                let loop_var_ptr = self.locals.get(loop_var).cloned().unwrap_or_else(|| format!("%local{}", loop_var));
                self.ir.push_str(&format!("  store i64 {}, i64* {}\n", next_val, loop_var_ptr));

                // Branch based on iterator exhaustion
                self.ir.push_str(&format!("  br i1 {}, label %bb{}, label %bb{}\n", is_done, exit, body));
            }
            MirTerminator::Assert { cond, expected, target, msg } => {
                let cond_i64 = self.generate_operand(cond)?;

                // Truncate i64 to i1 for boolean operations
                let cond_val = self.fresh_value();
                self.ir.push_str(&format!("  {} = trunc i64 {} to i1\n", cond_val, cond_i64));

                // Check if condition matches expected
                let check = if *expected {
                    cond_val.clone()
                } else {
                    let neg = self.fresh_value();
                    self.ir.push_str(&format!("  {} = xor i1 {}, true\n", neg, cond_val));
                    neg
                };

                // Create blocks for pass and fail
                let fail_label = format!("assert_fail_{}", self.next_value);
                self.next_value += 1;

                self.ir.push_str(&format!("  br i1 {}, label %bb{}, label %{}\n", check, target, fail_label));

                // Fail block
                self.ir.push_str(&format!("{}:\n", fail_label));
                let msg_global = self.codegen.add_string(msg);
                let msg_ptr = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0\n",
                    msg_ptr, msg.len() + 1, msg.len() + 1, msg_global
                ));
                self.ir.push_str(&format!("  call void @roast_assert(i1 false, i8* {})\n", msg_ptr));
                self.ir.push_str("  unreachable\n");
            }
            MirTerminator::Drop { place, target, unwind: _ } => {
                // Drop the value (decrement refcount or free)
                let ptr = self.get_place_ptr(place);
                let val = self.fresh_value();
                self.ir.push_str(&format!("  {} = load i64, i64* {}\n", val, ptr));
                self.ir.push_str(&format!("  call void @roast_decref(i8* inttoptr (i64 {} to i8*))\n", val));
                self.ir.push_str(&format!("  br label %bb{}\n", target));
            }
            MirTerminator::TryBegin { body, handlers, finally, exit } => {
                // Full setjmp/longjmp exception handling
                // 1. Push exception frame and get jmp_buf pointer
                // 2. Call setjmp - returns 0 on setup, non-zero on longjmp
                // 3. Branch based on setjmp result

                // Get jmp_buf pointer from runtime
                let jmp_buf_ptr = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = call i8* @roast_exception_push_frame()\n",
                    jmp_buf_ptr
                ));

                // Call setjmp - returns 0 normally, 1 if longjmp'd back
                let setjmp_result = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = call i32 @setjmp(i8* {})\n",
                    setjmp_result, jmp_buf_ptr
                ));

                // Check if this is normal execution (0) or exception (non-zero)
                let is_exc = self.fresh_value();
                self.ir.push_str(&format!(
                    "  {} = icmp ne i32 {}, 0\n",
                    is_exc, setjmp_result
                ));

                // Create dispatch label if there are handlers with type filters
                // For now, simplified: just go to first handler (bare except or first typed)
                let first_handler = handlers.first().map(|h| h.body).unwrap_or(*exit);
                self.ir.push_str(&format!(
                    "  br i1 {}, label %bb{}, label %bb{}\n",
                    is_exc, first_handler, body
                ));

                // Note: Exception type matching and handler dispatch happens in the handler blocks
                // Each handler block should check roast_exception_matches and branch accordingly
            }
            MirTerminator::Raise { exc } => {
                // Raise an exception - will longjmp back to setjmp point
                if let Some(exc_op) = exc {
                    let exc_val = self.generate_operand(exc_op)?;
                    self.ir.push_str(&format!("  call void @roast_raise(i64 {})\n", exc_val));
                } else {
                    // Re-raise current exception
                    self.ir.push_str("  call void @roast_reraise()\n");
                }
                self.ir.push_str("  unreachable\n");
            }
            MirTerminator::Unreachable => {
                self.ir.push_str("  unreachable\n");
            }
        }
        Ok(())
    }

    fn get_place_ptr(&self, place: &MirPlace) -> String {
        self.locals.get(&place.local).cloned().unwrap_or_else(|| format!("%local{}", place.local))
    }

    fn get_place_type(&self, place: &MirPlace) -> Type {
        // Find the type from locals or params
        for local in &self.body.locals {
            if local.id == place.local {
                return local.ty.clone();
            }
        }
        for param in &self.body.params {
            if param.local.id == place.local {
                return param.local.ty.clone();
            }
        }
        Type::Int
    }
}
