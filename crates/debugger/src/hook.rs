//! Debug hook implementation.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use roast_vm::debug::DebugHook;
use roast_vm::interpreter::{VM, VMResult};
use crate::session::{DebugSession, SessionState, StopReason};
use crate::vm_integration::{VMDebugState, extract_locals, value_to_variable, build_eval_context};
use std::sync::Mutex;

/// Debug hook that connects the VM to the debug session.
pub struct SessionDebugHook {
    session: Arc<DebugSession>,
    /// Debug state for stepping.
    debug_state: Mutex<VMDebugState>,
    /// Current call depth.
    call_depth: AtomicUsize,
    /// Last line we stopped at (to avoid stopping multiple times on same line).
    last_stop_line: AtomicUsize,
}

impl SessionDebugHook {
    pub fn new(session: Arc<DebugSession>) -> Self {
        Self { 
            session,
            debug_state: Mutex::new(VMDebugState::new()),
            call_depth: AtomicUsize::new(0),
            last_stop_line: AtomicUsize::new(0),
        }
    }
    
    /// Start step-in operation.
    pub fn step_in(&self) {
        let mut state = self.debug_state.lock().unwrap();
        state.step_in();
        self.last_stop_line.store(0, Ordering::SeqCst);
    }
    
    /// Start step-over operation.
    pub fn step_over(&self) {
        let depth = self.call_depth.load(Ordering::SeqCst);
        let mut state = self.debug_state.lock().unwrap();
        state.step_over(depth);
        self.last_stop_line.store(0, Ordering::SeqCst);
    }
    
    /// Start step-out operation.
    pub fn step_out(&self) {
        let depth = self.call_depth.load(Ordering::SeqCst);
        let mut state = self.debug_state.lock().unwrap();
        state.step_out(depth);
        self.last_stop_line.store(0, Ordering::SeqCst);
    }
    
    /// Sync variables from VM to session.
    fn sync_variables(&self, vm: &VM) {
        if let Some(frame) = vm.current_frame() {
            let locals = extract_locals(frame);
            self.session.update_variables_from_vm(locals);
        }
    }
    
    /// Build evaluation context from VM.
    pub fn build_context(&self, vm: &VM, frame_id: i64) -> crate::watch::EvaluationContext {
        build_eval_context(vm, frame_id)
    }
}

impl DebugHook for SessionDebugHook {
    fn on_step(&self, vm: &VM) -> VMResult<()> {
        // Update current location
        if let Some(frame) = vm.current_frame() {
            let path = std::path::PathBuf::from(&frame.code.filename);
            if path.as_os_str().is_empty() {
                return Ok(());
            }
            
            // Map instruction pointer to line number
            let line = frame.code.instructions
                .get(frame.ip)
                .map(|i| i.line)
                .unwrap_or(0) as i64;
            
            self.session.update_location(path.clone(), line);
            
            // Sync variables when stopped
            if self.session.state() == SessionState::Stopped {
                self.sync_variables(vm);
            }
            
            // Check for breakpoint hit
            if let Some(bp_id) = self.session.should_stop(&path, line) {
                // Sync variables before stopping
                self.sync_variables(vm);
                
                self.session.stop(StopReason::Breakpoint(bp_id));
                
                // Reset debug state
                {
                    let mut state = self.debug_state.lock().unwrap();
                    state.reset();
                }
                
                self.last_stop_line.store(line as usize, Ordering::SeqCst);
                self.session.wait_for_continue();
                return Ok(());
            }
            
            // Check for step completion
            {
                let state = self.debug_state.lock().unwrap();
                let last_line = self.last_stop_line.load(Ordering::SeqCst);
                
                // Only stop if we're on a different line
                if state.should_stop() && line as usize != last_line {
                    drop(state); // Release lock before sync
                    
                    // Sync variables before stopping
                    self.sync_variables(vm);
                    
                    self.session.stop(StopReason::Step);
                    
                    // Reset debug state
                    {
                        let mut state = self.debug_state.lock().unwrap();
                        state.reset();
                    }
                    
                    self.last_stop_line.store(line as usize, Ordering::SeqCst);
                    self.session.wait_for_continue();
                    return Ok(());
                }
            }
            
            // Check if externally paused
            if self.session.state() == SessionState::Stopped {
                self.sync_variables(vm);
                self.session.wait_for_continue();
            }
        }
        
        Ok(())
    }

    fn on_function_entry(&self, vm: &VM) -> VMResult<()> {
        // Increment call depth
        let depth = self.call_depth.fetch_add(1, Ordering::SeqCst) + 1;
        
        // Update debug state
        {
            let mut state = self.debug_state.lock().unwrap();
            state.current_depth = depth;
            state.on_function_entry();
        }
        
        // Push frame to session stack
        if let Some(frame) = vm.current_frame() {
            let source = if frame.code.filename.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(&frame.code.filename))
            };
            
            let line = frame.code.instructions
                .get(frame.ip)
                .map(|i| i.line as i64)
                .unwrap_or(1);
            
            self.session.push_frame(frame.code.name.clone(), source, line);
        }
        
        Ok(())
    }

    fn on_function_exit(&self, _vm: &VM) -> VMResult<()> {
        // Decrement call depth
        let depth = self.call_depth.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
        
        // Update debug state
        {
            let mut state = self.debug_state.lock().unwrap();
            state.current_depth = depth;
            state.on_function_exit();
        }
        
        // Pop frame from session stack
        self.session.pop_frame();
        
        Ok(())
    }
}

