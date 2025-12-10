//! Watch expression support for the debugger.
//!
//! Evaluates expressions in the context of the current stack frame.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::variables::{VariableRef, VariableValue};

/// A watch expression.
#[derive(Debug, Clone)]
pub struct WatchExpression {
    /// Unique ID.
    pub id: i64,
    /// Expression text.
    pub expression: String,
    /// Current value (None if not evaluated yet).
    pub value: Option<VariableValue>,
    /// Error message if evaluation failed.
    pub error: Option<String>,
    /// Frame ID the expression was evaluated in.
    pub frame_id: Option<i64>,
}

impl WatchExpression {
    /// Create a new watch expression.
    pub fn new(id: i64, expression: String) -> Self {
        Self {
            id,
            expression,
            value: None,
            error: None,
            frame_id: None,
        }
    }
    
    /// Get the display value.
    pub fn display(&self) -> String {
        if let Some(ref err) = self.error {
            format!("<error: {}>", err)
        } else if let Some(ref val) = self.value {
            val.display()
        } else {
            "<not evaluated>".to_string()
        }
    }
    
    /// Get the type name.
    pub fn type_name(&self) -> String {
        if self.error.is_some() {
            "error".to_string()
        } else if let Some(ref val) = self.value {
            val.type_name().to_string()
        } else {
            "unknown".to_string()
        }
    }
}

/// Watch expression manager.
pub struct WatchManager {
    /// Next expression ID.
    next_id: AtomicI64,
    /// Active watch expressions.
    watches: HashMap<i64, WatchExpression>,
    /// Expression evaluator.
    evaluator: ExpressionEvaluator,
}

impl WatchManager {
    /// Create a new watch manager.
    pub fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1),
            watches: HashMap::new(),
            evaluator: ExpressionEvaluator::new(),
        }
    }
    
    /// Add a watch expression.
    pub fn add(&mut self, expression: String) -> i64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let watch = WatchExpression::new(id, expression);
        self.watches.insert(id, watch);
        id
    }
    
    /// Remove a watch expression.
    pub fn remove(&mut self, id: i64) -> bool {
        self.watches.remove(&id).is_some()
    }
    
    /// Get a watch expression.
    pub fn get(&self, id: i64) -> Option<&WatchExpression> {
        self.watches.get(&id)
    }
    
    /// Get all watch expressions.
    pub fn all(&self) -> Vec<&WatchExpression> {
        self.watches.values().collect()
    }
    
    /// Evaluate a single expression in the given context.
    pub fn evaluate(
        &mut self,
        expression: &str,
        context: &EvaluationContext,
    ) -> EvaluationResult {
        self.evaluator.evaluate(expression, context)
    }
    
    /// Evaluate a watch expression by ID.
    pub fn evaluate_watch(
        &mut self,
        id: i64,
        context: &EvaluationContext,
    ) -> Option<EvaluationResult> {
        let expr = self.watches.get(&id)?.expression.clone();
        let result = self.evaluator.evaluate(&expr, context);
        
        // Update the watch with the result
        if let Some(watch) = self.watches.get_mut(&id) {
            watch.frame_id = Some(context.frame_id);
            match &result {
                EvaluationResult::Success { value, .. } => {
                    watch.value = Some(value.clone());
                    watch.error = None;
                }
                EvaluationResult::Error { message } => {
                    watch.value = None;
                    watch.error = Some(message.clone());
                }
            }
        }
        
        Some(result)
    }
    
    /// Evaluate all watches in the given context.
    pub fn evaluate_all(&mut self, context: &EvaluationContext) {
        let ids: Vec<i64> = self.watches.keys().copied().collect();
        for id in ids {
            self.evaluate_watch(id, context);
        }
    }
    
    /// Clear all watches.
    pub fn clear(&mut self) {
        self.watches.clear();
    }
}

impl Default for WatchManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for expression evaluation.
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// Current frame ID.
    pub frame_id: i64,
    /// Local variables.
    pub locals: HashMap<String, VariableValue>,
    /// Global variables.
    pub globals: HashMap<String, VariableValue>,
    /// Evaluation purpose (hover, watch, repl, etc.).
    pub context_type: EvaluationContextType,
}

