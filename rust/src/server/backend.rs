use std::path::PathBuf;
use tokio::sync::RwLock as AsyncRwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability,
    CodeActionResponse, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, Position,
    Range, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    Uri, WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::config::Config;
use crate::workspace::WorkspaceService;

/// Mantra LSP backend
pub struct MantraBackend {
    client: Client,
    /// Workspace service (initialized after receiving root_uri)
    workspace: AsyncRwLock<Option<WorkspaceService>>,
}

impl MantraBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: AsyncRwLock::new(None),
        }
    }

    /// Initialize workspace service from root_uri
    async fn init_workspace(&self, root_uri: Option<String>) -> bool {
        let root_path = match root_uri {
            Some(uri) => {
                // Parse file:// URI
                if let Some(path) = uri.strip_prefix("file://") {
                    PathBuf::from(path)
                } else {
                    tracing::warn!("root_uri is not a file:// URI: {}", uri);
                    return false;
                }
            }
            None => {
                tracing::warn!("No root_uri provided");
                return false;
            }
        };

        // Try to load config
        let config = match Config::load(&root_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to load mantra.toml: {}", e);
                return false;
            }
        };

        tracing::info!("Loaded config: model={}, url={}", config.model, config.url);
        tracing::info!("API key configured: {}", config.api_key.as_ref().map(|k| if k.starts_with("${") { "NOT EXPANDED (env var not set)" } else { "yes (hidden)" }).unwrap_or("none"));

        // Initialize workspace service
        match WorkspaceService::new(root_path, config).await {
            Ok(ws) => {
                let mut workspace = self.workspace.write().await;
                *workspace = Some(ws);
                tracing::info!("Workspace service initialized");
                true
            }
            Err(e) => {
                tracing::error!("Failed to initialize workspace service: {}", e);
                false
            }
        }
    }

    /// Analyze document and publish diagnostics
    async fn analyze_and_publish_diagnostics(&self, uri: Uri, text: &str) {
        let diagnostics = self.find_mantra_targets(text, uri.as_str());

        tracing::info!(
            "Publishing {} diagnostics for {}",
            diagnostics.len(),
            uri.as_str()
        );

        for d in &diagnostics {
            tracing::debug!("  Diagnostic: {} at line {}", d.message, d.range.start.line);
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Find mantra targets in text and return diagnostics
    fn find_mantra_targets(&self, text: &str, uri: &str) -> Vec<Diagnostic> {
        use crate::parser::target::Target;

        let targets = Target::find_targets_from_text(text, uri);

        targets
            .into_iter()
            .filter(|t| t.has_panic_not_implemented)
            .map(|t| {
                // Convert byte range start to line/character
                let (line, character) = byte_to_line_char(text, t.byte_range.start);

                Diagnostic {
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position { line, character },
                    },
                    severity: Some(DiagnosticSeverity::HINT),
                    source: Some("mantra".to_string()),
                    message: format!("Generate implementation: {}", t.instruction),
                    data: Some(serde_json::json!({
                        "instruction": t.instruction,
                    })),
                    ..Default::default()
                }
            })
            .collect()
    }
}

