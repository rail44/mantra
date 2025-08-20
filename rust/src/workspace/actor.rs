use actix::prelude::*;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;

use super::document::Document;
use super::messages::*;

// ============================================================================
// Helper Functions
// ============================================================================

// ============================================================================
// Actor Definition
// ============================================================================

/// Actix-based Workspace actor (renamed from WorkspaceActor for clarity)
pub struct Workspace {
    /// Documents by file URI
    documents: HashMap<String, Document>,
    /// LSP client
    lsp_client: LspClient,
    /// LLM client
    llm_client: LLMClient,
    /// Workspace root directory
    root_dir: PathBuf,
    /// Configuration
    config: Config,
}

impl Workspace {
    /// Create a new workspace actor
    pub async fn new(root_dir: PathBuf, config: Config) -> Result<Self> {
        // Initialize LSP client
        let lsp_client = LspClient::new("gopls", &[]).await?;

        // Initialize workspace with LSP
        let workspace_uri = format!("file://{}", root_dir.display());
        let capabilities = LspClient::default_capabilities();
        let workspace_folders = LspClient::default_workspace_folders(&workspace_uri)?;

        lsp_client
            .initialize(
                Some(std::process::id()),
                Some(workspace_uri.clone()),
                capabilities,
                Some(workspace_folders),
            )
            .await?;
        lsp_client.initialized().await?;

        // Create LLM client
        let llm_client = LLMClient::new(config.clone())?;

        Ok(Self {
            documents: HashMap::new(),
            lsp_client,
            llm_client,
            root_dir,
            config,
        })
    }

    /// Create and start a Workspace actor
    pub async fn start_actor(root_dir: PathBuf, config: Config) -> Result<Addr<Self>> {
        let workspace = Self::new(root_dir, config).await?;
        Ok(workspace.start())
    }
}

impl Actor for Workspace {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Workspace actor started for: {}", self.root_dir.display());
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Workspace actor stopped");
    }
}

impl Supervised for Workspace {
    fn restarting(&mut self, _ctx: &mut Context<Self>) {
        error!("Workspace actor crashed and is restarting");
        // Clear documents on restart
        self.documents.clear();
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

impl Handler<GetLspClient> for Workspace {
    type Result = MessageResult<GetLspClient>;

    fn handle(&mut self, _msg: GetLspClient, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("GetLspClient requested");
        MessageResult(self.lsp_client.clone())
    }
}

impl Handler<GetLlmClient> for Workspace {
    type Result = MessageResult<GetLlmClient>;

    fn handle(&mut self, _msg: GetLlmClient, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("GetLlmClient requested");
        MessageResult(self.llm_client.clone())
    }
}

// GetDocumentハンドラを削除（直接GenerateFileで処理）

impl Handler<GenerateFile> for Workspace {
    type Result = ResponseActFuture<Self, Result<String>>;

    fn handle(&mut self, msg: GenerateFile, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("GenerateFile: {}", msg.file_path.display());

        // Convert file path to URI
        let absolute_path = if msg.file_path.is_absolute() {
            msg.file_path
        } else {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(&msg.file_path),
                Err(e) => {
                    let err_msg = format!("Failed to get current directory: {}", e);
                    error!("{}", err_msg);
                    return Box::pin(async move { Err(anyhow::anyhow!(err_msg)) }.into_actor(self));
                }
            }
        };

        // Validate file exists
        if !absolute_path.exists() {
            let err_msg = format!("File does not exist: {}", absolute_path.display());
            error!("{}", err_msg);
            return Box::pin(async move { Err(anyhow::anyhow!(err_msg)) }.into_actor(self));
        }

        let file_uri = format!("file://{}", absolute_path.display());

        // For existing documents, we need to recreate them to generate
        // (because we can't safely borrow mutable in async context)

        // Create new document
        let config = self.config.clone();
        let lsp_client = self.lsp_client.clone();
        let llm_client = self.llm_client.clone();

        Box::pin(
            async move {
                // Read file content
                let source = tokio::fs::read_to_string(&absolute_path).await?;

                // Create document
                let mut document = Document::new(
                    config,
                    absolute_path.clone(),
                    file_uri.clone(),
                    lsp_client.clone(),
                    llm_client,
                )
                .await?;

                // Open document in LSP
                let doc_uri: lsp_types::Uri = file_uri.parse()?;
                lsp_client
                    .did_open(lsp_types::TextDocumentItem {
                        uri: doc_uri,
                        language_id: "go".to_string(),
                        version: 1,
                        text: source,
                    })
                    .await?;

                // Generate code
                let result = document.generate_all().await?;

                Ok((file_uri, document, result))
            }
            .into_actor(self)
            .map(|result, actor, _ctx| {
                match result {
                    Ok((uri, doc, generated)) => {
                        // Store the document
                        actor.documents.insert(uri, doc);
                        Ok(generated)
                    }
                    Err(e) => Err(e),
                }
            }),
        )
    }
}

impl Handler<Shutdown> for Workspace {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _msg: Shutdown, _ctx: &mut Context<Self>) -> Self::Result {
        info!("Shutting down Workspace actor");

        // Clear all document actors
        self.documents.clear();

        // Clone LSP client for shutdown
        let lsp_client = self.lsp_client.clone();

        Box::pin(
            async move {
                // Small delay to ensure document actors have released their references
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                if let Err(e) = lsp_client.shutdown().await {
                    error!("Failed to shutdown LSP client: {}", e);
                }
            }
            .into_actor(self)
            .map(|_, _act, ctx| {
                // Stop self after LSP shutdown
                ctx.stop();
            }),
        )
    }
}