/// Type of evaluation context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationContextType {
    /// Hover evaluation (for tooltip).
    Hover,
    /// Watch expression.
    Watch,
    /// REPL evaluation.
    Repl,
    /// Conditional breakpoint.
    Condition,
}

impl EvaluationContext {
    /// Create a new evaluation context.
    pub fn new(frame_id: i64) -> Self {
        Self {
            frame_id,
            locals: HashMap::new(),
            globals: HashMap::new(),
            context_type: EvaluationContextType::Watch,
        }
    }
    
    /// Set context type.
    pub fn with_context_type(mut self, context_type: EvaluationContextType) -> Self {
        self.context_type = context_type;
        self
    }
    
    /// Add a local variable.
    pub fn add_local(&mut self, name: String, value: VariableValue) {
        self.locals.insert(name, value);
    }
    
    /// Add a global variable.
    pub fn add_global(&mut self, name: String, value: VariableValue) {
        self.globals.insert(name, value);
    }
    
    /// Look up a variable by name.
    pub fn lookup(&self, name: &str) -> Option<&VariableValue> {
        // Check locals first, then globals
        self.locals.get(name).or_else(|| self.globals.get(name))
    }
}

/// Result of expression evaluation.
#[derive(Debug, Clone)]
pub enum EvaluationResult {
    /// Successful evaluation.
    Success {
        /// The result value.
        value: VariableValue,
        /// Variable reference for expansion (0 if not expandable).
        variables_reference: VariableRef,
    },
    /// Evaluation failed.
    Error {
        /// Error message.
        message: String,
    },
}

impl EvaluationResult {
    /// Create a success result.
    pub fn success(value: VariableValue) -> Self {
        let has_children = value.has_children();
        Self::Success {
            value,
            variables_reference: if has_children { 1 } else { 0 }, // Would allocate proper ref
        }
    }
    
    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
    
    /// Check if successful.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
    
    /// Get the value if successful.
    pub fn value(&self) -> Option<&VariableValue> {
        match self {
            Self::Success { value, .. } => Some(value),
            _ => None,
        }
    }
}

/// Simple expression evaluator.
///
/// Supports:
/// - Variable lookup
/// - Attribute access (a.b)
/// - Subscript access (a[b])
/// - Simple arithmetic (+, -, *, /)
/// - Comparisons (==, !=, <, >, <=, >=)
/// - Literals (int, float, string, bool)
pub struct ExpressionEvaluator {
    // For now, a simple implementation
}

impl ExpressionEvaluator {
    /// Create a new evaluator.
    pub fn new() -> Self {
        Self {}
    }
    
    /// Evaluate an expression.
    pub fn evaluate(&self, expression: &str, context: &EvaluationContext) -> EvaluationResult {
        let expr = expression.trim();
        
        if expr.is_empty() {
            return EvaluationResult::error("Empty expression");
        }
        
        // Try to parse and evaluate
        match self.parse_and_eval(expr, context) {
            Ok(value) => EvaluationResult::success(value),
            Err(msg) => EvaluationResult::error(msg),
        }
    }
    
