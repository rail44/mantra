use jsonrpsee::proc_macros::rpc;
use lsp_types::{Position, TextDocumentIdentifier};
use serde_json::Value;

// Use lsp-types directly in function signatures

/// Define LSP RPC interface using proc macro
/// This generates type-safe client methods automatically
#[rpc(client)]
pub trait LspRpc {
    /// Initialize the LSP connection
    #[method(name = "initialize", param_kind = map)]
    async fn initialize(
        &self,
        process_id: Option<u32>,
        root_uri: Option<String>,
        capabilities: Value,
        workspace_folders: Option<Vec<Value>>,
    ) -> Result<lsp_types::InitializeResult, jsonrpsee::core::Error>;

    /// Send initialized notification (no return = notification in LSP spec)
    #[method(name = "initialized", param_kind = map)]
    async fn initialized(&self);

    /// Get hover information at a position
    #[method(name = "textDocument/hover", param_kind = map)]
    async fn hover(
        &self,
        text_document: TextDocumentIdentifier,
        position: Position,
    ) -> Result<Option<lsp_types::Hover>, jsonrpsee::core::Error>;

    /// Open a text document notification (no return = notification in LSP spec)
    #[method(name = "textDocument/didOpen", param_kind = map)]
    async fn did_open(&self, text_document: lsp_types::TextDocumentItem);
}
