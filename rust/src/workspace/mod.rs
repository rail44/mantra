use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::document::{DocumentCommand, DocumentManager};
use crate::llm::LLMClient;
use crate::lsp::{Client as LspClient, Range};
use crate::tools::inspect::{InspectRequest, InspectResponse, InspectTool};

#[cfg(test)]
mod tests;

/// Commands that can be sent to a Workspace actor
pub enum WorkspaceCommand {
    /// Get or create a Document actor
    GetDocument {
        uri: String,
        response: oneshot::Sender<Result<mpsc::Sender<DocumentCommand>>>,
    },
    /// Get LSP client
    GetLspClient {
        response: oneshot::Sender<LspClient>,
    },
    /// Get LLM client
    GetLlmClient {
        response: oneshot::Sender<LLMClient>,
    },
    /// Register a scope in InspectTool
    RegisterScope {
        uri: String,
        range: Range,
        response: oneshot::Sender<String>,
    },
    /// Inspect a symbol
    InspectSymbol {
        request: InspectRequest,
        response: oneshot::Sender<Result<InspectResponse>>,
    },
    /// Generate code for a document
    GenerateForDocument {
        file_uri: String,
        response: oneshot::Sender<Result<String>>,
    },
    /// Generate code for a file
    GenerateFile {
        file_path: PathBuf,
        response: oneshot::Sender<Result<String>>,
    },
    /// Shutdown the workspace
    Shutdown,
}

/// Workspace manages Document actors and provides access to LSP/LLM clients
pub struct Workspace {
    /// Document actors by file URI
    documents: HashMap<String, mpsc::Sender<DocumentCommand>>,
    /// LSP client
    lsp_client: LspClient,
    /// LLM client
    llm_client: LLMClient,
    /// Workspace root directory
    _root_dir: PathBuf,
    /// Configuration
    config: Config,
    /// Inspect tool for symbol inspection
    inspect_tool: InspectTool,
    /// Self sender for workspace commands
    self_tx: mpsc::Sender<WorkspaceCommand>,
}

impl Workspace {
    /// Create a new workspace (internal constructor)
    async fn new(
        root_dir: PathBuf,
        config: Config,
        self_tx: mpsc::Sender<WorkspaceCommand>,
    ) -> Result<Self> {
        // Initialize LSP client
        let lsp_client = LspClient::new("gopls", &[]).await?;

        // Initialize workspace with LSP
        let workspace_uri = format!("file://{}", root_dir.display());
        lsp_client
            .initialize(
                Some(std::process::id()),
                Some(workspace_uri.clone()),
                serde_json::json!({
                    "textDocument": {
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "synchronization": {
                            "didOpen": true,
                            "didChange": true
                        },
                        "definition": {
                            "dynamicRegistration": false
                        },
                        "typeDefinition": {
                            "dynamicRegistration": false
                        }
                    }
                }),
                Some(vec![serde_json::json!({
                    "uri": workspace_uri,
                    "name": "workspace"
                })]),
            )
            .await?;
        lsp_client.initialized().await?;

        // Create LLM client
        let llm_client = LLMClient::new(config.clone())?;

        Ok(Self {
            documents: HashMap::new(),
            lsp_client,
            llm_client,
            _root_dir: root_dir,
            config,
            inspect_tool: InspectTool::new(),
            self_tx,
        })
    }

    /// Spawn a new workspace actor and return the sender for commands
    pub async fn spawn(
        root_dir: PathBuf,
        config: Config,
    ) -> Result<mpsc::Sender<WorkspaceCommand>> {
        let (tx, rx) = mpsc::channel(32);
        let self_tx = tx.clone();
        let actor_tx = tx.clone();

        let mut workspace = Self::new(root_dir, config, self_tx).await?;

        tokio::spawn(async move {
            if let Err(e) = workspace.run_actor(actor_tx, rx).await {
                tracing::error!("Workspace actor failed: {}", e);
            }
        });

        Ok(tx)
    }

    /// Get or create a Document actor for the given URI
    pub async fn get_document(&mut self, uri: &str) -> Result<mpsc::Sender<DocumentCommand>> {
        if let Some(sender) = self.documents.get(uri) {
            // Check if the actor is still alive
            if !sender.is_closed() {
                return Ok(sender.clone());
            }
            // Remove dead actor
            self.documents.remove(uri);
        }

        // Parse file path from URI
        let file_path = if let Some(stripped) = uri.strip_prefix("file://") {
            PathBuf::from(stripped)
        } else {
            PathBuf::from(uri)
        };

        // Spawn Document actor using the new spawn method
        let config = self.config.clone();
        let workspace_tx = self.self_tx.clone();
        let tx = DocumentManager::spawn(config, &file_path, workspace_tx).await?;

        // Store sender
        self.documents.insert(uri.to_string(), tx.clone());

        // Open the document in LSP
        let source = std::fs::read_to_string(&file_path)?;
        self.lsp_client
            .did_open(crate::lsp::TextDocumentItem {
                uri: uri.to_string(),
                language_id: "go".to_string(),
                version: 1,
                text: source,
            })
            .await?;

        Ok(tx)
    }

