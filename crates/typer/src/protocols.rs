//! Protocol (Trait) System for Roast.
//!
//! Protocols define behavior contracts that types can implement.
//! Similar to Python's protocols and Rust's traits.

use crate::types::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;

// =============================================================================
// Protocol Definition
// =============================================================================

/// A protocol definition (like a trait in Rust).
#[derive(Clone, Debug)]
pub struct Protocol {
    /// Protocol name.
    pub name: String,
    /// Type parameters.
    pub type_params: Vec<TypeVar>,
    /// Required methods.
    pub methods: Vec<ProtocolMethod>,
    /// Required associated types.
    pub associated_types: Vec<AssociatedType>,
    /// Super-protocols (inheritance).
    pub super_protocols: Vec<ProtocolRef>,
    /// Default method implementations.
    pub default_impls: FxHashMap<String, MethodImpl>,
    /// Documentation.
    pub doc: Option<String>,
    /// Is this a marker protocol (no methods)?
    pub is_marker: bool,
}

/// Reference to a protocol.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProtocolRef {
    pub name: String,
    pub type_args: Vec<Type>,
}

/// A method in a protocol.
#[derive(Clone, Debug)]
pub struct ProtocolMethod {
    pub name: String,
    pub params: Vec<FuncParam>,
    pub returns: Type,
    pub is_async: bool,
    pub is_static: bool,
    pub is_classmethod: bool,
    pub doc: Option<String>,
}

/// Associated type in a protocol.
#[derive(Clone, Debug)]
pub struct AssociatedType {
    pub name: String,
    pub bound: Option<ProtocolRef>,
    pub default: Option<Type>,
}

/// A method implementation.
#[derive(Clone, Debug)]
pub struct MethodImpl {
    pub body: MethodBody,
}

/// Method body representation.
#[derive(Clone, Debug)]
pub enum MethodBody {
    /// Abstract (no implementation).
    Abstract,
    /// Default implementation (stored as AST or placeholder).
    Default(String),
    /// Built-in implementation.
    Builtin(String),
}

impl Protocol {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            type_params: Vec::new(),
            methods: Vec::new(),
            associated_types: Vec::new(),
            super_protocols: Vec::new(),
            default_impls: FxHashMap::default(),
            doc: None,
            is_marker: false,
        }
    }
    
    pub fn marker(name: &str) -> Self {
        Self {
            is_marker: true,
            ..Self::new(name)
        }
    }
    
    pub fn with_type_param(mut self, param: TypeVar) -> Self {
        self.type_params.push(param);
        self
    }
    
    pub fn with_method(mut self, method: ProtocolMethod) -> Self {
        self.methods.push(method);
        self
    }
    
    pub fn with_super(mut self, proto: ProtocolRef) -> Self {
        self.super_protocols.push(proto);
        self
    }
    
    pub fn with_doc(mut self, doc: &str) -> Self {
        self.doc = Some(doc.to_string());
        self
    }
    
    /// Check if this protocol has a method.
    pub fn has_method(&self, name: &str) -> bool {
        self.methods.iter().any(|m| m.name == name)
    }
    
    /// Get a method by name.
    pub fn get_method(&self, name: &str) -> Option<&ProtocolMethod> {
        self.methods.iter().find(|m| m.name == name)
    }
    
    /// Get all required method names (including super-protocols).
    pub fn all_required_methods(&self, registry: &ProtocolRegistry) -> Vec<String> {
        let mut methods = Vec::new();
        
        // Add own methods
        for method in &self.methods {
            if !self.default_impls.contains_key(&method.name) {
                methods.push(method.name.clone());
            }
        }
        
        // Add super-protocol methods
        for super_ref in &self.super_protocols {
            if let Some(super_proto) = registry.get(&super_ref.name) {
                methods.extend(super_proto.all_required_methods(registry));
            }
        }
        
        methods
    }
}

impl ProtocolMethod {
    pub fn new(name: &str, params: Vec<FuncParam>, returns: Type) -> Self {
        Self {
            name: name.to_string(),
            params,
            returns,
            is_async: false,
            is_static: false,
            is_classmethod: false,
            doc: None,
        }
    }
    
