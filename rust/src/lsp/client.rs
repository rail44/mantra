use anyhow::Result;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::core::traits::ToRpcParams;
use serde::de::Error;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

use crate::lsp::connection::LspConnection;
use crate::lsp::PublishDiagnosticsParams;
use crate::lsp::{Hover, InitializeResult, Position, TextDocumentIdentifier, TextDocumentItem};

// パラメータ構造体をキャメルケース変換付きで定義
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeParams {
    process_id: Option<u32>,
    root_uri: Option<String>,
    capabilities: Value,
    workspace_folders: Option<Vec<Value>>,
}

impl ToRpcParams for InitializeParams {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        let mut params = ObjectParams::new();
        params
            .insert("processId", self.process_id)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params
            .insert("rootUri", self.root_uri)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params
            .insert("capabilities", self.capabilities)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params
            .insert("workspaceFolders", self.workspace_folders)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params.to_rpc_params()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HoverParams {
    text_document: TextDocumentIdentifier,
    position: Position,
}

impl ToRpcParams for HoverParams {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        let mut params = ObjectParams::new();
        params
            .insert("textDocument", self.text_document)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params
            .insert("position", self.position)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params.to_rpc_params()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DidOpenParams {
    text_document: TextDocumentItem,
}

impl ToRpcParams for DidOpenParams {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        let mut params = ObjectParams::new();
        params
            .insert("textDocument", self.text_document)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params.to_rpc_params()
    }
}

/// LSP client that constructs JSON-RPC requests with proper camelCase conversion
#[derive(Clone)]
pub struct Client {
    connection: Arc<LspConnection>,
}

impl Client {
    /// Start a new LSP server and create a client
    pub async fn new(command: &str, args: &[&str]) -> Result<Self> {
        let connection = LspConnection::new(command, args).await?;
        Ok(Self {
            connection: Arc::new(connection),
        })
    }

    /// Initialize the LSP connection
    pub async fn initialize(
        &self,
        process_id: Option<u32>,
        root_uri: Option<String>,
        capabilities: Value,
        workspace_folders: Option<Vec<Value>>,
    ) -> Result<InitializeResult> {
        let params = InitializeParams {
            process_id,
            root_uri,
            capabilities,
            workspace_folders,
        };

        let result = self.connection.client.request("initialize", params).await?;

        Ok(serde_json::from_value(result)?)
    }

    /// Send initialized notification
    pub async fn initialized(&self) -> Result<()> {
        // Empty params for initialized notification
        let params = ObjectParams::new();
        self.connection
            .client
            .notification("initialized", params)
            .await?;
        Ok(())
    }

    /// Wait for diagnostics for a specific URI
    pub async fn wait_for_diagnostics(&self, uri: &str) -> Result<PublishDiagnosticsParams> {
        self.connection
            .notification_handler
            .wait_for_diagnostics(uri)
            .await
    }

    /// Wait for diagnostics with a timeout
    pub async fn wait_for_diagnostics_timeout(
        &self,
        uri: &str,
        timeout: std::time::Duration,
    ) -> Result<PublishDiagnosticsParams> {
        self.connection
            .notification_handler
            .wait_for_diagnostics_timeout(uri, timeout)
            .await
    }

    /// Shutdown the LSP server (consumes self)
    /// Note: Because the client is Clone, ensure all clones are dropped before shutdown
    pub async fn shutdown(self) -> Result<()> {
        // Try to get the inner connection if this is the last reference
        match Arc::try_unwrap(self.connection) {
            Ok(connection) => connection.shutdown().await,
            Err(_) => {
                tracing::warn!("Cannot shutdown LSP server: other references still exist");
                Ok(())
            }
        }
    }

    /// Get hover information at a position
    pub async fn hover(
        &self,
        text_document: TextDocumentIdentifier,
        position: Position,
    ) -> Result<Option<Hover>> {
        let params = HoverParams {
            text_document,
            position,
        };

        let result: Value = self
            .connection
            .client
            .request("textDocument/hover", params)
            .await?;

        // Handle null response as None
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }

    /// Get definition location(s) of a symbol
    pub async fn definition(
        &self,
        text_document: TextDocumentIdentifier,
        position: Position,
    ) -> Result<Option<crate::lsp::Location>> {
        let params = HoverParams {
            text_document,
            position,
        };

        let result: Value = self
            .connection
            .client
            .request("textDocument/definition", params)
            .await?;

        // Handle null response as None
        if result.is_null() {
            Ok(None)
        } else {
            // Could be Location or Location[]
            if let Ok(location) = serde_json::from_value::<crate::lsp::Location>(result.clone()) {
                Ok(Some(location))
            } else if let Ok(locations) =
                serde_json::from_value::<Vec<crate::lsp::Location>>(result)
            {
                Ok(locations.into_iter().next())
            } else {
                Ok(None)
            }
        }
    }

    /// Get type definition location(s) of a symbol
    pub async fn type_definition(
        &self,
        text_document: TextDocumentIdentifier,
        position: Position,
    ) -> Result<Option<crate::lsp::Location>> {
        let params = HoverParams {
            text_document,
            position,
        };

        let result: Value = self
            .connection
            .client
            .request("textDocument/typeDefinition", params)
            .await?;

        // Handle null response as None
        if result.is_null() {
            Ok(None)
        } else {
            // Could be Location or Location[]
            if let Ok(location) = serde_json::from_value::<crate::lsp::Location>(result.clone()) {
                Ok(Some(location))
            } else if let Ok(locations) =
                serde_json::from_value::<Vec<crate::lsp::Location>>(result)
            {
                Ok(locations.into_iter().next())
            } else {
                Ok(None)
            }
        }
    }

    /// Open a text document notification
    pub async fn did_open(&self, text_document: TextDocumentItem) -> Result<()> {
        let params = DidOpenParams { text_document };

        self.connection
            .client
            .notification("textDocument/didOpen", params)
            .await?;
        Ok(())
    }

    pub async fn did_change(
        &self,
        text_document: VersionedTextDocumentIdentifier,
        content_changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<()> {
        let params = DidChangeParams {
            text_document,
            content_changes,
        };

        self.connection
            .client
            .notification("textDocument/didChange", params)
            .await?;
        Ok(())
    }
}

// didChange関連の構造体
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentContentChangeEvent {
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DidChangeParams {
    text_document: VersionedTextDocumentIdentifier,
    content_changes: Vec<TextDocumentContentChangeEvent>,
}

impl ToRpcParams for DidChangeParams {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        let mut params = ObjectParams::new();
        params
            .insert("textDocument", self.text_document)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params
            .insert("contentChanges", self.content_changes)
            .map_err(|e| serde_json::Error::custom(e.to_string()))?;
        params.to_rpc_params()
    }
}