    fn parse_and_eval(&self, expr: &str, context: &EvaluationContext) -> Result<VariableValue, String> {
        let expr = expr.trim();
        
        // Check for comparison operators (from longest to shortest to avoid prefix issues)
        let comparison_ops = ["==", "!=", "<=", ">=", "<", ">"];
        for op in comparison_ops {
            if let Some((left, right)) = self.split_binary(expr, op) {
                let left_val = self.parse_and_eval(left, context)?;
                let right_val = self.parse_and_eval(right, context)?;
                let result = match op {
                    "==" => self.compare_eq(&left_val, &right_val)?,
                    "!=" => !self.compare_eq(&left_val, &right_val)?,
                    "<=" => self.compare_le(&left_val, &right_val)?,
                    ">=" => self.compare_ge(&left_val, &right_val)?,
                    "<" => self.compare_lt(&left_val, &right_val)?,
                    ">" => self.compare_gt(&left_val, &right_val)?,
                    _ => unreachable!(),
                };
                return Ok(VariableValue::Bool(result));
            }
        }
        
        // Check for arithmetic operators (lower precedence first: +, -)
        for op in ["+", "-"] {
            if let Some((left, right)) = self.split_binary(expr, op) {
                let left_val = self.parse_and_eval(left, context)?;
                let right_val = self.parse_and_eval(right, context)?;
                let arith_fn: fn(i64, i64) -> Option<i64> = match op {
                    "+" => |a, b| a.checked_add(b),
                    "-" => |a, b| a.checked_sub(b),
                    _ => unreachable!(),
                };
                return self.apply_arithmetic(&left_val, &right_val, arith_fn, op);
            }
        }
        
        // Higher precedence: *, /, %
        for op in ["*", "/", "%"] {
            if let Some((left, right)) = self.split_binary(expr, op) {
                let left_val = self.parse_and_eval(left, context)?;
                let right_val = self.parse_and_eval(right, context)?;
                let arith_fn: fn(i64, i64) -> Option<i64> = match op {
                    "*" => |a, b| a.checked_mul(b),
                    "/" => |a, b| if b != 0 { Some(a / b) } else { None },
                    "%" => |a, b| if b != 0 { Some(a % b) } else { None },
                    _ => unreachable!(),
                };
                return self.apply_arithmetic(&left_val, &right_val, arith_fn, op);
            }
        }
        
        // Check for parentheses
        if expr.starts_with('(') && expr.ends_with(')') {
            return self.parse_and_eval(&expr[1..expr.len() - 1], context);
        }
        
        // Try to parse as literal BEFORE attribute access (to handle floats like 3.14)
        if let Some(val) = self.parse_literal(expr) {
            return Ok(val);
        }
        
        // Check for attribute access
        if let Some(dot_idx) = expr.rfind('.') {
            let obj_part = &expr[..dot_idx];
            let attr = &expr[dot_idx + 1..];
            
            if !attr.is_empty() && attr.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let obj = self.parse_and_eval(obj_part, context)?;
                return self.get_attribute(&obj, attr);
            }
        }
        
        // Check for subscript access
        if expr.ends_with(']') {
            if let Some(bracket_idx) = expr.rfind('[') {
                let obj_part = &expr[..bracket_idx];
                let index_part = &expr[bracket_idx + 1..expr.len() - 1];
                
                let obj = self.parse_and_eval(obj_part, context)?;
                let index = self.parse_and_eval(index_part, context)?;
                return self.get_subscript(&obj, &index);
            }
        }
        
        // Try variable lookup
        if expr.chars().all(|c| c.is_alphanumeric() || c == '_') {
            if let Some(val) = context.lookup(expr) {
                return Ok(val.clone());
            }
            return Err(format!("Name '{}' is not defined", expr));
        }
        