    pub fn async_method(mut self) -> Self {
        self.is_async = true;
        self
    }
    
    pub fn static_method(mut self) -> Self {
        self.is_static = true;
        self
    }
}

// =============================================================================
// Protocol Implementation
// =============================================================================

/// An implementation of a protocol for a type.
#[derive(Clone, Debug)]
pub struct ProtocolImpl {
    /// The type implementing the protocol.
    pub implementing_type: Type,
    /// The protocol being implemented.
    pub protocol: ProtocolRef,
    /// Method implementations.
    pub methods: FxHashMap<String, Type>,
    /// Associated type bindings.
    pub associated_types: FxHashMap<String, Type>,
    /// Where clause constraints.
    pub where_clause: Vec<WhereClause>,
}

/// A where clause constraint.
#[derive(Clone, Debug)]
pub struct WhereClause {
    pub ty: Type,
    pub bounds: Vec<ProtocolRef>,
}

impl ProtocolImpl {
    pub fn new(ty: Type, protocol: ProtocolRef) -> Self {
        Self {
            implementing_type: ty,
            protocol,
            methods: FxHashMap::default(),
            associated_types: FxHashMap::default(),
            where_clause: Vec::new(),
        }
    }
    
    pub fn with_method(mut self, name: &str, ty: Type) -> Self {
        self.methods.insert(name.to_string(), ty);
        self
    }
    
    pub fn with_associated_type(mut self, name: &str, ty: Type) -> Self {
        self.associated_types.insert(name.to_string(), ty);
        self
    }
}

// =============================================================================
// Protocol Registry
// =============================================================================

