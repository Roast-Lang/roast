//! Debug session management.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, Condvar};

use crate::breakpoint::BreakpointManager;
use crate::stack::{CallStack, Frame};
use crate::variables::{Scope, ScopeType, Variable, VariableStore};
use crate::watch::{WatchManager, WatchExpression, EvaluationContext, EvaluationResult};

/// Debug session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state.
    Initializing,
    /// Program launched/attached.
    Running,
    /// Stopped at breakpoint/step.
    Stopped,
    /// Program terminated.
    Terminated,
}

/// Stop reason.
#[derive(Debug, Clone)]
pub enum StopReason {
    /// Hit a breakpoint.
    Breakpoint(i64),
    /// Step completed.
    Step,
    /// Pause requested.
    Pause,
    /// Exception occurred.
    Exception(String),
    /// Entry point.
    Entry,
}

/// Debug session.
pub struct DebugSession {
    /// Current state.
    state: AtomicSessionState,
    /// Main thread ID.
    thread_id: i64,
    /// Breakpoint manager.
    breakpoints: Mutex<BreakpointManager>,
    /// Call stack.
    stack: Mutex<CallStack>,
    /// Variable store.
    variables: Mutex<VariableStore>,
    /// Watch expression manager.
    watches: Mutex<WatchManager>,
    /// Current source file.
    current_source: Mutex<Option<PathBuf>>,
    /// Current line.
    current_line: AtomicI64,
    /// Stop on entry.
    #[allow(dead_code)]
    stop_on_entry: bool,
    /// Last stop reason.
    stop_reason: Mutex<Option<StopReason>>,
    /// Condition variable for pausing execution.
    continue_cond: Condvar,
    /// Mutex for pausing execution.
    continue_mutex: Mutex<()>,
}

/// Atomic session state.
struct AtomicSessionState(std::sync::atomic::AtomicU8);

impl AtomicSessionState {
    fn new(state: SessionState) -> Self {
        Self(std::sync::atomic::AtomicU8::new(state as u8))
    }
    
    fn load(&self) -> SessionState {
        match self.0.load(Ordering::SeqCst) {
            0 => SessionState::Initializing,
            1 => SessionState::Running,
            2 => SessionState::Stopped,
            _ => SessionState::Terminated,
        }
    }
    
    fn store(&self, state: SessionState) {
        self.0.store(state as u8, Ordering::SeqCst);
    }
}

impl DebugSession {
    /// Create a new session.
    pub fn new() -> Self {
        Self {
            state: AtomicSessionState::new(SessionState::Initializing),
            thread_id: 1,
            breakpoints: Mutex::new(BreakpointManager::new()),
            stack: Mutex::new(CallStack::new()),
            variables: Mutex::new(VariableStore::new()),
            watches: Mutex::new(WatchManager::new()),
            current_source: Mutex::new(None),
            current_line: AtomicI64::new(0),
            stop_on_entry: false,
            stop_reason: Mutex::new(None),
            continue_cond: Condvar::new(),
            continue_mutex: Mutex::new(()),
        }
    }
    
    /// Get current state.
    pub fn state(&self) -> SessionState {
        self.state.load()
    }
    
    /// Get thread ID.
    pub fn thread_id(&self) -> i64 {
        self.thread_id
    }
    
    // =========================================================================
    // Breakpoints
    // =========================================================================
    
    /// Set breakpoints for a source.
    pub fn set_breakpoints(
        &self,
        source: PathBuf,
        breakpoints: Vec<crate::protocol::SourceBreakpoint>,
    ) -> Vec<crate::protocol::BreakpointResponse> {
        let mut mgr = self.breakpoints.lock().unwrap();
        
        // Clear existing breakpoints for this source
        mgr.clear_source(&source);
        
        // Add new breakpoints
        breakpoints.iter().map(|bp| {
            let added = mgr.add_conditional(
                source.clone(),
                bp.line,
                bp.condition.clone(),
                bp.hit_condition.clone(),
                bp.log_message.clone(),
            );
            
            crate::protocol::BreakpointResponse {
                id: Some(added.id),
                verified: true, // Would verify against source
                message: None,
                source: Some(crate::protocol::Source {
                    name: source.file_name().map(|n| n.to_string_lossy().to_string()),
                    path: Some(source.to_string_lossy().to_string()),
                    source_reference: None,
                    presentation_hint: None,
                    origin: None,
                }),
                line: Some(bp.line),
                column: bp.column,
                end_line: None,
                end_column: None,
            }
        }).collect()
    }
    