        Err(format!("Cannot evaluate expression: {}", expr))
    }
    
    fn split_binary<'a>(&self, expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
        // Find operator outside of brackets/parens
        let mut depth = 0;
        let mut i = expr.len();
        
        while i > 0 {
            i -= 1;
            let c = expr.chars().nth(i)?;
            
            match c {
                ')' | ']' => depth += 1,
                '(' | '[' => depth -= 1,
                _ if depth == 0 => {
                    if expr[i..].starts_with(op) {
                        let left = expr[..i].trim();
                        let right = expr[i + op.len()..].trim();
                        if !left.is_empty() && !right.is_empty() {
                            return Some((left, right));
                        }
                    }
                }
                _ => {}
            }
        }
        
        None
    }
    
    fn parse_literal(&self, expr: &str) -> Option<VariableValue> {
        // Boolean
        if expr == "True" || expr == "true" {
            return Some(VariableValue::Bool(true));
        }
        if expr == "False" || expr == "false" {
            return Some(VariableValue::Bool(false));
        }
        if expr == "None" || expr == "null" {
            return Some(VariableValue::None);
        }
        
        // Integer
        if let Ok(i) = expr.parse::<i64>() {
            return Some(VariableValue::Int(i));
        }
        
        // Float
        if let Ok(f) = expr.parse::<f64>() {
            return Some(VariableValue::Float(f));
        }
        
        // String
        if (expr.starts_with('"') && expr.ends_with('"')) ||
           (expr.starts_with('\'') && expr.ends_with('\'')) {
            let s = &expr[1..expr.len() - 1];
            return Some(VariableValue::String(s.to_string()));
        }
        
        // List literal
        if expr.starts_with('[') && expr.ends_with(']') {
            let inner = expr[1..expr.len() - 1].trim();
            if inner.is_empty() {
                return Some(VariableValue::List(vec![]));
            }
            // Would need more sophisticated parsing for nested structures
        }
        
        None
    }
    
    fn get_attribute(&self, obj: &VariableValue, attr: &str) -> Result<VariableValue, String> {
        match obj {
            VariableValue::Object { fields, .. } => {
                fields.get(attr)
                    .cloned()
                    .ok_or_else(|| format!("Object has no attribute '{}'", attr))
            }
            VariableValue::Dict(map) => {
                map.get(attr)
                    .cloned()
                    .ok_or_else(|| format!("Key '{}' not found", attr))
            }
            VariableValue::String(s) => {
                // String methods
                match attr {
                    "length" | "len" => Ok(VariableValue::Int(s.len() as i64)),
                    _ => Err(format!("'str' has no attribute '{}'", attr)),
                }
            }
            VariableValue::List(items) => {
                match attr {
                    "length" | "len" => Ok(VariableValue::Int(items.len() as i64)),
                    _ => Err(format!("'list' has no attribute '{}'", attr)),
                }
            }
            _ => Err(format!("Cannot get attribute '{}' of {}", attr, obj.type_name())),
        }
    }
    
    fn get_subscript(&self, obj: &VariableValue, index: &VariableValue) -> Result<VariableValue, String> {
        match obj {
            VariableValue::List(items) => {
                if let VariableValue::Int(i) = index {
                    let idx = if *i < 0 {
                        (items.len() as i64 + i) as usize
                    } else {
                        *i as usize
                    };
                    items.get(idx)
                        .cloned()
                        .ok_or_else(|| format!("Index {} out of range", i))
                } else {
                    Err("List indices must be integers".to_string())
                }
            }
            VariableValue::Dict(map) => {
                if let VariableValue::String(key) = index {
                    map.get(key)
                        .cloned()
                        .ok_or_else(|| format!("Key '{}' not found", key))
                } else {
                    Err("Dict key must be string".to_string())
                }
            }
            VariableValue::String(s) => {
                if let VariableValue::Int(i) = index {
                    let idx = if *i < 0 {
                        (s.len() as i64 + i) as usize
                    } else {
                        *i as usize
                    };
                    s.chars()
                        .nth(idx)
                        .map(|c| VariableValue::String(c.to_string()))
                        .ok_or_else(|| format!("Index {} out of range", i))
                } else {
                    Err("String indices must be integers".to_string())
                }
            }
            _ => Err(format!("'{}' object is not subscriptable", obj.type_name())),
        }
    }
    
    fn apply_arithmetic(
        &self,
        left: &VariableValue,
        right: &VariableValue,
        op_fn: fn(i64, i64) -> Option<i64>,
        op_name: &str,
    ) -> Result<VariableValue, String> {
        match (left, right) {
            (VariableValue::Int(a), VariableValue::Int(b)) => {
                op_fn(*a, *b)
                    .map(VariableValue::Int)
                    .ok_or_else(|| format!("Arithmetic overflow or division by zero"))
            }
            (VariableValue::Float(a), VariableValue::Float(b)) => {
                let result = match op_name {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => a / b,
                    "%" => a % b,
                    _ => return Err(format!("Unknown operator: {}", op_name)),
                };
                Ok(VariableValue::Float(result))
            }
            (VariableValue::Int(a), VariableValue::Float(b)) |
            (VariableValue::Float(b), VariableValue::Int(a)) => {
                let a = *a as f64;
                let result = match op_name {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => a / b,
                    "%" => a % b,
                    _ => return Err(format!("Unknown operator: {}", op_name)),
                };
                Ok(VariableValue::Float(result))
            }
            (VariableValue::String(a), VariableValue::String(b)) if op_name == "+" => {
                Ok(VariableValue::String(format!("{}{}", a, b)))
            }
            _ => Err(format!(
                "Unsupported operand types for {}: '{}' and '{}'",
                op_name, left.type_name(), right.type_name()
            )),
        }
    }
    
    fn compare_eq(&self, a: &VariableValue, b: &VariableValue) -> Result<bool, String> {
        match (a, b) {
            (VariableValue::Int(x), VariableValue::Int(y)) => Ok(x == y),
            (VariableValue::Float(x), VariableValue::Float(y)) => Ok((x - y).abs() < f64::EPSILON),
            (VariableValue::Bool(x), VariableValue::Bool(y)) => Ok(x == y),
            (VariableValue::String(x), VariableValue::String(y)) => Ok(x == y),
            (VariableValue::None, VariableValue::None) => Ok(true),
            (VariableValue::None, _) | (_, VariableValue::None) => Ok(false),
            _ => Err(format!(
                "Cannot compare '{}' with '{}'",
                a.type_name(), b.type_name()
            )),
        }
    }
    
    fn compare_lt(&self, a: &VariableValue, b: &VariableValue) -> Result<bool, String> {
        match (a, b) {
            (VariableValue::Int(x), VariableValue::Int(y)) => Ok(x < y),
            (VariableValue::Float(x), VariableValue::Float(y)) => Ok(x < y),
            (VariableValue::String(x), VariableValue::String(y)) => Ok(x < y),
            _ => Err(format!(
                "'<' not supported between '{}' and '{}'",
                a.type_name(), b.type_name()
            )),
        }
    }
    
    fn compare_gt(&self, a: &VariableValue, b: &VariableValue) -> Result<bool, String> {
        self.compare_lt(b, a)
    }
    
    fn compare_le(&self, a: &VariableValue, b: &VariableValue) -> Result<bool, String> {
        Ok(self.compare_lt(a, b)? || self.compare_eq(a, b)?)
    }
    
    fn compare_ge(&self, a: &VariableValue, b: &VariableValue) -> Result<bool, String> {
        Ok(self.compare_gt(a, b)? || self.compare_eq(a, b)?)
    }
}

