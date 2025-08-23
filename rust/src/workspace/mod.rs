pub mod document;
pub mod service;

pub use self::document::{Document, DocumentService, GetContent, StartGeneration};
pub use self::service::{Client as ServiceClient, Service};

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;

/// Workspace managing documents and services
pub struct Workspace {
    /// Documents by file URI
    documents: HashMap<String, ServiceClient<document::DocumentService>>,
    /// LSP client
    lsp_client: LspClient,
    /// LLM client
    llm_client: LLMClient,
    /// Configuration (kept for potential future use)
    _config: Config,
}

impl Workspace {
    /// Create a new workspace
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
            _config: config,
        })
    }

    /// Generate code for a file
    pub async fn generate_file(&mut self, file_path: PathBuf) -> Result<String> {
        debug!("GenerateFile: {}", file_path.display());

        // Convert file path to absolute
        let absolute_path = if file_path.is_absolute() {
            file_path
        } else {
            std::env::current_dir()?.join(&file_path)
        };

        // Validate file exists
        if !absolute_path.exists() {
            return Err(anyhow::anyhow!(
                "File does not exist: {}",
                absolute_path.display()
            ));
        }

        let file_uri = format!("file://{}", absolute_path.display());

        // Check if document already exists
        if let Some(client) = self.documents.get_mut(&file_uri) {
            client.request(StartGeneration).await;
            return Ok(client.request(GetContent).await);
        }

        // Read file content
        let source = tokio::fs::read_to_string(&absolute_path).await?;

        // Open document in LSP
        let doc_uri: lsp_types::Uri = file_uri.parse()?;
        self.lsp_client
            .did_open(lsp_types::TextDocumentItem {
                uri: doc_uri,
                language_id: "go".to_string(),
                version: 1,
                text: source,
            })
            .await?;

        // Create document service
        let document_service = document::DocumentService::new(
            absolute_path.clone(),
            file_uri.clone(),
            self.lsp_client.clone(),
            self.llm_client.clone(),
        )
        .await?;
        let client = document_service.spawn();

        // Start generation
        client.request(StartGeneration).await;
        let result = client.request(GetContent).await;

        // Store the document
        self.documents.insert(file_uri, client);

        Ok(result)
    }

    /// Shutdown the workspace
    pub async fn shutdown(self) -> Result<()> {
        info!("Shutting down Workspace");

        // Clear all documents
        drop(self.documents);

        // Small delay to ensure documents have released their references
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Shutdown LSP client
        if let Err(e) = self.lsp_client.shutdown().await {
            error!("Failed to shutdown LSP client: {}", e);
        }

        Ok(())
    }
}
