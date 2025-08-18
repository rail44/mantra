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
use lsp_types::{FormattingOptions, TextDocumentIdentifier, TextEdit as LspTextEdit};

// ============================================================================
// Helper Functions
// ============================================================================

/// Apply LSP text edits to a string
fn apply_text_edits(text: &str, edits: &[LspTextEdit]) -> String {
    // Sort edits in reverse order to apply from end to start
    let mut sorted_edits = edits.to_vec();
    sorted_edits.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then(b.range.start.character.cmp(&a.range.start.character))
    });

    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();

    for edit in sorted_edits {
        let start_line = edit.range.start.line as usize;
        let start_char = edit.range.start.character as usize;
        let end_line = edit.range.end.line as usize;
        let end_char = edit.range.end.character as usize;

        // Handle single-line edit
        if start_line == end_line {
            if let Some(line) = lines.get_mut(start_line) {
                let mut chars: Vec<char> = line.chars().collect();
                chars.splice(start_char..end_char.min(chars.len()), edit.new_text.chars());
                *line = chars.into_iter().collect();
            }
        } else {
            // Multi-line edit
            let new_lines: Vec<String> = edit.new_text.lines().map(|s| s.to_string()).collect();

            // Get the parts to keep from start and end lines
            let start_prefix = lines
                .get(start_line)
                .map(|l| l.chars().take(start_char).collect::<String>())
                .unwrap_or_default();
            let end_suffix = lines
                .get(end_line)
                .map(|l| l.chars().skip(end_char).collect::<String>())
                .unwrap_or_default();

            // Build replacement lines
            let mut replacement = Vec::new();
            if let Some(first) = new_lines.first() {
                replacement.push(format!("{}{}", start_prefix, first));
                for line in new_lines.iter().skip(1) {
                    replacement.push(line.clone());
                }
                if let Some(last) = replacement.last_mut() {
                    last.push_str(&end_suffix);
                }
            } else {
                replacement.push(format!("{}{}", start_prefix, end_suffix));
            }

            // Replace the lines
            lines.splice(start_line..=end_line, replacement);
        }
    }

    lines.join("\n")
}

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
                let doc_uri: lsp_types::Uri = uri.parse()?;
                lsp_client
                    .did_open(lsp_types::TextDocumentItem {
                        uri: doc_uri,
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

impl Handler<FormatGeneratedCode> for Workspace {
    type Result = ResponseFuture<Result<String>>;

    fn handle(&mut self, msg: FormatGeneratedCode, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("FormatGeneratedCode request");

        let lsp_client = self.lsp_client.clone();
        let root_dir = self.root_dir.clone();

        Box::pin(async move {
            // Create a temporary file with the code to format
            let temp_file = tempfile::NamedTempFile::new_in(&root_dir)?;
            let temp_path = temp_file.path().to_path_buf();
            let temp_uri = format!("file://{}", temp_path.display());

            // Write the code to the temp file
            tokio::fs::write(&temp_path, &msg.code).await?;

            // Open the document in LSP
            let text_document_item = lsp_types::TextDocumentItem {
                uri: temp_uri.parse()?,
                language_id: "go".to_string(),
                version: 1,
                text: msg.code.clone(),
            };

            lsp_client.did_open(text_document_item).await?;

            // Request formatting
            let text_document = TextDocumentIdentifier {
                uri: temp_uri.parse()?,
            };

            let formatting_options = FormattingOptions {
                tab_size: 4,
                insert_spaces: false, // Go uses tabs
                trim_trailing_whitespace: Some(true),
                insert_final_newline: Some(true),
                trim_final_newlines: Some(true),
                ..Default::default()
            };

            let formatted = match lsp_client
                .format_document(text_document, formatting_options)
                .await?
            {
                Some(edits) if !edits.is_empty() => {
                    // Apply the edits to get the formatted code
                    apply_text_edits(&msg.code, &edits)
                }
                _ => {
                    debug!("No formatting changes from LSP");
                    msg.code
                }
            };

            // Clean up temp file
            drop(temp_file);

            Ok(formatted)
        })
    }
}

impl Handler<Shutdown> for Workspace {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _msg: Shutdown, _ctx: &mut Context<Self>) -> Self::Result {
        info!("Shutting down Workspace actor");

        // Send shutdown to all document actors
        for (uri, addr) in self.documents.drain() {
            debug!("Shutting down document: {}", uri);
            addr.do_send(DocumentShutdown);
        }

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
