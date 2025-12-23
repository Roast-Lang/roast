//! Concurrency safety checking for Roast.
//!
//! This module provides compile-time verification that values shared
//! across threads or sent between threads satisfy Send and Sync requirements.

use crate::types::*;
use crate::ownership::{OwnershipAnalyzer, OwnershipInfo, TypeTrait};
use roast_common::Span;
use std::collections::HashMap;

/// Concurrency safety checker.
///
/// Verifies that:
/// - Values sent between threads implement Send
/// - Values shared between threads implement Sync  
/// - Closures spawned as tasks only capture Send values
/// - Mutex guards contain Send types
pub struct ConcurrencyChecker {
    /// Ownership analyzer for type trait checking.
    analyzer: OwnershipAnalyzer,
    
    /// Accumulated errors.
    errors: Vec<ConcurrencyError>,
    
    /// Known non-Send types (for better error messages).
    non_send_types: HashMap<String, String>,
    
    /// Known non-Sync types.
    non_sync_types: HashMap<String, String>,
}

/// A concurrency safety error.
#[derive(Debug, Clone)]
pub struct ConcurrencyError {
    /// Error kind.
    pub kind: ConcurrencyErrorKind,
    
    /// Source location.
    pub span: Span,
    
    /// The type that caused the error.
    pub ty: Type,
    
    /// Additional context.
    pub context: String,
}

/// Kind of concurrency error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcurrencyErrorKind {
    /// Type cannot be sent between threads.
    NotSend,
    
    /// Type cannot be shared between threads.
    NotSync,
    
    /// Closure captures non-Send value.
    CapturesNonSend,
    
    /// Async task captures non-Send value.
    TaskCapturesNonSend,
    
    /// Mutex contains non-Send type.
    MutexContainsNonSend,
    
    /// Arc contains non-Send+Sync type.
    ArcNotSendSync,
    
    /// Channel cannot send this type.
    ChannelTypeNotSend,
}

impl std::fmt::Display for ConcurrencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ConcurrencyErrorKind::NotSend => {
                write!(f, "type `{}` cannot be sent between threads safely", self.ty)?;
            }
            ConcurrencyErrorKind::NotSync => {
                write!(f, "type `{}` cannot be shared between threads safely", self.ty)?;
            }
            ConcurrencyErrorKind::CapturesNonSend => {
                write!(f, "closure captures value of type `{}` which is not Send", self.ty)?;
            }
            ConcurrencyErrorKind::TaskCapturesNonSend => {
                write!(f, "async task captures value of type `{}` which is not Send", self.ty)?;
            }
            ConcurrencyErrorKind::MutexContainsNonSend => {
                write!(f, "Mutex<T> requires T: Send, but `{}` is not Send", self.ty)?;
            }
            ConcurrencyErrorKind::ArcNotSendSync => {
                write!(f, "Arc<T> requires T: Send + Sync, but `{}` does not satisfy this", self.ty)?;
            }
            ConcurrencyErrorKind::ChannelTypeNotSend => {
                write!(f, "channel cannot send type `{}` which is not Send", self.ty)?;
            }
        }
        
        if !self.context.is_empty() {
            write!(f, "\n  note: {}", self.context)?;
        }
        
        Ok(())
    }
}

impl ConcurrencyChecker {
    /// Create a new concurrency checker.
    pub fn new() -> Self {
        let mut non_send_types = HashMap::new();
        let mut non_sync_types = HashMap::new();
        
        // Known non-Send types
        non_send_types.insert(
            "Rc".to_string(),
            "Rc uses non-atomic reference counting, use Arc for thread-safe sharing".to_string(),
        );
        non_send_types.insert(
            "Cell".to_string(),
            "Cell has interior mutability that is not thread-safe".to_string(),
        );
        non_send_types.insert(
            "RefCell".to_string(),
            "RefCell has interior mutability that is not thread-safe".to_string(),
        );
        
        // Known non-Sync types  
        non_sync_types.insert(
            "Cell".to_string(),
            "Cell has interior mutability that is not thread-safe, use Atomic types".to_string(),
        );
        non_sync_types.insert(
            "RefCell".to_string(),
            "RefCell has interior mutability that is not thread-safe, use RwLock".to_string(),
        );
        non_sync_types.insert(
            "UnsafeCell".to_string(),
            "UnsafeCell is the building block for interior mutability".to_string(),
        );
        
        Self {
            analyzer: OwnershipAnalyzer::new(),
            errors: Vec::new(),
            non_send_types,
            non_sync_types,
        }
    }

