use actix::prelude::*;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::tools::inspect::{InspectTool, RegisterScope as InspectRegisterScope};

use super::document::DocumentActor;
use super::messages::*;

// ============================================================================
// Actor Definition
// ============================================================================

/// Actix-based Workspace actor (renamed from WorkspaceActor for clarity)
pub struct Workspace {
    /// Document actors by file URI
    documents: HashMap<String, Addr<DocumentActor>>,
    /// LSP client
    lsp_client: LspClient,
    /// LLM client
    llm_client: LLMClient,
    /// Workspace root directory
    root_dir: PathBuf,
    /// Configuration
    config: Config,
    /// Inspect tool actor address
    inspect_tool: Option<Addr<InspectTool>>,
}

impl Workspace {
    /// Create a new workspace actor
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
            root_dir,
            config,
            inspect_tool: None,
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

        // Start InspectTool actor
        let inspect_tool = InspectTool::new().start();
        self.inspect_tool = Some(inspect_tool);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Workspace actor stopped");
        // Clean up LSP client connection
        let lsp_client = self.lsp_client.clone();
        if let Err(e) = futures::executor::block_on(lsp_client.shutdown()) {
            error!("Failed to shutdown LSP client: {}", e);
        }
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

impl Handler<RegisterScope> for Workspace {
    type Result = ResponseFuture<String>;

    fn handle(&mut self, msg: RegisterScope, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("RegisterScope: uri={}, range={:?}", msg.uri, msg.range);

        if let Some(ref inspect_tool) = self.inspect_tool {
            let inspect_tool = inspect_tool.clone();
            Box::pin(async move {
                inspect_tool
                    .send(InspectRegisterScope {
                        uri: msg.uri,
                        range: msg.range,
                    })
                    .await
                    .unwrap_or_else(|e| {
                        error!("Failed to register scope: {}", e);
                        "error_registering_scope".to_string()
                    })
            })
        } else {
            Box::pin(async move { "no_inspect_tool".to_string() })
        }
    }
}

impl Handler<GetDocument> for Workspace {
    type Result = ResponseActFuture<Self, Result<Addr<DocumentActor>>>;

    fn handle(&mut self, msg: GetDocument, ctx: &mut Context<Self>) -> Self::Result {
        debug!("GetDocument: uri={}", msg.uri);

        // Check if document actor already exists
        if let Some(addr) = self.documents.get(&msg.uri) {
            let addr = addr.clone();
            return Box::pin(async move { Ok(addr) }.into_actor(self));
        }

        // Parse file path from URI
        let file_path = if let Some(stripped) = msg.uri.strip_prefix("file://") {
            PathBuf::from(stripped)
        } else {
            PathBuf::from(&msg.uri)
        };

        // Create new document actor
        let config = self.config.clone();
        let uri = msg.uri.clone();
        let workspace_addr = ctx.address();
        let lsp_client = self.lsp_client.clone();

        Box::pin(
            async move {
                // Read file content
                let source = tokio::fs::read_to_string(&file_path).await?;

                // Create and start document actor
                let document =
                    DocumentActor::new(config, file_path.clone(), uri.clone(), workspace_addr)
                        .await?;

                let document_addr = document.start();

                // Open document in LSP
                lsp_client
                    .did_open(crate::lsp::TextDocumentItem {
                        uri: uri.clone(),
                        language_id: "go".to_string(),
                        version: 1,
                        text: source,
                    })
                    .await?;

                Ok((uri, document_addr))
            }
            .into_actor(self)
            .map(|result, actor, _ctx| {
                match result {
                    Ok((uri, addr)) => {
                        // Store the document actor
                        actor.documents.insert(uri, addr.clone());
                        Ok(addr)
                    }
                    Err(e) => Err(e),
                }
            }),
        )
    }
}

impl Handler<InspectSymbol> for Workspace {
    type Result = ResponseFuture<Result<crate::tools::inspect::InspectResponse>>;

    fn handle(&mut self, msg: InspectSymbol, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("InspectSymbol: {:?}", msg.request);

        if let Some(ref inspect_tool) = self.inspect_tool {
            let inspect_tool = inspect_tool.clone();
            let lsp_client = self.lsp_client.clone();

            Box::pin(async move {
                inspect_tool
                    .send(crate::tools::inspect::Inspect {
                        request: msg.request,
                        lsp_client,
                    })
                    .await?
            })
        } else {
            Box::pin(async move { Err(anyhow::anyhow!("InspectTool not initialized")) })
        }
    }
}

impl Handler<GenerateFile> for Workspace {
    type Result = ResponseFuture<Result<String>>;

    fn handle(&mut self, msg: GenerateFile, ctx: &mut Context<Self>) -> Self::Result {
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
                    return Box::pin(async move { Err(anyhow::anyhow!(err_msg)) });
                }
            }
        };

        // Validate file exists
        if !absolute_path.exists() {
            let err_msg = format!("File does not exist: {}", absolute_path.display());
            error!("{}", err_msg);
            return Box::pin(async move { Err(anyhow::anyhow!(err_msg)) });
        }

        let file_uri = format!("file://{}", absolute_path.display());

        // Get self address for GetDocument
        let addr = ctx.address();

        Box::pin(async move {
            // Get document actor
            let document_addr = match addr
                .send(GetDocument {
                    uri: file_uri.clone(),
                })
                .await
            {
                Ok(Ok(addr)) => addr,
                Ok(Err(e)) => {
                    error!("Failed to get document actor for {}: {}", file_uri, e);
                    return Err(e);
                }
                Err(e) => {
                    error!("Failed to send GetDocument message: {}", e);
                    return Err(anyhow::anyhow!("Actor communication error: {}", e));
                }
            };

            // Generate code
            match document_addr.send(GenerateAll).await {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(e)) => {
                    error!("Code generation failed: {}", e);
                    Err(e)
                }
                Err(e) => {
                    error!("Failed to send GenerateAll message: {}", e);
                    Err(anyhow::anyhow!("Actor communication error: {}", e))
                }
            }
        })
    }
}

impl Handler<Shutdown> for Workspace {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Context<Self>) -> Self::Result {
        info!("Shutting down Workspace actor");

        // Send shutdown to all document actors
        for (uri, addr) in self.documents.drain() {
            debug!("Shutting down document: {}", uri);
            addr.do_send(DocumentShutdown);
        }

        // Stop self
        ctx.stop();
    }
}
