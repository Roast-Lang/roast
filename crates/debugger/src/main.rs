//! Roast debugger entry point.

use roast_debugger::DebugAdapter;
use std::io::{stdin, stdout};

fn main() {
    eprintln!("Roast Debug Adapter starting...");
    
    let stdin = stdin();
    let stdout = stdout();
    
    let mut adapter = DebugAdapter::new(stdin.lock(), stdout.lock());
    
    if let Err(e) = adapter.run() {
        eprintln!("Debug adapter error: {}", e);
        std::process::exit(1);
    }
}

