//! Variable inspection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

/// Variable reference.
pub type VariableRef = i64;

/// Variable type.
#[derive(Debug, Clone)]
pub enum VariableValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<VariableValue>),
    Dict(HashMap<String, VariableValue>),
    Object { type_name: String, fields: HashMap<String, VariableValue> },
    None,
    Bytes(Vec<u8>),
    Reference(VariableRef),
}

impl VariableValue {
    /// Get display value.
    pub fn display(&self) -> String {
        match self {
            VariableValue::Int(i) => i.to_string(),
            VariableValue::Float(f) => format!("{:.6}", f),
            VariableValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            VariableValue::String(s) => format!("\"{}\"", s),
            VariableValue::List(items) => {
                let parts: Vec<_> = items.iter().take(3).map(|v| v.display()).collect();
                if items.len() > 3 {
                    format!("[{}, ...]", parts.join(", "))
                } else {
                    format!("[{}]", parts.join(", "))
                }
            }
            VariableValue::Dict(map) => {
                format!("{{...}} ({} items)", map.len())
            }
            VariableValue::Object { type_name, fields: _ } => {
                format!("<{} object>", type_name)
            }
            VariableValue::None => "None".to_string(),
            VariableValue::Bytes(b) => format!("b'...' ({} bytes)", b.len()),
            VariableValue::Reference(r) => format!("<ref {}>", r),
        }
    }
    
    /// Get type name.
    pub fn type_name(&self) -> &str {
        match self {
            VariableValue::Int(_) => "int",
            VariableValue::Float(_) => "float",
            VariableValue::Bool(_) => "bool",
            VariableValue::String(_) => "str",
            VariableValue::List(_) => "list",
            VariableValue::Dict(_) => "dict",
            VariableValue::Object { type_name, .. } => type_name,
            VariableValue::None => "NoneType",
            VariableValue::Bytes(_) => "bytes",
            VariableValue::Reference(_) => "reference",
        }
    }
    
    /// Check if value has children.
    pub fn has_children(&self) -> bool {
        matches!(
            self,
            VariableValue::List(_) | VariableValue::Dict(_) | VariableValue::Object { .. }
        )
    }
    
    /// Get children count.
    pub fn children_count(&self) -> usize {
        match self {
            VariableValue::List(items) => items.len(),
            VariableValue::Dict(map) => map.len(),
            VariableValue::Object { fields, .. } => fields.len(),
            _ => 0,
        }
    }
}

/// Variable.
#[derive(Debug, Clone)]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// Variable value.
    pub value: VariableValue,
    /// Reference for expanding (0 if not expandable).
    pub reference: VariableRef,
}

/// Scope type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeType {
    Local,
    Global,
    Closure,
}

impl ScopeType {
    pub fn name(&self) -> &str {
        match self {
            ScopeType::Local => "Locals",
            ScopeType::Global => "Globals",
            ScopeType::Closure => "Closure",
        }
    }
}

/// Variable scope.
#[derive(Debug, Clone)]
pub struct Scope {
    /// Scope type.
    pub scope_type: ScopeType,
    /// Variables reference for this scope.
    pub reference: VariableRef,
    /// Variables in this scope.
    pub variables: Vec<Variable>,
}

/// Variable store.
pub struct VariableStore {
    /// Next reference ID.
    next_ref: AtomicI64,
    /// Scopes by frame ID.
    scopes: HashMap<i64, Vec<Scope>>,
    /// Expanded variable values.
    expanded: HashMap<VariableRef, Vec<Variable>>,
}

impl VariableStore {
    /// Create a new store.
    pub fn new() -> Self {
        Self {
            next_ref: AtomicI64::new(1000), // Start above frame IDs
            scopes: HashMap::new(),
            expanded: HashMap::new(),
        }
    }
    
    /// Allocate a new reference.
    pub fn alloc_ref(&self) -> VariableRef {
        self.next_ref.fetch_add(1, Ordering::SeqCst)
    }
    
    /// Set scopes for a frame.
    pub fn set_scopes(&mut self, frame_id: i64, scopes: Vec<Scope>) {
        self.scopes.insert(frame_id, scopes);
    }
    
    /// Get scopes for a frame.
    pub fn get_scopes(&self, frame_id: i64) -> Option<&Vec<Scope>> {
        self.scopes.get(&frame_id)
    }
    
    /// Set variables for a reference.
    pub fn set_variables(&mut self, reference: VariableRef, variables: Vec<Variable>) {
        self.expanded.insert(reference, variables);
    }
    
    /// Get variables for a reference.
    pub fn get_variables(&self, reference: VariableRef) -> Option<&Vec<Variable>> {
        // First check expanded
        if let Some(vars) = self.expanded.get(&reference) {
            return Some(vars);
        }
        
        // Then check scopes
        for scopes in self.scopes.values() {
            for scope in scopes {
                if scope.reference == reference {
                    return Some(&scope.variables);
                }
            }
        }
        
        None
    }
    
    /// Clear all data.
    pub fn clear(&mut self) {
        self.scopes.clear();
        self.expanded.clear();
    }
}

impl Default for VariableStore {
    fn default() -> Self {
        Self::new()
    }
}

