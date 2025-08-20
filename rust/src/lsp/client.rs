use anyhow::Result;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::core::traits::ToRpcParams;
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DocumentFormattingParams,
    DocumentRangeFormattingParams, FormattingOptions, GotoCapability, Hover,
    HoverClientCapabilities, InitializeResult, MarkupKind, Position,
    PublishDiagnosticsParams, Range, TextDocumentClientCapabilities, TextDocumentIdentifier,
    TextDocumentSyncClientCapabilities, TextEdit, Uri, WorkDoneProgressParams, WorkspaceFolder,
};
use serde::de::Error;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

use crate::lsp::connection::LspConnection;

/// 中間trait：Serializeできる型をRPCパラメータに変換
trait LspParams: Serialize + Send {
    fn to_object_params(self) -> Result<ObjectParams, serde_json::Error>;
}

// Serializeを実装している全ての型に対してLspParamsを実装
impl<T: Serialize + Send> LspParams for T {
    fn to_object_params(self) -> Result<ObjectParams, serde_json::Error> {
        let value = serde_json::to_value(self)?;
        let mut params = ObjectParams::new();
        if let Value::Object(map) = value {
            for (key, value) in map {
                params
                    .insert(&key, value)
                    .map_err(|e| serde_json::Error::custom(e.to_string()))?;
            }
        }
        Ok(params)
    }
}

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
    text_document: lsp_types::TextDocumentItem,
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
#[derive(Clone, Debug)]
pub struct Client {
    connection: Arc<LspConnection>,
    server_capabilities: Arc<tokio::sync::RwLock<Option<lsp_types::ServerCapabilities>>>,
}

impl Client {
    /// Start a new LSP server and create a client
    pub async fn new(command: &str, args: &[&str]) -> Result<Self> {
        let connection = LspConnection::new(command, args).await?;
        Ok(Self {
            connection: Arc::new(connection),
            server_capabilities: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// Get default client capabilities
    pub fn default_capabilities() -> ClientCapabilities {
        ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                hover: Some(HoverClientCapabilities {
                    content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    ..Default::default()
                }),
                synchronization: Some(TextDocumentSyncClientCapabilities {
                    dynamic_registration: Some(false),
                    will_save: Some(false),
                    will_save_wait_until: Some(false),
                    did_save: Some(true),
                }),
                definition: Some(GotoCapability {
                    dynamic_registration: Some(false),
                    ..Default::default()
                }),
                type_definition: Some(GotoCapability {
                    dynamic_registration: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Create default workspace folders
    pub fn default_workspace_folders(workspace_uri: &str) -> Result<Vec<WorkspaceFolder>> {
        let url: Uri = workspace_uri.parse()?;
        Ok(vec![WorkspaceFolder {
            uri: url,
            name: "workspace".to_string(),
        }])
    }

    /// Initialize the LSP connection
    pub async fn initialize(
        &self,
        process_id: Option<u32>,
        root_uri: Option<String>,
        capabilities: ClientCapabilities,
        workspace_folders: Option<Vec<WorkspaceFolder>>,
    ) -> Result<InitializeResult> {
        // Convert to Value for JSON-RPC
        let capabilities_value = serde_json::to_value(capabilities)?;
        let workspace_folders_value = workspace_folders
            .map(|folders| {
                folders
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let params = InitializeParams {
            process_id,
            root_uri,
            capabilities: capabilities_value,
            workspace_folders: workspace_folders_value,
        };

        let result = self.connection.client.request("initialize", params).await?;
        let init_result: InitializeResult = serde_json::from_value(result)?;

        // Save server capabilities
        *self.server_capabilities.write().await = Some(init_result.capabilities.clone());

        Ok(init_result)
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

    /// Check if the server supports range formatting
    pub async fn supports_range_formatting(&self) -> bool {
        let capabilities = self.server_capabilities.read().await;
        if let Some(caps) = capabilities.as_ref() {
            if let Some(doc_formatting) = &caps.document_range_formatting_provider {
                match doc_formatting {
                    lsp_types::OneOf::Left(supported) => *supported,
                    lsp_types::OneOf::Right(_) => true,
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Check if the server supports document formatting
    pub async fn supports_document_formatting(&self) -> bool {
        let capabilities = self.server_capabilities.read().await;
        if let Some(caps) = capabilities.as_ref() {
            if let Some(doc_formatting) = &caps.document_formatting_provider {
                match doc_formatting {
                    lsp_types::OneOf::Left(supported) => *supported,
                    lsp_types::OneOf::Right(_) => true,
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Format a document using LSP
    pub async fn format_document(
        &self,
        text_document: TextDocumentIdentifier,
        options: FormattingOptions,
    ) -> Result<Option<Vec<TextEdit>>> {
        let params = DocumentFormattingParams {
            text_document,
            options,
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
        };

        let result: Value = self
            .connection
            .client
            .request("textDocument/formatting", params.to_object_params()?)
            .await?;

        // Handle null response as None
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }

    /// Format a range of a document using LSP
    pub async fn range_formatting(
        &self,
        text_document: TextDocumentIdentifier,
        range: Range,
        options: FormattingOptions,
    ) -> Result<Option<Vec<TextEdit>>> {
        let params = DocumentRangeFormattingParams {
            text_document,
            range,
            options,
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
        };

        let result: Value = self
            .connection
            .client
            .request("textDocument/rangeFormatting", params.to_object_params()?)
            .await?;

        // Handle null response as None
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }

    /// Open a text document notification
    pub async fn did_open(&self, text_document: lsp_types::TextDocumentItem) -> Result<()> {
        let params = DidOpenParams { text_document };

        self.connection
            .client
            .notification("textDocument/didOpen", params)
            .await?;
        Ok(())
    }

    pub async fn did_change(&self, params: DidChangeTextDocumentParams) -> Result<()> {
        self.connection
            .client
            .notification("textDocument/didChange", params.to_object_params()?)
            .await?;
        Ok(())
    }
}

