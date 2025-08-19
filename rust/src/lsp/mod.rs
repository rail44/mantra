pub mod client;
mod connection;
pub mod crdt_adapter;
pub mod error;
pub mod formatting;
pub mod notification;
pub mod rpc;
mod transport;

pub use client::{Client, DocumentSymbol};
pub use notification::NotificationHandler;

// Re-export lsp-types for convenient access
pub use lsp_types::{
    Diagnostic, Hover, InitializeResult, Location, MarkupContent, Position,
    PublishDiagnosticsParams, Range, ServerCapabilities, ServerInfo, TextDocumentIdentifier,
    TextDocumentItem, TextEdit,
};

// Re-export our custom types
pub use rpc::{DidOpenTextDocumentParams, HoverParams, InitializeParams, LspRpcClient};