    /// Check if a type is Send.
    pub fn is_send(&mut self, ty: &Type) -> bool {
        self.analyzer.is_send(ty)
    }

    /// Check if a type is Sync.
    pub fn is_sync(&mut self, ty: &Type) -> bool {
        self.analyzer.is_sync(ty)
    }

    /// Validate that a type can be sent between threads.
    pub fn validate_send(&mut self, ty: &Type, span: Span, context: &str) -> bool {
        if self.is_send(ty) {
            true
        } else {
            let hint = self.get_non_send_hint(ty);
            self.errors.push(ConcurrencyError {
                kind: ConcurrencyErrorKind::NotSend,
                span,
                ty: ty.clone(),
                context: if hint.is_empty() { context.to_string() } else { hint },
            });
            false
        }
    }

    /// Validate that a type can be shared between threads.
    pub fn validate_sync(&mut self, ty: &Type, span: Span, context: &str) -> bool {
        if self.is_sync(ty) {
            true
        } else {
            let hint = self.get_non_sync_hint(ty);
            self.errors.push(ConcurrencyError {
                kind: ConcurrencyErrorKind::NotSync,
                span,
                ty: ty.clone(),
                context: if hint.is_empty() { context.to_string() } else { hint },
            });
            false
        }
    }

    /// Validate captured variables for a spawned task.
    ///
    /// When spawning a task/goroutine, all captured variables must be Send.
    pub fn validate_spawn_captures(
        &mut self,
        captures: &[(String, Type)],
        span: Span,
    ) -> bool {
        let mut all_valid = true;
        
        for (name, ty) in captures {
            if !self.is_send(ty) {
                self.errors.push(ConcurrencyError {
                    kind: ConcurrencyErrorKind::TaskCapturesNonSend,
                    span,
                    ty: ty.clone(),
                    context: format!("variable `{}` captured by spawned task", name),
                });
                all_valid = false;
            }
        }
        
        all_valid
    }

    /// Validate a closure that will be sent to another thread.
    pub fn validate_thread_closure(
        &mut self,
        captures: &[(String, Type)],
        span: Span,
    ) -> bool {
        let mut all_valid = true;
        
        for (name, ty) in captures {
            if !self.is_send(ty) {
                self.errors.push(ConcurrencyError {
                    kind: ConcurrencyErrorKind::CapturesNonSend,
                    span,
                    ty: ty.clone(),
                    context: format!("closure captures `{}` which is not Send", name),
                });
                all_valid = false;
            }
        }
        
        all_valid
    }

    /// Validate Mutex<T> - requires T: Send.
    pub fn validate_mutex_type(&mut self, inner_ty: &Type, span: Span) -> bool {
        if self.is_send(inner_ty) {
            true
        } else {
            self.errors.push(ConcurrencyError {
                kind: ConcurrencyErrorKind::MutexContainsNonSend,
                span,
                ty: inner_ty.clone(),
                context: "Mutex<T> protects T from concurrent access, but T must be Send to transfer ownership to other threads".to_string(),
            });
            false
        }
    }

    /// Validate Arc<T> - requires T: Send + Sync.
    pub fn validate_arc_type(&mut self, inner_ty: &Type, span: Span) -> bool {
        let is_send = self.is_send(inner_ty);
        let is_sync = self.is_sync(inner_ty);
        
        if is_send && is_sync {
            true
        } else {
            let mut context = String::new();
            if !is_send {
                context.push_str("T must be Send to share Arc<T> between threads");
            }
            if !is_sync {
                if !context.is_empty() {
                    context.push_str("; ");
                }
                context.push_str("T must be Sync for concurrent immutable access");
            }
            
            self.errors.push(ConcurrencyError {
                kind: ConcurrencyErrorKind::ArcNotSendSync,
                span,
                ty: inner_ty.clone(),
                context,
            });
            false
        }
    }

    /// Validate channel send type - must be Send.
    pub fn validate_channel_type(&mut self, elem_ty: &Type, span: Span) -> bool {
        if self.is_send(elem_ty) {
            true
        } else {
            self.errors.push(ConcurrencyError {
                kind: ConcurrencyErrorKind::ChannelTypeNotSend,
                span,
                ty: elem_ty.clone(),
                context: "values sent through channels must be Send".to_string(),
            });
            false
        }
    }

