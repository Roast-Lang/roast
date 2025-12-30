//! Fuzz target for the Roast type checker.

#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::{Arbitrary, Unstructured};
use roast_parser::Parser;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, skipping invalid UTF-8
    if let Ok(source) = std::str::from_utf8(data) {
        // Try to parse first
        let mut parser = Parser::new(source);
        if let Ok(ast) = parser.parse() {
            // If parsing succeeds, we could try type checking
            // Note: This would require setting up a proper type context
            // For now, we just verify the parser doesn't crash on random input
            let _ = ast;
        }
    }
});
