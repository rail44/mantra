use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentItem {
    pub uri: String,
    #[serde(rename = "languageId")]
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: Option<ServerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(rename = "hoverProvider", default)]
    pub hover_provider: bool,
    #[serde(rename = "textDocumentSync", default)]
    pub text_document_sync: Option<Value>,
    // 他のcapabilitiesは必要に応じて追加
    #[serde(flatten)]
    pub other: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: MarkupContent,
    pub range: Option<Range>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarkupContent {
    PlainText(String),
    Markdown {
        kind: String,
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Define LSP RPC interface using proc macro
/// This generates type-safe client methods automatically
#[rpc(client)]
pub trait LspRpc {
    /// Initialize the LSP connection
    #[method(name = "initialize", param_kind = map)]
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult, jsonrpsee::core::Error>;

    /// Send initialized notification
    #[method(name = "initialized", param_kind = map)]
    async fn initialized(&self) -> Result<(), jsonrpsee::core::Error>;

    /// Get hover information at a position
    #[method(name = "textDocument/hover", param_kind = map)]
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>, jsonrpsee::core::Error>;

    /// Open a text document
    #[method(name = "textDocument/didOpen", param_kind = map)]
    async fn did_open(
        &self,
        params: DidOpenTextDocumentParams,
    ) -> Result<(), jsonrpsee::core::Error>;
}
