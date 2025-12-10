//! Language Server Protocol implementation for Roast.

pub mod server;
pub mod capabilities;
pub mod analysis;

pub use server::RoastLanguageServer;
pub use analysis::{Analyzer, DocumentAnalysis, SymbolInfo, SymbolKind, InlayHintInfo, InlayHintKind};

