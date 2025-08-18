use jsonrpsee::proc_macros::rpc;
use lsp_types::{Position, TextDocumentIdentifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "processId")]
    pub process_id: Option<u32>,
    #[serde(rename = "rootUri")]
    pub root_uri: Option<String>,
    pub capabilities: Value,
    #[serde(rename = "workspaceFolders")]
    pub workspace_folders: Option<Vec<Value>>,
}

// Kept for backward compatibility, will be removed later
pub type TextDocumentItem = lsp_types::TextDocumentItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

// Use lsp-types version
pub type InitializeResult = lsp_types::InitializeResult;

// Use lsp-types version
pub type ServerCapabilities = lsp_types::ServerCapabilities;
pub type ServerInfo = lsp_types::ServerInfo;

// Use lsp-types version
pub type Hover = lsp_types::Hover;
pub type MarkupContent = lsp_types::MarkupContent;

// Use lsp-types versions
pub type Diagnostic = lsp_types::Diagnostic;
pub type DiagnosticRelatedInformation = lsp_types::DiagnosticRelatedInformation;
pub type PublishDiagnosticsParams = lsp_types::PublishDiagnosticsParams;

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
    ) -> Result<InitializeResult, jsonrpsee::core::Error>;

    /// Send initialized notification (no return = notification in LSP spec)
    #[method(name = "initialized", param_kind = map)]
    async fn initialized(&self);

    /// Get hover information at a position
    #[method(name = "textDocument/hover", param_kind = map)]
    async fn hover(
        &self,
        text_document: TextDocumentIdentifier,
        position: Position,
    ) -> Result<Option<Hover>, jsonrpsee::core::Error>;

    /// Open a text document notification (no return = notification in LSP spec)
    #[method(name = "textDocument/didOpen", param_kind = map)]
    async fn did_open(&self, text_document: TextDocumentItem);
}
