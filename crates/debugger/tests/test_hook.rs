use std::sync::Arc;
use std::thread;
use std::time::Duration;
use roast_debugger::session::{DebugSession, SessionState};
use roast_debugger::hook::SessionDebugHook;
use roast_vm::VM;
use roast_codegen::{Bytecode, Instruction, OpCode, Operand};

#[test]
fn test_debug_hook_integration() {
    // Create VM and Session
    let mut vm = VM::new();
    let session = Arc::new(DebugSession::new());
    let hook = Arc::new(SessionDebugHook::new(session.clone()));
    
    // Attach hook
    vm.set_debug_hook(Some(hook));
    
    // Create simple bytecode:
    // 0: LoadTrue
    // 1: LoadFalse 
    // 2: LoadNone
    let mut code = Bytecode::new("test");
    code.instructions.push(Instruction::new(OpCode::LoadTrue).with_line(1));
    code.instructions.push(Instruction::new(OpCode::LoadFalse).with_line(1));
    code.instructions.push(Instruction::new(OpCode::LoadNone).with_line(1));
    
    // Add dummy line info (all on line 1)
    // code.lines.add_line(0, 1);
    // code.lines.add_line(1, 1);
    // code.lines.add_line(2, 1);
    
    let code = Arc::new(code);
    
    // Start session
    session.start();
    
    // Spawn VM in a separate thread because wait_for_continue blocks
    let vm_thread = thread::spawn(move || {
        vm.execute(code).unwrap();
    });
    
    // Wait for VM to finish
    vm_thread.join().unwrap();
    
    // Verify session state is Running (since we didn't pause)
    assert_eq!(session.state(), SessionState::Running);
}

#[test]
fn test_debug_hook_pause() {
    // Create VM and Session
    let mut vm = VM::new();
    let session = Arc::new(DebugSession::new());
    let hook = Arc::new(SessionDebugHook::new(session.clone()));
    
    // Attach hook
    vm.set_debug_hook(Some(hook));
    
    // Create simple bytecode
    let mut code = Bytecode::new("test");
    code.instructions.push(Instruction::new(OpCode::LoadTrue).with_line(1));
    code.instructions.push(Instruction::new(OpCode::LoadFalse).with_line(1));
    
    // Add dummy line info
    // code.lines.add_line(0, 1);
    // code.lines.add_line(1, 1);
    
    let code = Arc::new(code);
    
    // Start session and request pause
    session.start();
    session.stop(roast_debugger::session::StopReason::Pause);
    
    // Spawn VM
    let session_clone = session.clone();
    let vm_thread = thread::spawn(move || {
        vm.execute(code).unwrap();
    });
    
    // Give VM time to hit the hook and pause
    thread::sleep(Duration::from_millis(100));
    
    // Verify session is stopped
    assert_eq!(session.state(), SessionState::Stopped);
    
    // Resume execution
    session.continue_execution();
    
    // Wait for VM to finish
    vm_thread.join().unwrap();
}