/// Registry of all protocols.
#[derive(Clone, Debug, Default)]
pub struct ProtocolRegistry {
    /// All registered protocols.
    protocols: FxHashMap<String, Protocol>,
    /// Implementations: (Type, Protocol) -> Impl.
    implementations: Vec<ProtocolImpl>,
    /// Cache of protocol checks.
    cache: FxHashMap<(String, String), bool>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register_builtins();
        registry
    }
    
    /// Register built-in protocols.
    fn register_builtins(&mut self) {
        // Copy protocol
        let copy = Protocol::marker("Copy")
            .with_doc("Types that can be copied bitwise.");
        self.register(copy);
        
        // Clone protocol
        let clone = Protocol::new("Clone")
            .with_method(ProtocolMethod::new(
                "__clone__",
                vec![],  // self is implicit
                Type::Var(TypeVar { id: 0, name: None, bound: None }), // Self
            ))
            .with_doc("Types that can be explicitly cloned.");
        self.register(clone);
        
        // Iterator protocol
        let iterator = Protocol::new("Iterator")
            .with_type_param(TypeVar { id: 0, name: None, bound: None })
            .with_method(ProtocolMethod::new(
                "__next__",
                vec![],
                Type::Optional(Arc::new(Type::Var(TypeVar { id: 0, name: None, bound: None }))),
            ))
            .with_doc("Types that can produce a sequence of values.");
        self.register(iterator);
        
        // Iterable protocol
        let iterable = Protocol::new("Iterable")
            .with_type_param(TypeVar { id: 0, name: None, bound: None })
            .with_method(ProtocolMethod::new(
                "__iter__",
                vec![],
                Type::Protocol(ProtocolType {
                    name: "Iterator".to_string(),
                    members: vec![],
                    methods: vec![],
                }),
            ))
            .with_doc("Types that can be iterated over.");
        self.register(iterable);
        
        // Hashable protocol
        let hashable = Protocol::new("Hashable")
            .with_method(ProtocolMethod::new(
                "__hash__",
                vec![],
                Type::Int,
            ))
            .with_doc("Types that can be hashed.");
        self.register(hashable);
        
        // Eq protocol
        let eq = Protocol::new("Eq")
            .with_method(ProtocolMethod::new(
                "__eq__",
                vec![FuncParam {
                    name: None,
                    ty: Type::Any,
                    default: false,
                    kind: ParamKind::Regular,
                }],
                Type::Bool,
            ))
            .with_doc("Types that support equality comparison.");
        self.register(eq);
        
        // Ord protocol
        let ord = Protocol::new("Ord")
            .with_super(ProtocolRef { name: "Eq".to_string(), type_args: vec![] })
            .with_method(ProtocolMethod::new(
                "__lt__",
                vec![FuncParam {
                    name: None,
                    ty: Type::Any,
                    default: false,
                    kind: ParamKind::Regular,
                }],
                Type::Bool,
            ))
            .with_method(ProtocolMethod::new(
                "__le__",
                vec![FuncParam {
                    name: None,
                    ty: Type::Any,
                    default: false,
                    kind: ParamKind::Regular,
                }],
                Type::Bool,
            ))
            .with_doc("Types that support ordering comparison.");
        self.register(ord);
        
        // Add protocol
        let add = Protocol::new("Add")
            .with_type_param(TypeVar { id: 0, name: None, bound: None })
            .with_type_param(TypeVar { id: 1, name: None, bound: None })
            .with_method(ProtocolMethod::new(
                "__add__",
                vec![FuncParam {
                    name: None,
                    ty: Type::Var(TypeVar { id: 0, name: None, bound: None }),
                    default: false,
                    kind: ParamKind::Regular,
                }],
                Type::Var(TypeVar { id: 1, name: None, bound: None }),
            ))
            .with_doc("Types that support the + operator.");
        self.register(add);
        
        // Context manager protocol
        let context_manager = Protocol::new("ContextManager")
            .with_method(ProtocolMethod::new(
                "__enter__",
                vec![],
                Type::Any,
            ))
            .with_method(ProtocolMethod::new(
                "__exit__",
                vec![
                    FuncParam {
                        name: None,
                        ty: Type::Optional(Arc::new(Type::Any)),
                        default: false,
                        kind: ParamKind::Regular,
                    },
                    FuncParam {
                        name: None,
                        ty: Type::Optional(Arc::new(Type::Any)),
                        default: false,
                        kind: ParamKind::Regular,
                    },
                    FuncParam {
                        name: None,
                        ty: Type::Optional(Arc::new(Type::Any)),
                        default: false,
                        kind: ParamKind::Regular,
                    },
                ],
                Type::Optional(Arc::new(Type::Bool)),
            ))
            .with_doc("Types that can be used with the 'with' statement.");
        self.register(context_manager);
        
        // Callable protocol
        let callable = Protocol::new("Callable")
            .with_method(ProtocolMethod::new(
                "__call__",
                vec![], // variadic
                Type::Any,
            ))
            .with_doc("Types that can be called as functions.");
        self.register(callable);
        
        // Send marker
        let send = Protocol::marker("Send")
            .with_doc("Types that can be safely sent between threads.");
        self.register(send);
        
        // Sync marker
        let sync = Protocol::marker("Sync")
            .with_doc("Types that can be safely shared between threads.");
        self.register(sync);
        
        // Sized marker
        let sized = Protocol::marker("Sized")
            .with_doc("Types with a known size at compile time.");
        self.register(sized);
    }
    
    /// Register a protocol.
    pub fn register(&mut self, protocol: Protocol) {
        self.protocols.insert(protocol.name.clone(), protocol);
    }
    
    /// Get a protocol by name.
    pub fn get(&self, name: &str) -> Option<&Protocol> {
        self.protocols.get(name)
    }
    
    /// Get all protocol names.
    pub fn protocol_names(&self) -> impl Iterator<Item = &str> {
        self.protocols.keys().map(|s| s.as_str())
    }
    
    /// Register an implementation.
    pub fn register_impl(&mut self, impl_: ProtocolImpl) {
        self.implementations.push(impl_);
        self.cache.clear();
    }
    
    /// Check if a type implements a protocol.
    pub fn implements(&self, ty: &Type, protocol: &str) -> bool {
        let ty_key = format!("{:?}", ty);
        let cache_key = (ty_key.clone(), protocol.to_string());
        
        if let Some(&result) = self.cache.get(&cache_key) {
            return result;
        }
        
        // Check explicit implementations
        for impl_ in &self.implementations {
            if self.types_match(&impl_.implementing_type, ty) 
                && impl_.protocol.name == protocol 
            {
                return true;
            }
        }
        
        // Check built-in implementations
        self.has_builtin_impl(ty, protocol)
    }
    
    /// Check if two types match (simplified).
    fn types_match(&self, pattern: &Type, concrete: &Type) -> bool {
        match (pattern, concrete) {
            (Type::Any, _) => true,
            (Type::Var(_), _) => true, // Type variable matches anything
            (Type::Int, Type::Int) => true,
            (Type::Float, Type::Float) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Str, Type::Str) => true,
            (Type::List(a), Type::List(b)) => self.types_match(a, b),
            (Type::Dict(k1, v1), Type::Dict(k2, v2)) => {
                self.types_match(k1, k2) && self.types_match(v1, v2)
            }
            (Type::Optional(a), Type::Optional(b)) => self.types_match(a, b),
            _ => pattern == concrete,
        }
    }
    
    /// Check for built-in protocol implementations.
    fn has_builtin_impl(&self, ty: &Type, protocol: &str) -> bool {
        match protocol {
            "Copy" => matches!(ty, 
                Type::Bool | Type::Int | Type::Int8 | Type::Int16 | Type::Int32 |
                Type::Int64 | Type::Int128 | Type::UInt | Type::UInt8 | Type::UInt16 |
                Type::UInt32 | Type::UInt64 | Type::UInt128 | Type::Float |
                Type::Float32 | Type::Float64 | Type::NoneType
            ),
            "Clone" => matches!(ty,
                Type::Bool | Type::Int | Type::Float | Type::Str | Type::Bytes |
                Type::List(_) | Type::Dict(_, _) | Type::Set(_) | Type::Tuple(_) |
                Type::NoneType | Type::Optional(_)
            ),
            "Hashable" => matches!(ty,
                Type::Bool | Type::Int | Type::Float | Type::Str | Type::Bytes |
                Type::NoneType | Type::Tuple(_)
            ),
            "Eq" => !matches!(ty, Type::Float | Type::Float32 | Type::Float64),
            "Iterable" => matches!(ty,
                Type::List(_) | Type::Dict(_, _) | Type::Set(_) | Type::Str | 
                Type::Bytes | Type::Tuple(_)
            ),
            "Sized" => !matches!(ty, Type::Any | Type::Unknown),
            "Send" | "Sync" => {
                // Most types are Send and Sync unless they contain raw pointers
                !matches!(ty, Type::Any)
            }
            _ => false,
        }
    }
    
    /// Get all protocols a type implements.
    pub fn implemented_protocols(&self, ty: &Type) -> Vec<String> {
        let mut result = Vec::new();
        
        for name in self.protocols.keys() {
            if self.implements(ty, name) {
                result.push(name.clone());
            }
        }
        
        result
    }
    
    /// Get methods available from protocols for a type.
    pub fn protocol_methods(&self, ty: &Type) -> Vec<(&str, &ProtocolMethod)> {
        let mut methods = Vec::new();
        
        for (name, proto) in &self.protocols {
            if self.implements(ty, name) {
                for method in &proto.methods {
                    methods.push((name.as_str(), method));
                }
            }
        }
        
        methods
    }
}

