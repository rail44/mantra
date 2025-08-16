use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::document::{DocumentCommand, DocumentManager};
use crate::generation::TargetGenerator;
use crate::llm::LLMClient;
use crate::lsp::{Client as LspClient, Range};
use crate::parser::target_map::TargetMap;
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
}

impl Workspace {
    /// Create a new workspace
    pub async fn new(root_dir: PathBuf, config: Config) -> Result<Self> {
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
        })
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

        // Create new Document actor
        let (tx, rx) = mpsc::channel(32);

        // Parse file path from URI
        let file_path = if let Some(stripped) = uri.strip_prefix("file://") {
            PathBuf::from(stripped)
        } else {
            PathBuf::from(uri)
        };

        // Spawn Document actor
        let config = self.config.clone();
        tokio::spawn(async move {
            let mut manager = DocumentManager::new(config, &file_path).await?;
            manager.run_actor(rx).await
        });

        // Store sender
        self.documents.insert(uri.to_string(), tx.clone());

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

    /// Generate code for a document
    pub async fn generate_for_document(&mut self, file_uri: &str) -> Result<String> {
        // Get document actor
        let document = self.get_document(file_uri).await?;

        // Get source code and tree
        let (source_tx, source_rx) = oneshot::channel();
        document
            .send(DocumentCommand::GetSource {
                response: source_tx,
            })
            .await?;
        let source = source_rx.await??;

        let (tree_tx, tree_rx) = oneshot::channel();
        document
            .send(DocumentCommand::GetTree { response: tree_tx })
            .await?;
        let tree = tree_rx.await??;

        // Build target map
        let target_map = TargetMap::build(&tree, &source)?;
        let package_name = target_map.package_name().to_string();

        tracing::info!("Found {} targets in file", target_map.len());
        for target in target_map.targets() {
            tracing::info!("  - {} ({})", target.name, target.instruction);
        }

        // Generate for each target
        let mut generated_results = Vec::new();

        for (target, node) in target_map.targets().zip(target_map.nodes()) {
            // Create target generator with file URI
            let _target_generator =
                TargetGenerator::new(target, &package_name, *node, file_uri.to_string());

            // Generate code using Workspace (need tx here)
            // This method should not be called directly, but through WorkspaceCommand
            let generated_code = String::new(); // Placeholder

            generated_results.push((target.clone(), generated_code));

            tracing::debug!("Generated code for target: {}", target.name);
        }

        // Apply generated code back to document
        // For now, just return the source (actual application would be done in DocumentManager)
        Ok(source.clone())
    }

    /// Shutdown all Document actors
    async fn shutdown(&mut self) {
        for (_, sender) in self.documents.drain() {
            // Send shutdown command (channel will close when actor exits)
            let _ = sender.send(DocumentCommand::Shutdown).await;
        }
    }

    /// Run the actor's event loop
    pub async fn run_actor(
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
                WorkspaceCommand::GenerateForDocument {
                    file_uri: _,
                    response,
                } => {
                    // Clone necessary data for the task
                    let _workspace_tx = tx.clone();

                    // Spawn generation as a separate task
                    tokio::spawn(async move {
                        // Use workspace_tx to communicate with Workspace
                        // For now, return placeholder
                        let result = Ok("Generation not yet implemented".to_string());
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
