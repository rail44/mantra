pub mod client;
mod connection;
pub mod notification;
pub mod rpc;
mod transport;

pub use client::{Client, DocumentSymbol};
pub use notification::NotificationHandler;
// Re-export core types
pub use crate::core::types::{Location, Position, Range, TextDocumentIdentifier};

// Re-export LSP-specific types
pub use rpc::{
    Diagnostic, DidOpenTextDocumentParams, Hover, HoverParams, InitializeParams, InitializeResult,
    LspRpcClient, MarkupContent, PublishDiagnosticsParams, ServerCapabilities, ServerInfo,
    TextDocumentItem,
};
