//! Macro system for Roast.
//!
//! Provides decorator-based macros and code transformation.

use crate::{Decorator, Expr, ExprKind, Ident, Keyword};
use std::collections::HashMap;

// =============================================================================
// Built-in Decorators
// =============================================================================

/// Built-in decorator types.
#[derive(Clone, Debug)]
pub enum BuiltinDecorator {
    /// @staticmethod
    StaticMethod,
    /// @classmethod
    ClassMethod,
    /// @property
    Property,
    /// @abstractmethod
    AbstractMethod,
    /// @dataclass
    Dataclass(DataclassOptions),
    /// @derive(Protocol1, Protocol2, ...)
    Derive(Vec<String>),
    /// @deprecated(reason="...")
    Deprecated(Option<String>),
    /// @override
    Override,
    /// @final
    Final,
    /// @cached_property
    CachedProperty,
    /// @functools.lru_cache
    LruCache(Option<usize>),
    /// @test
    Test(TestOptions),
    /// @benchmark
    Benchmark,
    /// Unknown/custom decorator
    Custom(String, Vec<Expr>, Vec<Keyword>),
}

/// Options for @dataclass.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DataclassOptions {
    pub init: bool,
    pub repr: bool,
    pub eq: bool,
    pub order: bool,
    pub frozen: bool,
    pub slots: bool,
    pub kw_only: bool,
}

impl DataclassOptions {
    pub fn new() -> Self {
        Self {
            init: true,
            repr: true,
            eq: true,
            order: false,
            frozen: false,
            slots: false,
            kw_only: false,
        }
    }
}

/// Options for @test.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TestOptions {
    pub expected_failure: bool,
    pub skip: bool,
    pub skip_reason: Option<String>,
    pub timeout: Option<u64>,
}

// =============================================================================
// Decorator Classification
// =============================================================================

/// Get decorator name as string.
fn get_decorator_name(decorator: &Decorator) -> String {
    match &decorator.name.kind {
        ExprKind::Name { id, .. } => id.name.as_raw().to_string(),
        ExprKind::Attribute { value, attr, .. } => {
            if let ExprKind::Name { id, .. } = &value.kind {
                format!("{}.{}", id.name.as_raw(), attr.name.as_raw())
            } else {
                format!("{:?}", decorator.name)
            }
        }
        _ => format!("{:?}", decorator.name),
    }
}

fn keyword_to_bool(expr: &Expr) -> Option<bool> {
    if let ExprKind::BoolLit { value } = &expr.kind {
        return Some(*value);
    }
    None
}

fn expr_to_string(expr: &Expr) -> Option<String> {
    if let ExprKind::StringLit { value, .. } = &expr.kind {
        return Some(value.clone());
    }
    None
}

