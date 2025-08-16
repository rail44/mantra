use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::document::{DocumentCommand, DocumentManager};
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;

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

    /// Shutdown all Document actors
    pub async fn shutdown(&mut self) {
        for (_, sender) in self.documents.drain() {
            // Send shutdown command (channel will close when actor exits)
            let _ = sender.send(DocumentCommand::Shutdown).await;
        }
    }
}