/// Convert byte position to (line, character) in text
fn byte_to_line_char(text: &str, byte_pos: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut character = 0u32;

    for (idx, ch) in text.char_indices() {
        if idx >= byte_pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    (line, character)
}

impl LanguageServer for MantraBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("LSP initialize request received");

        // Initialize workspace in background
        let root_uri = params.root_uri.map(|u| u.to_string());
        self.init_workspace(root_uri).await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Incremental document sync for CRDT
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                // Code actions for generating implementations
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "mantra".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("LSP server initialized");

        let has_workspace = self.workspace.read().await.is_some();
        let msg = if has_workspace {
            "mantra LSP server initialized with workspace"
        } else {
            "mantra LSP server initialized (no mantra.toml found)"
        };

        self.client
            .log_message(tower_lsp_server::lsp_types::MessageType::INFO, msg)
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("LSP shutdown request received");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        tracing::debug!("Document opened: {}", uri.as_str());

        // Create DocumentService via WorkspaceService
        let workspace = self.workspace.read().await;
        if let Some(ws) = workspace.as_ref() {
            if let Err(e) = ws.open_document_with_text(uri.as_str(), &text).await {
                tracing::warn!("Failed to create DocumentService: {}", e);
            }
        }

        // Analyze and publish diagnostics
        self.analyze_and_publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let changes = params.content_changes;

        tracing::debug!(
            "Document changed: {} ({} changes)",
            uri.as_str(),
            changes.len()
        );

        // Apply incremental changes to DocumentService and get updated text
        let text = {
            let workspace = self.workspace.read().await;
            if let Some(ws) = workspace.as_ref() {
                if let Some(doc_service) = ws.get_document(uri.as_str()) {
                    if let Err(e) = doc_service.apply_changes(&changes) {
                        tracing::warn!("Failed to apply changes: {}", e);
                    }
                    doc_service.get_text().ok()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(text) = text {
            // Re-analyze and publish diagnostics
            self.analyze_and_publish_diagnostics(uri, &text).await;
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let diagnostics = params.context.diagnostics;

        tracing::info!("Code action request for: {}", uri.as_str());
        tracing::info!("Received {} diagnostics", diagnostics.len());

        // Filter for mantra diagnostics
        let mantra_diagnostics: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.source.as_deref() == Some("mantra"))
            .collect();

        tracing::info!("Found {} mantra diagnostics", mantra_diagnostics.len());

        if mantra_diagnostics.is_empty() {
            return Ok(None);
        }

        let mut actions = Vec::new();

        // Get DocumentService via WorkspaceService
        let workspace = self.workspace.read().await;
        let workspace = match workspace.as_ref() {
            Some(ws) => ws,
            None => {
                tracing::warn!("No workspace available for code action");
                return Ok(None);
            }
        };

        let doc_service = match workspace.open_document(uri.as_str()).await {
            Ok(ds) => ds,
            Err(e) => {
                tracing::error!("Failed to open document: {}", e);
                return Ok(None);
            }
        };

        // Find targets using DocumentService (to get correct CRDT snapshots)
        let targets = match doc_service.find_targets() {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to find targets: {}", e);
                return Ok(None);
            }
        };
        tracing::info!("Found {} targets in document", targets.len());

        // Get text for position calculation
        let text = match doc_service.get_text() {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to get document text: {}", e);
                return Ok(None);
            }
        };

        for diagnostic in mantra_diagnostics {
            tracing::info!("Diagnostic range: {:?}", diagnostic.range);

            // Extract instruction from diagnostic data for matching
            let instruction = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("instruction"))
                .and_then(|v| v.as_str());

            // Find the matching target by instruction (primary) or line number (fallback)
            let diagnostic_line = diagnostic.range.start.line;

            let target = if let Some(instr) = instruction {
                // Match by instruction content
                targets.iter().find(|t| t.instruction == instr)
            } else {
                // Fallback to line number matching
                targets.iter().find(|t| {
                    let (target_start_pos, _) = byte_range_to_lsp_range(&text, &t.byte_range);
                    target_start_pos.line == diagnostic_line + 1
                })
            };

            let target = match target {
                Some(t) => t.clone(),
                None => {
                    tracing::warn!(
                        "No target found for diagnostic at line {} (instruction: {:?})",
                        diagnostic_line,
                        instruction
                    );
                    continue;
                }
            };

            tracing::info!(
                "Found target: signature='{}', byte_range={:?}",
                target.signature,
                target.byte_range
            );

            // Generate code using DocumentService (with formatting)
            let edits = match doc_service.generate_single_with_edits(&target).await {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!("Generation failed: {}", e);
                    continue;
                }
            };

            if edits.is_empty() {
                tracing::warn!("No edits generated for target");
                continue;
            }

            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), edits);

            let workspace_edit = WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            };

            // Create a code action with the workspace edit
            let action = CodeAction {
                title: format!("🔮 Generate: {}", target.instruction),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                command: None,
                edit: Some(workspace_edit),
                is_preferred: Some(true),
                disabled: None,
                data: None,
            };

            tracing::info!("Created code action: title='{}'", action.title);

            actions.push(CodeActionOrCommand::CodeAction(action));
        }

        tracing::info!("Returning {} code actions", actions.len());
        Ok(Some(actions))
    }
}

/// Convert byte range to LSP position range
fn byte_range_to_lsp_range(text: &str, byte_range: &std::ops::Range<usize>) -> (Position, Position) {
    let mut line = 0u32;
    let mut character = 0u32;
    let mut start_pos = Position { line: 0, character: 0 };
    let mut end_pos = Position { line: 0, character: 0 };
    let mut found_start = false;

    for (byte_idx, ch) in text.char_indices() {
        if byte_idx == byte_range.start {
            start_pos = Position { line, character };
            found_start = true;
        }
        if byte_idx == byte_range.end {
            end_pos = Position { line, character };
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    // Handle end position at end of file
    if !found_start {
        start_pos = Position { line, character };
    }
    if byte_range.end >= text.len() {
        end_pos = Position { line, character };
    }

    (start_pos, end_pos)
}