    /// Check if we should stop at location.
    pub fn should_stop(&self, source: &PathBuf, line: i64) -> Option<i64> {
        let mgr = self.breakpoints.lock().unwrap();
        mgr.check(source, line).map(|bp| bp.id)
    }
    
    // =========================================================================
    // Execution Control
    // =========================================================================
    
    /// Start execution.
    pub fn start(&self) {
        self.state.store(SessionState::Running);
    }
    
    /// Stop execution.
    pub fn stop(&self, reason: StopReason) {
        self.state.store(SessionState::Stopped);
        *self.stop_reason.lock().unwrap() = Some(reason);
    }
    
    /// Continue execution.
    pub fn continue_execution(&self) {
        self.state.store(SessionState::Running);
        *self.stop_reason.lock().unwrap() = None;
        self.continue_cond.notify_all();
    }
    
    /// Wait for continue signal.
    pub fn wait_for_continue(&self) {
        let mut guard = self.continue_mutex.lock().unwrap();
        while self.state() == SessionState::Stopped {
            guard = self.continue_cond.wait(guard).unwrap();
        }
    }
    
    /// Terminate.
    pub fn terminate(&self) {
        self.state.store(SessionState::Terminated);
    }
    
    /// Get stop reason.
    pub fn stop_reason(&self) -> Option<StopReason> {
        self.stop_reason.lock().unwrap().clone()
    }
    
    // =========================================================================
    // Stack
    // =========================================================================
    
    /// Push a stack frame.
    pub fn push_frame(&self, name: String, source: Option<PathBuf>, line: i64) -> i64 {
        let mut stack = self.stack.lock().unwrap();
        stack.push(name, source, line, 1)
    }
    
    /// Pop a stack frame.
    pub fn pop_frame(&self) -> Option<Frame> {
        let mut stack = self.stack.lock().unwrap();
        stack.pop()
    }
    
    /// Get stack trace.
    pub fn stack_trace(&self) -> Vec<crate::protocol::StackFrame> {
        let stack = self.stack.lock().unwrap();
        
        stack.frames().iter().map(|f| {
            crate::protocol::StackFrame {
                id: f.id,
                name: f.name.clone(),
                source: f.source.as_ref().map(|s| crate::protocol::Source {
                    name: s.file_name().map(|n| n.to_string_lossy().to_string()),
                    path: Some(s.to_string_lossy().to_string()),
                    source_reference: None,
                    presentation_hint: None,
                    origin: None,
                }),
                line: f.line,
                column: f.column,
                end_line: f.end_line,
                end_column: f.end_column,
                instruction_pointer_reference: None,
                module_id: None,
                presentation_hint: None,
            }
        }).collect()
    }
    
    /// Update current location.
    pub fn update_location(&self, source: PathBuf, line: i64) {
        *self.current_source.lock().unwrap() = Some(source);
        self.current_line.store(line, Ordering::SeqCst);
        
        let mut stack = self.stack.lock().unwrap();
        stack.update_location(line, 1);
    }
    
    // =========================================================================
    // Variables
    // =========================================================================
    
    /// Get scopes for a frame.
    pub fn scopes(&self, frame_id: i64) -> Vec<crate::protocol::Scope> {
        let vars = self.variables.lock().unwrap();
        
        if let Some(scopes) = vars.get_scopes(frame_id) {
            scopes.iter().map(|s| crate::protocol::Scope {
                name: s.scope_type.name().to_string(),
                presentation_hint: Some(match s.scope_type {
                    ScopeType::Local => "locals",
                    ScopeType::Global => "globals",
                    ScopeType::Closure => "arguments",
                }.to_string()),
                variables_reference: s.reference,
                named_variables: Some(s.variables.len() as i64),
                indexed_variables: None,
                expensive: false,
                source: None,
                line: None,
                column: None,
                end_line: None,
                end_column: None,
            }).collect()
        } else {
            // Return default scopes
            vec![
                crate::protocol::Scope {
                    name: "Locals".to_string(),
                    presentation_hint: Some("locals".to_string()),
                    variables_reference: frame_id,
                    named_variables: None,
                    indexed_variables: None,
                    expensive: false,
                    source: None,
                    line: None,
                    column: None,
                    end_line: None,
                    end_column: None,
                },
            ]
        }
    }
    
