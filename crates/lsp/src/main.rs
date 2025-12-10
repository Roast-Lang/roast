//! Roast Language Server (roast-lsp).

use roast_lsp::RoastLanguageServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| RoastLanguageServer::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

