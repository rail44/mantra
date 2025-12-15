use std::path::PathBuf;
use tokio::sync::RwLock as AsyncRwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InitializeResult,
    InitializedParams, Position, Range, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri, WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::config::Config;
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Mantra LSP backend
pub struct MantraBackend {
    client: Client,
    /// Workspace service (initialized after receiving `root_uri`)
    workspace: AsyncRwLock<Option<WorkspaceService>>,
}

impl MantraBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: AsyncRwLock::new(None),
        }
    }

    /// Initialize workspace service from `root_uri`
    async fn init_workspace(&self, root_uri: Option<String>) -> bool {
        let root_path = if let Some(uri) = root_uri {
            // Parse file:// URI
            if let Some(path) = uri.strip_prefix("file://") {
                PathBuf::from(path)
            } else {
                tracing::warn!("root_uri is not a file:// URI: {}", uri);
                return false;
            }
        } else {
            tracing::warn!("No root_uri provided");
            return false;
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
        tracing::info!(
            "API key configured: {}",
            config
                .api_key
                .as_ref()
                .map_or("none", |k| if k.starts_with("${") {
                    "NOT EXPANDED (env var not set)"
                } else {
                    "yes (hidden)"
                })
        );

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

    /// Analyze document and start background generation
    /// Diagnostics are published based on `generated_not_applied` state
    async fn analyze_and_start_generation(&self, uri: Uri, text: &str) {
        let workspace = {
            let ws = self.workspace.read().await;
            if let Some(w) = ws.as_ref() {
                w.clone()
            } else {
                tracing::debug!("No workspace available for background generation");
                return;
            }
        };

        let uri_str = uri.to_string();

        // Get or create DocumentService first, then find targets from its editor
        // This ensures targets have snapshots from the document's CRDT editor
        let doc_service = match workspace.open_document_with_text(&uri_str, text).await {
            Ok(ds) => ds,
            Err(e) => {
                tracing::warn!("Failed to open document: {}", e);
                return;
            }
        };

        // Find targets from DocumentService's editor (correct snapshot)
        let targets = match doc_service.find_targets() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Failed to find targets: {}", e);
                return;
            }
        };

        // Collect diagnostics for targets that have overlays (ready for code action)
        let diagnostics_for_overlays: Vec<Diagnostic> = targets
            .iter()
            .filter(|t| doc_service.is_generated(t.checksum))
            .map(|t| {
                let (start_pos, end_pos) = byte_range_to_lsp_range(text, &t.byte_range);
                let edit_start_pos = byte_offset_to_position(text, t.edit_start_byte);
                create_diagnostic(t, start_pos, end_pos, edit_start_pos)
            })
            .collect();

        // Filter out already generated or currently generating targets
        let generation_targets: Vec<Target> = targets
            .into_iter()
            .filter(|t| {
                !t.is_generated
                    && !doc_service.is_generated(t.checksum)
                    && !doc_service.is_pending_generation(t.checksum)
            })
            .collect();

        if generation_targets.is_empty() {
            // No new targets to generate, publish diagnostics for existing overlays
            self.client
                .publish_diagnostics(uri, diagnostics_for_overlays, None)
                .await;
            return;
        }

        // Clone targets for diagnostics (need original positions)
        let targets_for_diagnostics = generation_targets.clone();
        let original_text = text.to_string();

        // Start background generation and get completion receiver
        let Some(completion_rx) = doc_service.spawn_background_generation(generation_targets)
        else {
            return;
        };

        // Spawn a task to publish diagnostics when generation completes
        let client = self.client.clone();
        tokio::spawn(async move {
            let Ok(succeeded_checksums) = completion_rx.await else {
                return;
            };

            // Publish diagnostics for newly generated targets AND existing overlays
            let mut diagnostics: Vec<Diagnostic> = targets_for_diagnostics
                .iter()
                .filter(|t| succeeded_checksums.contains(&t.checksum))
                .map(|t| {
                    let (start_pos, end_pos) =
                        byte_range_to_lsp_range(&original_text, &t.byte_range);
                    let edit_start_pos = byte_offset_to_position(&original_text, t.edit_start_byte);
                    create_diagnostic(t, start_pos, end_pos, edit_start_pos)
                })
                .collect();

            // Include diagnostics for existing overlays
            diagnostics.extend(diagnostics_for_overlays);

            let uri: Uri = match uri_str.parse() {
                Ok(u) => u,
                Err(_) => return,
            };

            client.publish_diagnostics(uri, diagnostics, None).await;
        });
    }
}

impl LanguageServer for MantraBackend {
    #[allow(deprecated)] // root_uri is deprecated but still widely used
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

