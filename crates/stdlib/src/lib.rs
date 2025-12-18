//! Roast Standard Library
//!
//! A comprehensive standard library providing:
//! - Core types and operations
//! - File system and I/O
//! - Networking
//! - Concurrency primitives
//! - Data structures
//! - Encoding and serialization
//! - Testing utilities

// Core modules
pub mod builtins;
pub mod collections;
pub mod io;
pub mod math;
pub mod string;

// File system
pub mod fs;
pub mod path;

// Networking
pub mod net;
pub mod http;

// Concurrency
pub mod sync;
pub mod thread;
pub mod channel;
pub mod async_utils;

// Logging
pub mod logging;

// Data structures
pub mod heap;
pub mod queue;
pub mod graph;

// Encoding
pub mod json;
pub mod base64;
pub mod hex;

// Crypto
pub mod hash;
pub mod random;
pub mod crypto;

// Time
pub mod time;
pub mod duration;

// Testing
pub mod testing;
pub mod coverage;

// Text processing
pub mod regex;
pub mod fmt;

// Error handling
pub mod error;
pub mod result;

// Functional programming
pub mod itertools;
pub mod functools;

// Profiling
pub mod profiler;

// Database
pub mod database;

// Subprocess
pub mod subprocess;

// CSV
pub mod csv;

// Web Framework
pub mod web;

// Async I/O
pub mod async_io;

// System utilities
pub mod glob;
pub mod tempfile;
pub mod shutil;

// Documentation
pub mod doc;

// XML
pub mod xml;

// Algorithms
pub mod heapq;
pub mod bisect;

// System
pub mod signal;
pub mod mmap;
pub mod timezone;

/// Standard library prelude - commonly used items.
pub mod prelude {
    pub use crate::builtins::*;
    pub use crate::result::{Result, RoastResult};
    pub use crate::error::RoastError;
}

/// Version of the standard library.
pub const VERSION: &str = "0.1.0";

/// Initialize the standard library.
pub fn init() {
    // Initialize any global state
}
