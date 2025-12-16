pub use crate::document::{Document, DocumentService};

use anyhow::Result;
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::config::Config;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;

/// Workspace managing documents (state only)
pub struct Workspace {
    /// Documents by file URI
    documents: FxHashMap<String, DocumentService>,
}

/// Service wrapper for Workspace with external dependencies  
#[derive(Clone)]
pub struct WorkspaceService {
    workspace: Arc<RwLock<Workspace>>,
    /// LSP client
    lsp_client: LspClient,
    /// LLM client
    llm_client: LLMClient,
}

impl Workspace {
    /// Create a new empty workspace
    pub fn new() -> Self {
        Self {
            documents: FxHashMap::default(),
        }
    }
}

impl WorkspaceService {
    /// Create a new workspace service
    pub async fn new(root_dir: PathBuf, config: Config) -> Result<Self> {
        // Initialize LSP client
        let lsp_client = LspClient::new("gopls", &[])?;

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

        // Create workspace
        let workspace = Workspace::new();

        Ok(Self {
            workspace: Arc::new(RwLock::new(workspace)),
            lsp_client,
            llm_client,
        })
    }
}

impl WorkspaceService {
    /// Open a document with provided text (from editor), creating if not exists
    pub async fn open_document_with_text(&self, uri: &str, text: &str) -> Result<DocumentService> {
        // Check if document already exists
        if let Some(document) = self.get_document(uri) {
            return Ok(document);
        }

        let parsed_uri: lsp_types::Uri = uri.parse()?;

        // Open document in gopls
        self.lsp_client
            .did_open(lsp_types::TextDocumentItem {
                uri: parsed_uri,
                language_id: "go".to_string(),
                version: 1,
                text: text.to_string(),
            })
            .await?;

        // Create document from provided text (not from disk)
        let d = Document::from_text(uri.to_string(), text)?;
        let document = DocumentService::new(
            d,
            self.lsp_client.clone(),
            self.llm_client.clone(),
            self.clone(),
        );

        // Store the document
        {
            let mut workspace = self.workspace.write().unwrap();
            workspace
                .documents
                .insert(uri.to_string(), document.clone());
        }

        Ok(document)
    }

    /// Get an existing document by URI
    pub fn get_document(&self, uri: &str) -> Option<DocumentService> {
        let workspace = self.workspace.read().unwrap();
        workspace.documents.get(uri).cloned()
    }

    /// Open a document by URI, reusing existing if already open
    pub async fn open_document(&self, uri: &str) -> Result<DocumentService> {
        // Check if document already exists
        if let Some(document) = self.get_document(uri) {
            return Ok(document);
        }

        // Extract path from file:// URI
        let path_str = uri
            .strip_prefix("file://")
            .ok_or_else(|| anyhow::anyhow!("URI must be a file:// URI: {uri}"))?;

        let path = PathBuf::from(path_str);

        // Validate file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File does not exist: {}", path.display()));
        }

        // Read file content and delegate to open_document_with_text
        let source = tokio::fs::read_to_string(&path).await?;
        self.open_document_with_text(uri, &source).await
    }
}