// =============================================================================
// Derive Macro Support
// =============================================================================

/// Protocols that can be automatically derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivableProtocol {
    Copy,
    Clone,
    Eq,
    Ord,
    Hash,
    Default,
    Debug,
}

impl DerivableProtocol {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Copy" => Some(DerivableProtocol::Copy),
            "Clone" => Some(DerivableProtocol::Clone),
            "Eq" => Some(DerivableProtocol::Eq),
            "Ord" => Some(DerivableProtocol::Ord),
            "Hash" => Some(DerivableProtocol::Hash),
            "Default" => Some(DerivableProtocol::Default),
            "Debug" => Some(DerivableProtocol::Debug),
            _ => None,
        }
    }
    
    pub fn name(&self) -> &'static str {
        match self {
            DerivableProtocol::Copy => "Copy",
            DerivableProtocol::Clone => "Clone",
            DerivableProtocol::Eq => "Eq",
            DerivableProtocol::Ord => "Ord",
            DerivableProtocol::Hash => "Hash",
            DerivableProtocol::Default => "Default",
            DerivableProtocol::Debug => "Debug",
        }
    }
}

/// Check if a protocol can be derived for a class.
pub fn can_derive(
    protocol: DerivableProtocol,
    class: &ClassType,
    registry: &ProtocolRegistry,
) -> bool {
    match protocol {
        DerivableProtocol::Copy => {
            // All fields must be Copy
            class.members.iter().all(|(_, ty)| registry.implements(ty, "Copy"))
        }
        DerivableProtocol::Clone => {
            // All fields must be Clone
            class.members.iter().all(|(_, ty)| registry.implements(ty, "Clone"))
        }
        DerivableProtocol::Eq => {
            // All fields must be Eq
            class.members.iter().all(|(_, ty)| registry.implements(ty, "Eq"))
        }
        DerivableProtocol::Ord => {
            // All fields must be Ord
            class.members.iter().all(|(_, ty)| registry.implements(ty, "Ord"))
        }
        DerivableProtocol::Hash => {
            // All fields must be Hashable
            class.members.iter().all(|(_, ty)| registry.implements(ty, "Hashable"))
        }
        DerivableProtocol::Default => {
            // All fields must have defaults or be Default
            class.members.iter().all(|(_, ty)| {
                matches!(ty, Type::Optional(_)) || registry.implements(ty, "Default")
            })
        }
        DerivableProtocol::Debug => {
            // Always derivable (we can always print something)
            true
        }
    }
}

