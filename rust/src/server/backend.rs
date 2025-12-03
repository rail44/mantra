use std::path::PathBuf;
use std::sync::RwLock;
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
use crate::editor::crdt::CrdtEditor;
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Info about panic("not implemented") location
struct PanicInfo {
    line: u32,
    start_char: u32,
    end_char: u32,
}

/// Mantra LSP backend
pub struct MantraBackend {
    client: Client,
    /// Document contents indexed by URI
    documents: RwLock<std::collections::HashMap<String, String>>,
    /// Workspace service (initialized after receiving root_uri)
    workspace: AsyncRwLock<Option<WorkspaceService>>,
}

impl MantraBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(std::collections::HashMap::new()),
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
        let diagnostics = self.find_mantra_targets(text);

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
    fn find_mantra_targets(&self, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Simple line-by-line parsing to find // mantra: comments
        let lines: Vec<&str> = text.lines().collect();
        let mut pending_mantra_line: Option<(usize, String)> = None;

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check for mantra comment
            if let Some(instruction) = trimmed.strip_prefix("// mantra:") {
                pending_mantra_line = Some((line_num, instruction.trim().to_string()));
                continue;
            }

            // Check if this line is a function declaration following a mantra comment
            if let Some((mantra_line, instruction)) = pending_mantra_line.take() {
                if trimmed.starts_with("func ") || trimmed.contains(") ") && trimmed.contains("func") {
                    // Check if it contains panic("not implemented") and get its location
                    if let Some(panic_info) = self.find_panic_not_implemented(&lines, line_num) {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position {
                                    line: mantra_line as u32,
                                    character: 0,
                                },
                                end: Position {
                                    line: mantra_line as u32,
                                    character: lines[mantra_line].len() as u32,
                                },
                            },
                            severity: Some(DiagnosticSeverity::HINT),
                            source: Some("mantra".to_string()),
                            message: format!("Generate implementation: {}", instruction),
                            // Store panic line info for code action
                            data: Some(serde_json::json!({
                                "panic_line": panic_info.line,
                                "panic_start_char": panic_info.start_char,
                                "panic_end_char": panic_info.end_char,
                                "instruction": instruction,
                            })),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        diagnostics
    }

    /// Find panic("not implemented") in a function and return its location
    fn find_panic_not_implemented(&self, lines: &[&str], start_line: usize) -> Option<PanicInfo> {
        let mut brace_count = 0;
        let mut started = false;
        let panic_pattern = "panic(\"not implemented\")";

        for (offset, line) in lines.iter().skip(start_line).enumerate() {
            let line_num = start_line + offset;

            for ch in line.chars() {
                if ch == '{' {
                    brace_count += 1;
                    started = true;
                } else if ch == '}' {
                    brace_count -= 1;
                    if started && brace_count == 0 {
                        return None;
                    }
                }
            }

            if let Some(pos) = line.find(panic_pattern) {
                return Some(PanicInfo {
                    line: line_num as u32,
                    start_char: pos as u32,
                    end_char: (pos + panic_pattern.len()) as u32,
                });
            }

            if started && brace_count == 0 {
                break;
            }
        }

        None
    }

    /// Find targets in the document text using tree-sitter parsing
    fn find_targets_in_text(&self, uri: &str, text: &str) -> Vec<Target> {
        match CrdtEditor::new(text) {
            Ok(editor) => {
                if let Some(tree) = editor.tree() {
                    let rope = editor.rope();
                    let snapshot = editor.fork();
                    Target::find_targets(tree, rope, &snapshot, uri)
                } else {
                    tracing::warn!("No parse tree available for {}", uri);
                    Vec::new()
                }
            }
            Err(e) => {
                tracing::warn!("Failed to parse document {}: {}", uri, e);
                Vec::new()
            }
        }
    }

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

        // Store document content for diagnostics
        if let Ok(mut docs) = self.documents.write() {
            docs.insert(uri.to_string(), text.clone());
        }

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

        // Apply incremental changes to DocumentService
        let workspace = self.workspace.read().await;
        if let Some(ws) = workspace.as_ref() {
            if let Some(doc_service) = ws.get_document(uri.as_str()) {
                if let Err(e) = doc_service.apply_changes(&changes) {
                    tracing::warn!("Failed to apply changes: {}", e);
                }
            }
        }
        drop(workspace);

        // Get updated text for diagnostics
        let text = {
            let workspace = self.workspace.read().await;
            if let Some(ws) = workspace.as_ref() {
                if let Some(doc_service) = ws.get_document(uri.as_str()) {
                    doc_service.get_text().ok()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(text) = text {
            // Update stored document content for code_action
            if let Ok(mut docs) = self.documents.write() {
                docs.insert(uri.to_string(), text.clone());
            }
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

            // Find the matching target by checking if the diagnostic line is just before the function
            // The diagnostic is on the mantra comment line, and the function starts on the next line
            let diagnostic_line = diagnostic.range.start.line;

            let target = targets.iter().find(|t| {
                // Convert target's byte_range.start to line number
                let (target_start_pos, _) = byte_range_to_lsp_range(&text, &t.byte_range);
                // The function should start on the line after the mantra comment
                target_start_pos.line == diagnostic_line + 1
            });

            let target = match target {
                Some(t) => t.clone(),
                None => {
                    tracing::warn!("No target found for diagnostic at line {}", diagnostic_line);
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