        // Analyze and start background generation
        self.analyze_and_start_generation(uri, &text).await;
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
                    Some(doc_service.get_text())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(text) = text {
            // Re-analyze and start generation for any new targets
            self.analyze_and_start_generation(uri, &text).await;
        }
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::mutable_key_type
    )]
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let diagnostics = params.context.diagnostics;

        // Filter for mantra diagnostics
        let mantra_diagnostics: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.source.as_deref() == Some("mantra"))
            .collect();

        if mantra_diagnostics.is_empty() {
            return Ok(None);
        }

        let mut actions = Vec::new();

        // Get DocumentService via WorkspaceService
        let workspace = self.workspace.read().await;
        let Some(workspace) = workspace.as_ref() else {
            tracing::warn!("No workspace available for code action");
            return Ok(None);
        };

        let Some(doc_service) = workspace.get_document(uri.as_str()) else {
            tracing::warn!("Document not found: {}", uri.as_str());
            return Ok(None);
        };

        for diagnostic in mantra_diagnostics {
            // Extract checksum and instruction from diagnostic data
            let checksum_str = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("checksum"))
                .and_then(|v| v.as_str());

            let instruction = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("instruction"))
                .and_then(|v| v.as_str());

            let checksum = if let Some(s) = checksum_str {
                match u64::from_str_radix(s, 16) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to parse checksum '{}': {}", s, e);
                        continue;
                    }
                }
            } else {
                tracing::warn!("No checksum in diagnostic data");
                continue;
            };

            let instruction = if let Some(i) = instruction {
                i.to_string()
            } else {
                tracing::warn!("No instruction in diagnostic data");
                continue;
            };

            // Extract target range from diagnostic data
            let target_end_line = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("target_end_line"))
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32);

            let target_end_character = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("target_end_character"))
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32);

            // Extract edit start position (includes any preceding checksum comments)
            let edit_start_line = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("edit_start_line"))
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32);

            let edit_start_character = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("edit_start_character"))
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32);

            let (start_pos, end_pos) = if let (Some(esl), Some(esc), Some(el), Some(ec)) = (
                edit_start_line,
                edit_start_character,
                target_end_line,
                target_end_character,
            ) {
                (
                    Position {
                        line: esl,
                        character: esc,
                    },
                    Position {
                        line: el,
                        character: ec,
                    },
                )
            } else {
                tracing::warn!("No target range in diagnostic data");
                continue;
            };

            // Check if code has been generated (either via background or now)
            let is_generated = doc_service.is_generated(checksum);

            tracing::info!(
                "Code action: checksum={:x}, is_generated={}",
                checksum,
                is_generated
            );

            if !is_generated {
                // Find target from CRDT
                let target = match doc_service.find_targets() {
                    Ok(targets) => {
                        if let Some(t) = targets.into_iter().find(|t| t.checksum == checksum) {
                            t
                        } else {
                            tracing::warn!("Target with checksum {:x} not found", checksum);
                            continue;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to find targets: {}", e);
                        continue;
                    }
                };

                if let Err(e) = doc_service.generate_single(target).await {
                    tracing::error!("Generation failed: {}", e);
                    continue;
                }
            }

            // Get the generated text from CRDT using checksum
            let generated_text = match doc_service.get_generated_text_by_checksum(checksum) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("Failed to get generated text: {}", e);
                    continue;
                }
            };

            let edit_range = lsp_types::Range::new(start_pos, end_pos);

            // Note: With automerge, we don't need to filter didChange events.
            // When the editor applies the WorkspaceEdit and sends didChange,
            // the base will be updated and overlays will be automatically
            // merged correctly via automerge's merge.

            let edits = vec![lsp_types::TextEdit {
                range: edit_range,
                new_text: generated_text,
            }];

            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), edits);

            let workspace_edit = WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            };

            // Create a code action with the workspace edit
            let action = CodeAction {
                title: format!("🔮 Generate: {instruction}"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                command: None,
                edit: Some(workspace_edit),
                is_preferred: Some(true),
                disabled: None,
                data: None,
            };

            // Mark as applied (optimistically - assumes user will apply the action)
            doc_service.mark_applied(checksum);

            actions.push(CodeActionOrCommand::CodeAction(action));
        }

        Ok(Some(actions))
    }
}

/// Convert byte range to LSP position range
fn byte_range_to_lsp_range(
    text: &str,
    byte_range: &std::ops::Range<usize>,
) -> (Position, Position) {
    let mut line = 0u32;
    let mut character = 0u32;
    let mut start_pos = Position {
        line: 0,
        character: 0,
    };
    let mut end_pos = Position {
        line: 0,
        character: 0,
    };
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

/// Convert byte offset to LSP position
fn byte_offset_to_position(text: &str, byte_offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;

    for (byte_idx, ch) in text.char_indices() {
        if byte_idx == byte_offset {
            return Position { line, character };
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    // Handle offset at end of file
    Position { line, character }
}

/// Create a diagnostic for a target
fn create_diagnostic(
    target: &Target,
    start_pos: Position,
    end_pos: Position,
    edit_start_pos: Position,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: start_pos.line,
                character: 0,
            },
            end: Position {
                line: start_pos.line,
                character: start_pos.character,
            },
        },
        severity: Some(DiagnosticSeverity::HINT),
        source: Some("mantra".to_string()),
        message: format!("Generate implementation: {}", target.instruction),
        data: Some(serde_json::json!({
            "instruction": target.instruction,
            "checksum": format!("{:x}", target.checksum),
            "target_start_line": start_pos.line,
            "target_start_character": start_pos.character,
            "target_end_line": end_pos.line,
            "target_end_character": end_pos.character,
            "edit_start_line": edit_start_pos.line,
            "edit_start_character": edit_start_pos.character,
        })),
        ..Default::default()
    }
}