impl Default for ExpressionEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_literal_eval() {
        let eval = ExpressionEvaluator::new();
        let ctx = EvaluationContext::new(1);
        
        assert!(matches!(
            eval.evaluate("42", &ctx),
            EvaluationResult::Success { value: VariableValue::Int(42), .. }
        ));
        
        // Test float literal
        if let EvaluationResult::Success { value: VariableValue::Float(f), .. } = eval.evaluate("3.14", &ctx) {
            assert!((f - 3.14).abs() < 0.001, "Float value {} not close to 3.14", f);
        } else {
            panic!("Expected Float result for '3.14', got {:?}", eval.evaluate("3.14", &ctx));
        }
        
        assert!(matches!(
            eval.evaluate("True", &ctx),
            EvaluationResult::Success { value: VariableValue::Bool(true), .. }
        ));
        
        assert!(matches!(
            eval.evaluate("\"hello\"", &ctx),
            EvaluationResult::Success { value: VariableValue::String(s), .. } if s == "hello"
        ));
    }
    
    #[test]
    fn test_variable_lookup() {
        let eval = ExpressionEvaluator::new();
        let mut ctx = EvaluationContext::new(1);
        ctx.add_local("x".to_string(), VariableValue::Int(42));
        
        assert!(matches!(
            eval.evaluate("x", &ctx),
            EvaluationResult::Success { value: VariableValue::Int(42), .. }
        ));
        
        assert!(matches!(
            eval.evaluate("undefined", &ctx),
            EvaluationResult::Error { .. }
        ));
    }
    
    #[test]
    fn test_arithmetic() {
        let eval = ExpressionEvaluator::new();
        let mut ctx = EvaluationContext::new(1);
        ctx.add_local("x".to_string(), VariableValue::Int(10));
        ctx.add_local("y".to_string(), VariableValue::Int(5));
        
        assert!(matches!(
            eval.evaluate("x + y", &ctx),
            EvaluationResult::Success { value: VariableValue::Int(15), .. }
        ));
        
        assert!(matches!(
            eval.evaluate("x * 2", &ctx),
            EvaluationResult::Success { value: VariableValue::Int(20), .. }
        ));
    }
    
    #[test]
    fn test_comparison() {
        let eval = ExpressionEvaluator::new();
        let mut ctx = EvaluationContext::new(1);
        ctx.add_local("x".to_string(), VariableValue::Int(10));
        
        assert!(matches!(
            eval.evaluate("x == 10", &ctx),
            EvaluationResult::Success { value: VariableValue::Bool(true), .. }
        ));
        
        assert!(matches!(
            eval.evaluate("x > 5", &ctx),
            EvaluationResult::Success { value: VariableValue::Bool(true), .. }
        ));
    }
}