    /// Get reference to the LSP client
    pub fn lsp_client(&self) -> &LspClient {
        &self.lsp_client
    }

    /// Get reference to the LLM client  
    pub fn llm_client(&self) -> &LLMClient {
        &self.llm_client
    }

    /// Get mutable reference to the inspect tool
    pub fn inspect_tool_mut(&mut self) -> &mut InspectTool {
        &mut self.inspect_tool
    }

    /// Shutdown all Document actors
    async fn shutdown(&mut self) {
        for (_, sender) in self.documents.drain() {
            // Send shutdown command (channel will close when actor exits)
            let _ = sender.send(DocumentCommand::Shutdown).await;
        }
    }

    /// Run the actor's event loop (internal)
    async fn run_actor(
        &mut self,
        tx: mpsc::Sender<WorkspaceCommand>,
        mut rx: mpsc::Receiver<WorkspaceCommand>,
    ) -> Result<()> {
        while let Some(command) = rx.recv().await {
            match command {
                WorkspaceCommand::GetDocument { uri, response } => {
                    let result = self.get_document(&uri).await;
                    let _ = response.send(result);
                }
                WorkspaceCommand::GetLspClient { response } => {
                    let _ = response.send(self.lsp_client.clone());
                }
                WorkspaceCommand::GetLlmClient { response } => {
                    let _ = response.send(self.llm_client.clone());
                }
                WorkspaceCommand::RegisterScope {
                    uri,
                    range,
                    response,
                } => {
                    let scope_id = self.inspect_tool.register_scope(uri, range);
                    let _ = response.send(scope_id);
                }
                WorkspaceCommand::InspectSymbol { request, response } => {
                    // Clone InspectTool and Workspace sender for the task
                    let mut inspect_tool = self.inspect_tool.clone();
                    let workspace_tx = tx.clone(); // We need tx defined outside the loop

                    // Spawn inspection as a separate task
                    tokio::spawn(async move {
                        let result = inspect_tool.inspect(request, workspace_tx).await;
                        let _ = response.send(result);
                    });
                }
                WorkspaceCommand::GenerateForDocument { file_uri, response } => {
                    // Get the document
                    let document = match self.get_document(&file_uri).await {
                        Ok(doc) => doc,
                        Err(e) => {
                            let _ = response.send(Err(e));
                            continue;
                        }
                    };

                    // Spawn generation task to avoid blocking the actor loop
                    tokio::spawn(async move {
                        let (gen_tx, gen_rx) = oneshot::channel();
                        let send_result = document
                            .send(DocumentCommand::GenerateAll { response: gen_tx })
                            .await;

                        let result = match send_result {
                            Ok(_) => gen_rx.await.unwrap_or_else(|e| {
                                Err(anyhow::anyhow!("Failed to receive response: {}", e))
                            }),
                            Err(e) => Err(anyhow::anyhow!("Failed to send command: {}", e)),
                        };

                        let _ = response.send(result);
                    });
                }
                WorkspaceCommand::GenerateFile {
                    file_path,
                    response,
                } => {
                    // Convert file path to URI
                    let absolute_path = if file_path.is_absolute() {
                        file_path
                    } else {
                        std::env::current_dir()?.join(&file_path)
                    };
                    let file_uri = format!("file://{}", absolute_path.display());

                    // Get the document
                    let document = match self.get_document(&file_uri).await {
                        Ok(doc) => doc,
                        Err(e) => {
                            let _ = response.send(Err(e));
                            continue;
                        }
                    };

                    // Spawn generation task to avoid blocking the actor loop
                    tokio::spawn(async move {
                        let (gen_tx, gen_rx) = oneshot::channel();
                        let send_result = document
                            .send(DocumentCommand::GenerateAll { response: gen_tx })
                            .await;

                        let result = match send_result {
                            Ok(_) => gen_rx.await.unwrap_or_else(|e| {
                                Err(anyhow::anyhow!("Failed to receive response: {}", e))
                            }),
                            Err(e) => Err(anyhow::anyhow!("Failed to send command: {}", e)),
                        };

                        let _ = response.send(result);
                    });
                }
                WorkspaceCommand::Shutdown => {
                    self.shutdown().await;
                    break;
                }
            }
        }
        Ok(())
    }

    /// Process inspect symbol request (unused now as inspection runs in separate task)
    #[allow(dead_code)]
    async fn inspect_symbol(&mut self, request: InspectRequest) -> Result<InspectResponse> {
        // Create a channel to communicate with InspectTool
        let (tx, _rx) = mpsc::channel(32);

        // Clone the sender for InspectTool to use
        let workspace_sender = tx.clone();

        // Spawn a task to handle InspectTool's requests
        let inspect_future = self.inspect_tool.inspect(request, workspace_sender);

        // Process InspectTool's requests while it runs
        // This is a simplified approach - in production you might want a more sophisticated solution
        inspect_future.await
    }
}
