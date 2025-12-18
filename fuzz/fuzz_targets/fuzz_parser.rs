//! Fuzz target for the Roast parser.
//!
//! This target feeds random input to the parser to discover crashes,
//! hangs, and other bugs.
//!
//! Run with: cargo +nightly fuzz run fuzz_parser

#![no_main]

use libfuzzer_sys::fuzz_target;
use roast_common::{DiagnosticSink, Interner};

fuzz_target!(|data: &[u8]| {
    // Only process valid UTF-8
    if let Ok(source) = std::str::from_utf8(data) {
        // Skip very long inputs to avoid timeouts
        if source.len() > 10_000 {
            return;
        }

        let interner = Interner::new();
        let mut diagnostics = DiagnosticSink::new();

        // Try to parse as a module - should never panic
        let _ = roast_parser::parse_module(source, "<fuzz>", &interner, &mut diagnostics);

        // Also try to parse as an expression
        let _ = roast_parser::parse_expr(source, &interner, &mut diagnostics);
    }
});