    /// Get a hint for why a type is not Send.
    fn get_non_send_hint(&self, ty: &Type) -> String {
        match ty {
            Type::Class(cls) => {
                if let Some(hint) = self.non_send_types.get(&cls.name) {
                    return hint.clone();
                }
            }
            Type::Rc(_) => {
                return "Rc uses non-atomic reference counting; use Arc for thread-safe sharing".to_string();
            }
            Type::Ref { inner, mutable: true } => {
                return format!("mutable references (&mut {}) cannot be shared across threads", inner);
            }
            _ => {}
        }
        String::new()
    }

    /// Get a hint for why a type is not Sync.
    fn get_non_sync_hint(&self, ty: &Type) -> String {
        match ty {
            Type::Class(cls) => {
                if let Some(hint) = self.non_sync_types.get(&cls.name) {
                    return hint.clone();
                }
            }
            Type::Ref { mutable: true, .. } => {
                return "mutable references cannot be shared between threads".to_string();
            }
            _ => {}
        }
        String::new()
    }

    /// Get all accumulated errors.
    pub fn errors(&self) -> &[ConcurrencyError] {
        &self.errors
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Clear all errors.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Take all errors, leaving the checker empty.
    pub fn take_errors(&mut self) -> Vec<ConcurrencyError> {
        std::mem::take(&mut self.errors)
    }
}

impl Default for ConcurrencyChecker {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Thread Safety Analysis for Functions
// =============================================================================

/// Analyzes a function for thread safety.
#[derive(Debug, Clone)]
pub struct ThreadSafetyAnalysis {
    /// Whether the function is thread-safe.
    pub is_safe: bool,
    
    /// Reasons why the function might not be thread-safe.
    pub warnings: Vec<String>,
    
    /// Types that need to be Send for the function to be spawnable.
    pub send_requirements: Vec<Type>,
    
    /// Types that need to be Sync for the function to use shared state.
    pub sync_requirements: Vec<Type>,
}

impl ThreadSafetyAnalysis {
    pub fn new() -> Self {
        Self {
            is_safe: true,
            warnings: Vec::new(),
            send_requirements: Vec::new(),
            sync_requirements: Vec::new(),
        }
    }

    /// Mark the function as requiring Send for a captured type.
    pub fn require_send(&mut self, ty: Type) {
        self.send_requirements.push(ty);
    }

    /// Mark the function as requiring Sync for a shared type.
    pub fn require_sync(&mut self, ty: Type) {
        self.sync_requirements.push(ty);
    }

    /// Add a warning about potential thread safety issues.
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Mark the function as not thread-safe.
    pub fn mark_unsafe(&mut self, reason: String) {
        self.is_safe = false;
        self.warnings.push(reason);
    }
}

impl Default for ThreadSafetyAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_types_are_send_sync() {
        let mut checker = ConcurrencyChecker::new();
        
        assert!(checker.is_send(&Type::Int));
        assert!(checker.is_sync(&Type::Int));
        
        assert!(checker.is_send(&Type::Float));
        assert!(checker.is_sync(&Type::Float));
        
        assert!(checker.is_send(&Type::Bool));
        assert!(checker.is_sync(&Type::Bool));
        
        assert!(checker.is_send(&Type::Str));
        assert!(checker.is_sync(&Type::Str));
    }

    #[test]
    fn test_list_send_sync() {
        let mut checker = ConcurrencyChecker::new();
        
        // List of Send types is Send
        let list_int = Type::list(Type::Int);
        assert!(checker.is_send(&list_int));
        assert!(checker.is_sync(&list_int));
    }

    #[test]
    fn test_tuple_send_sync() {
        let mut checker = ConcurrencyChecker::new();
        
        // Tuple of Send types is Send
        let tuple = Type::Tuple(vec![Type::Int, Type::Str, Type::Bool]);
        assert!(checker.is_send(&tuple));
        assert!(checker.is_sync(&tuple));
    }

    #[test]
    fn test_validate_spawn_captures() {
        let mut checker = ConcurrencyChecker::new();
        
        // All Send types should pass
        let captures = vec![
            ("x".to_string(), Type::Int),
            ("y".to_string(), Type::Str),
        ];
        
        let span = Span::default();
        assert!(checker.validate_spawn_captures(&captures, span));
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_error_messages() {
        let error = ConcurrencyError {
            kind: ConcurrencyErrorKind::NotSend,
            span: Span::default(),
            ty: Type::Int,
            context: "test context".to_string(),
        };
        
        let message = format!("{}", error);
        assert!(message.contains("cannot be sent between threads"));
        assert!(message.contains("test context"));
    }
}