    /// Get variables for a reference.
    pub fn variables(&self, reference: i64) -> Vec<crate::protocol::Variable> {
        let vars = self.variables.lock().unwrap();
        
        if let Some(variables) = vars.get_variables(reference) {
            variables.iter().map(|v| crate::protocol::Variable {
                name: v.name.clone(),
                value: v.value.display(),
                type_name: Some(v.value.type_name().to_string()),
                presentation_hint: None,
                evaluate_name: Some(v.name.clone()),
                variables_reference: if v.value.has_children() { v.reference } else { 0 },
                named_variables: if v.value.has_children() {
                    Some(v.value.children_count() as i64)
                } else {
                    None
                },
                indexed_variables: None,
                memory_reference: None,
            }).collect()
        } else {
            vec![]
        }
    }
    
    /// Set frame variables.
    pub fn set_frame_variables(&self, frame_id: i64, locals: Vec<Variable>) {
        let mut vars = self.variables.lock().unwrap();
        
        let local_scope = Scope {
            scope_type: ScopeType::Local,
            reference: frame_id,
            variables: locals,
        };
        
        vars.set_scopes(frame_id, vec![local_scope]);
    }
    
    /// Update variables from VM state.
    pub fn update_variables_from_vm(&self, vm_locals: std::collections::HashMap<String, crate::variables::VariableValue>) {
        let mut vars = self.variables.lock().unwrap();
        let stack = self.stack.lock().unwrap();
        
        // Get current frame ID
        let frame_id = stack.frames().last().map(|f| f.id).unwrap_or(1);
        
        // Convert to Variable list
        let mut ref_counter = frame_id * 1000; // Namespace references per frame
        let locals: Vec<Variable> = vm_locals.into_iter().map(|(name, value)| {
            ref_counter += 1;
            Variable {
                name,
                value,
                reference: ref_counter,
            }
        }).collect();
        
        // Create local scope
        let local_scope = Scope {
            scope_type: ScopeType::Local,
            reference: frame_id,
            variables: locals,
        };
        
        vars.set_scopes(frame_id, vec![local_scope]);
    }
    
    // =========================================================================
    // Watch Expressions
    // =========================================================================
    
    /// Add a watch expression.
    pub fn add_watch(&self, expression: String) -> i64 {
        let mut watches = self.watches.lock().unwrap();
        watches.add(expression)
    }
    
    /// Remove a watch expression.
    pub fn remove_watch(&self, id: i64) -> bool {
        let mut watches = self.watches.lock().unwrap();
        watches.remove(id)
    }
    
    /// Get all watch expressions.
    pub fn get_watches(&self) -> Vec<WatchExpression> {
        let watches = self.watches.lock().unwrap();
        watches.all().into_iter().cloned().collect()
    }
    
    /// Evaluate an expression in the context of a frame.
    pub fn evaluate_expression(&self, expression: &str, frame_id: i64) -> EvaluationResult {
        let mut watches = self.watches.lock().unwrap();
        let context = self.build_evaluation_context(frame_id);
        watches.evaluate(expression, &context)
    }
    
    /// Evaluate all watch expressions.
    pub fn evaluate_all_watches(&self, frame_id: i64) {
        let mut watches = self.watches.lock().unwrap();
        let context = self.build_evaluation_context(frame_id);
        watches.evaluate_all(&context);
    }
    
    /// Build evaluation context from current frame.
    fn build_evaluation_context(&self, frame_id: i64) -> EvaluationContext {
        let vars = self.variables.lock().unwrap();
        let mut context = EvaluationContext::new(frame_id);
        
        // Add variables from the frame's scopes
        if let Some(scopes) = vars.get_scopes(frame_id) {
            for scope in scopes {
                for var in &scope.variables {
                    match scope.scope_type {
                        ScopeType::Local => {
                            context.add_local(var.name.clone(), var.value.clone());
                        }
                        ScopeType::Global => {
                            context.add_global(var.name.clone(), var.value.clone());
                        }
                        ScopeType::Closure => {
                            context.add_local(var.name.clone(), var.value.clone());
                        }
                    }
                }
            }
        }
        
        context
    }
}

impl Default for DebugSession {
    fn default() -> Self {
        Self::new()
    }
}