// =============================================================================
// Protocol Bounds
// =============================================================================

/// A bound on a type parameter.
#[derive(Clone, Debug)]
pub enum TypeBound {
    /// Must implement a protocol.
    Protocol(ProtocolRef),
    /// Must be a subtype of.
    Subtype(Type),
}

/// Check if a type satisfies a bound.
pub fn satisfies_bound(ty: &Type, bound: &TypeBound, registry: &ProtocolRegistry) -> bool {
    match bound {
        TypeBound::Protocol(proto_ref) => {
            registry.implements(ty, &proto_ref.name)
        }
        TypeBound::Subtype(super_type) => {
            // Simplified subtype check
            ty == super_type || matches!(super_type, Type::Any)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_protocol_registry() {
        let registry = ProtocolRegistry::new();
        
        // Check built-in protocols exist
        assert!(registry.get("Copy").is_some());
        assert!(registry.get("Clone").is_some());
        assert!(registry.get("Iterator").is_some());
        assert!(registry.get("Iterable").is_some());
    }
    
    #[test]
    fn test_builtin_impls() {
        let registry = ProtocolRegistry::new();
        
        // Primitives are Copy
        assert!(registry.implements(&Type::Int, "Copy"));
        assert!(registry.implements(&Type::Bool, "Copy"));
        assert!(registry.implements(&Type::Float, "Copy"));
        
        // Strings are Clone but not Copy
        assert!(!registry.implements(&Type::Str, "Copy"));
        assert!(registry.implements(&Type::Str, "Clone"));
        
        // Lists are Iterable
        assert!(registry.implements(&Type::List(Arc::new(Type::Int)), "Iterable"));
    }
    
    #[test]
    fn test_implemented_protocols() {
        let registry = ProtocolRegistry::new();
        
        let impls = registry.implemented_protocols(&Type::Int);
        assert!(impls.contains(&"Copy".to_string()));
        assert!(impls.contains(&"Clone".to_string()));
        assert!(impls.contains(&"Hashable".to_string()));
    }
    
    #[test]
    fn test_custom_protocol() {
        let mut registry = ProtocolRegistry::new();
        
        let printable = Protocol::new("Printable")
            .with_method(ProtocolMethod::new("__str__", vec![], Type::Str));
        
        registry.register(printable);
        assert!(registry.get("Printable").is_some());
    }
}

