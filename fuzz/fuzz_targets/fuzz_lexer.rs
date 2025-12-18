//! Fuzz target for the Roast lexer.
//!
//! This target feeds random input to the lexer to discover crashes.
//!
//! Run with: cargo +nightly fuzz run fuzz_lexer

#![no_main]

use libfuzzer_sys::fuzz_target;
use roast_common::Interner;

fuzz_target!(|data: &[u8]| {
    // Only process valid UTF-8
    if let Ok(source) = std::str::from_utf8(data) {
        // Skip very long inputs
        if source.len() > 10_000 {
            return;
        }

        let interner = Interner::new();

        // Tokenize input - should never panic
        let _ = roast_parser::tokenize(source, &interner);
    }
});