/// Classify a decorator.
pub fn classify_decorator(decorator: &Decorator) -> BuiltinDecorator {
    let name = get_decorator_name(decorator);
    
    match name.as_str() {
        "staticmethod" => BuiltinDecorator::StaticMethod,
        "classmethod" => BuiltinDecorator::ClassMethod,
        "property" => BuiltinDecorator::Property,
        "abstractmethod" => BuiltinDecorator::AbstractMethod,
        "override" => BuiltinDecorator::Override,
        "final" => BuiltinDecorator::Final,
        "cached_property" => BuiltinDecorator::CachedProperty,
        
        "dataclass" => {
            let mut opts = DataclassOptions::new();
            for kw in &decorator.keywords {
                if let Some(ref kw_name) = kw.name {
                    let val = keyword_to_bool(&kw.value);
                    let name_str = kw_name.name.as_raw().to_string();
                    match name_str.as_str() {
                        "init" => opts.init = val.unwrap_or(true),
                        "repr" => opts.repr = val.unwrap_or(true),
                        "eq" => opts.eq = val.unwrap_or(true),
                        "order" => opts.order = val.unwrap_or(false),
                        "frozen" => opts.frozen = val.unwrap_or(false),
                        "slots" => opts.slots = val.unwrap_or(false),
                        "kw_only" => opts.kw_only = val.unwrap_or(false),
                        _ => {}
                    }
                }
            }
            BuiltinDecorator::Dataclass(opts)
        }
        
        "derive" => {
            let protocols: Vec<String> = decorator.arguments.iter()
                .filter_map(|arg| {
                    if let ExprKind::Name { id, .. } = &arg.kind {
                        Some(id.name.as_raw().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            BuiltinDecorator::Derive(protocols)
        }
        
        "deprecated" => {
            let reason = decorator.keywords.iter()
                .find(|kw| {
                    kw.name.as_ref()
                        .map(|n| n.name.as_raw().to_string() == "reason")
                        .unwrap_or(false)
                })
                .and_then(|kw| expr_to_string(&kw.value));
            BuiltinDecorator::Deprecated(reason)
        }
        
        "test" => {
            let mut opts = TestOptions::default();
            for kw in &decorator.keywords {
                if let Some(ref kw_name) = kw.name {
                    let name_str = kw_name.name.as_raw().to_string();
                    match name_str.as_str() {
                        "expected_failure" => {
                            opts.expected_failure = keyword_to_bool(&kw.value).unwrap_or(false)
                        }
                        "skip" => {
                            opts.skip = keyword_to_bool(&kw.value).unwrap_or(false)
                        }
                        "skip_reason" => {
                            opts.skip_reason = expr_to_string(&kw.value)
                        }
                        _ => {}
                    }
                }
            }
            BuiltinDecorator::Test(opts)
        }
        
        "benchmark" => BuiltinDecorator::Benchmark,
        
        "functools.lru_cache" | "lru_cache" => {
            let maxsize = decorator.arguments.first()
                .and_then(|arg| {
                    if let ExprKind::IntLit { value } = &arg.kind {
                        use std::convert::TryInto;
                        value.try_into().ok()
                    } else {
                        None
                    }
                });
            BuiltinDecorator::LruCache(maxsize)
        }
        
        _ => BuiltinDecorator::Custom(
            name,
            decorator.arguments.clone(),
            decorator.keywords.clone(),
        ),
    }
}

// =============================================================================
// Declarative Macros
// =============================================================================

/// A declarative macro definition.
#[derive(Clone, Debug)]
pub struct DeclarativeMacro {
    pub name: String,
    pub rules: Vec<MacroRule>,
}

/// A single macro rule.
#[derive(Clone, Debug)]
pub struct MacroRule {
    pub pattern: MacroPattern,
    pub template: MacroTemplate,
}

/// Pattern for matching in declarative macros.
#[derive(Clone, Debug)]
pub enum MacroPattern {
    /// Match a literal token.
    Literal(String),
    /// Match an identifier: $name:ident
    Ident(String),
    /// Match an expression: $name:expr
    Expr(String),
    /// Match a type: $name:ty
    Type(String),
    /// Match a statement: $name:stmt
    Stmt(String),
    /// Match a block: $name:block
    Block(String),
    /// Repetition: $($pat),*
    Repeat {
        pattern: Box<MacroPattern>,
        separator: Option<String>,
        kind: RepeatKind,
    },
    /// Sequence of patterns.
    Sequence(Vec<MacroPattern>),
}

/// Repetition kind.
#[derive(Clone, Copy, Debug)]
pub enum RepeatKind {
    /// Zero or more: *
    ZeroOrMore,
    /// One or more: +
    OneOrMore,
    /// Zero or one: ?
    ZeroOrOne,
}

/// Template for expansion.
#[derive(Clone, Debug)]
pub enum MacroTemplate {
    /// Literal code.
    Literal(String),
    /// Substitution: $name
    Substitute(String),
    /// Repetition: $($template),*
    Repeat {
        template: Box<MacroTemplate>,
        separator: Option<String>,
    },
    /// Sequence of templates.
    Sequence(Vec<MacroTemplate>),
}

// =============================================================================
// Macro Registry
// =============================================================================

/// Registry for macros.
#[derive(Default)]
pub struct MacroRegistry {
    macros: HashMap<String, DeclarativeMacro>,
}

impl MacroRegistry {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
        }
    }
    
    /// Register a macro.
    pub fn register(&mut self, mac: DeclarativeMacro) {
        self.macros.insert(mac.name.clone(), mac);
    }
    
    /// Get a macro by name.
    pub fn get(&self, name: &str) -> Option<&DeclarativeMacro> {
        self.macros.get(name)
    }
    
    /// Check if a macro exists.
    pub fn contains(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }
}

// =============================================================================
// Macro Helpers
// =============================================================================

/// Check if a decorator is a built-in.
pub fn is_builtin_decorator(name: &str) -> bool {
    matches!(
        name,
        "staticmethod" | "classmethod" | "property" | "abstractmethod" |
        "override" | "final" | "cached_property" | "dataclass" | "derive" |
        "deprecated" | "test" | "benchmark" | "lru_cache" | "functools.lru_cache"
    )
}

/// Get all decorators from a list that match a specific type.
pub fn filter_decorators<'a>(decorators: &'a [Decorator], kind: &str) -> Vec<&'a Decorator> {
    decorators.iter()
        .filter(|d| get_decorator_name(d) == kind)
        .collect()
}

/// Check if any decorator marks a method as static.
pub fn is_static_method(decorators: &[Decorator]) -> bool {
    decorators.iter().any(|d| {
        matches!(classify_decorator(d), BuiltinDecorator::StaticMethod)
    })
}

/// Check if any decorator marks a method as a class method.
pub fn is_class_method(decorators: &[Decorator]) -> bool {
    decorators.iter().any(|d| {
        matches!(classify_decorator(d), BuiltinDecorator::ClassMethod)
    })
}

/// Check if any decorator marks a function as a property.
pub fn is_property(decorators: &[Decorator]) -> bool {
    decorators.iter().any(|d| {
        matches!(classify_decorator(d), BuiltinDecorator::Property)
    })
}

/// Check if class has dataclass decorator.
pub fn has_dataclass(decorators: &[Decorator]) -> Option<DataclassOptions> {
    for d in decorators {
        if let BuiltinDecorator::Dataclass(opts) = classify_decorator(d) {
            return Some(opts);
        }
    }
    None
}

/// Get derive protocols from decorators.
pub fn get_derive_protocols(decorators: &[Decorator]) -> Vec<String> {
    let mut protocols = Vec::new();
    for d in decorators {
        if let BuiltinDecorator::Derive(protos) = classify_decorator(d) {
            protocols.extend(protos);
        }
    }
    protocols
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dataclass_options() {
        let opts = DataclassOptions::new();
        assert!(opts.init);
        assert!(opts.repr);
        assert!(opts.eq);
        assert!(!opts.order);
        assert!(!opts.frozen);
    }
    
    #[test]
    fn test_is_builtin_decorator() {
        assert!(is_builtin_decorator("staticmethod"));
        assert!(is_builtin_decorator("dataclass"));
        assert!(!is_builtin_decorator("my_custom_decorator"));
    }
    
    #[test]
    fn test_macro_registry() {
        let mut registry = MacroRegistry::new();
        assert!(!registry.contains("test_macro"));
        
        registry.register(DeclarativeMacro {
            name: "test_macro".to_string(),
            rules: vec![],
        });
        
        assert!(registry.contains("test_macro"));
    }
}
