use anyhow::Result;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::core::traits::ToRpcParams;
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DocumentFormattingParams, FormattingOptions,
    GotoCapability, GotoDefinitionResponse, HoverClientCapabilities, InitializeResult, Location,
    LocationLink, MarkupKind, TextDocumentClientCapabilities, TextDocumentIdentifier,
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
    pub fn new(command: &str, args: &[&str]) -> Result<Self> {
        let connection = LspConnection::new(command, args)?;
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

    /// Close a text document notification
    pub async fn did_close(&self, text_document: TextDocumentIdentifier) -> Result<()> {
        let params = lsp_types::DidCloseTextDocumentParams { text_document };

        self.connection
            .client
            .notification("textDocument/didClose", params.to_object_params()?)
            .await?;
        Ok(())
    }

    /// Get definition location(s) for a symbol at a position
    pub async fn definition(
        &self,
        text_document: TextDocumentIdentifier,
        position: lsp_types::Position,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct DefinitionParams {
            text_document: TextDocumentIdentifier,
            position: lsp_types::Position,
        }

        let params = DefinitionParams {
            text_document,
            position,
        };

        let result: Value = self
            .connection
            .client
            .request("textDocument/definition", params.to_object_params()?)
            .await?;

        // Handle null response as None
        if result.is_null() {
            Ok(None)
        } else {
            // Parse the various response formats
            parse_goto_definition_response(&result).map(Some)
        }
    }
}

/// Parse `GotoDefinitionResponse` from a JSON value
/// Handles the three possible formats: Location, Location[], `LocationLink`[]
fn parse_goto_definition_response(value: &Value) -> Result<GotoDefinitionResponse> {
    // First try to parse as a single Location
    if let Ok(location) = serde_json::from_value::<Location>(value.clone()) {
        return Ok(GotoDefinitionResponse::Scalar(location));
    }

    // Then try to parse as an array
    if let Ok(array) = serde_json::from_value::<Vec<Value>>(value.clone()) {
        if array.is_empty() {
            return Ok(GotoDefinitionResponse::Array(vec![]));
        }

        // Check if the first element looks like a LocationLink (has targetUri field)
        if let Some(first) = array.first() {
            if first.get("targetUri").is_some() {
                // Try to parse as LocationLink array
                if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(value.clone()) {
                    return Ok(GotoDefinitionResponse::Link(links));
                }
            }
        }

        // Otherwise try to parse as Location array
        if let Ok(locations) = serde_json::from_value::<Vec<Location>>(value.clone()) {
            return Ok(GotoDefinitionResponse::Array(locations));
        }
    }

    // If all parsing attempts fail, return an error
    Err(anyhow::anyhow!(
        "Failed to parse GotoDefinitionResponse: unexpected format"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_goto_definition_response_single_location() {
        let json = serde_json::json!({
            "uri": "file:///path/to/file.go",
            "range": {
                "start": {"line": 10, "character": 5},
                "end": {"line": 10, "character": 10}
            }
        });

        let result = parse_goto_definition_response(&json).unwrap();
        match result {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri.as_str(), "file:///path/to/file.go");
                assert_eq!(location.range.start.line, 10);
            }
            _ => panic!("Expected Scalar variant"),
        }
    }

    #[test]
    fn test_parse_goto_definition_response_location_array() {
        let json = serde_json::json!([
            {
                "uri": "file:///path/to/file1.go",
                "range": {
                    "start": {"line": 10, "character": 5},
                    "end": {"line": 10, "character": 10}
                }
            },
            {
                "uri": "file:///path/to/file2.go",
                "range": {
                    "start": {"line": 20, "character": 15},
                    "end": {"line": 20, "character": 20}
                }
            }
        ]);

        let result = parse_goto_definition_response(&json).unwrap();
        match result {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 2);
                assert_eq!(locations[0].uri.as_str(), "file:///path/to/file1.go");
                assert_eq!(locations[1].uri.as_str(), "file:///path/to/file2.go");
            }
            _ => panic!("Expected Array variant"),
        }
    }

    #[test]
    fn test_parse_goto_definition_response_location_link_array() {
        let json = serde_json::json!([
            {
                "originSelectionRange": {
                    "start": {"line": 5, "character": 10},
                    "end": {"line": 5, "character": 15}
                },
                "targetUri": "file:///path/to/target.go",
                "targetRange": {
                    "start": {"line": 100, "character": 0},
                    "end": {"line": 110, "character": 0}
                },
                "targetSelectionRange": {
                    "start": {"line": 100, "character": 5},
                    "end": {"line": 100, "character": 20}
                }
            }
        ]);

        let result = parse_goto_definition_response(&json).unwrap();
        match result {
            GotoDefinitionResponse::Link(links) => {
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].target_uri.as_str(), "file:///path/to/target.go");
                assert_eq!(links[0].target_range.start.line, 100);
                assert_eq!(links[0].target_range.end.line, 110);
                assert!(links[0].origin_selection_range.is_some());
            }
            _ => panic!("Expected Link variant"),
        }
    }

    #[test]
    fn test_parse_goto_definition_response_empty_array() {
        let json = serde_json::json!([]);
        let result = parse_goto_definition_response(&json).unwrap();
        match result {
            GotoDefinitionResponse::Array(locations) => {
                assert!(locations.is_empty());
            }
            _ => panic!("Expected Array variant with empty vector"),
        }
    }
}
