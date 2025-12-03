use rustc_hash::FxHashMap;
use std::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability,
    CodeActionResponse, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, Position,
    Range, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Uri, WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::editor::crdt::CrdtEditor;

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
    documents: RwLock<FxHashMap<String, CrdtEditor>>,
}

impl MantraBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(FxHashMap::default()),
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
}

impl LanguageServer for MantraBackend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("LSP initialize request received");

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Full document sync for now
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
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
        self.client
            .log_message(
                tower_lsp_server::lsp_types::MessageType::INFO,
                "mantra LSP server initialized",
            )
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

        // Store document (for future use)
        if let Ok(editor) = CrdtEditor::new(&text) {
            if let Ok(mut docs) = self.documents.write() {
                docs.insert(uri.to_string(), editor);
            }
        }

        // Analyze and publish diagnostics
        self.analyze_and_publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // For full sync, take the last change (which contains the full document)
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;

            tracing::debug!("Document changed: {}", uri.as_str());

            // Update stored document
            if let Ok(editor) = CrdtEditor::new(&text) {
                if let Ok(mut docs) = self.documents.write() {
                    docs.insert(uri.to_string(), editor);
                }
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

        for diagnostic in mantra_diagnostics {
            tracing::info!("Diagnostic data: {:?}", diagnostic.data);

            // Extract data from diagnostic
            let data = match &diagnostic.data {
                Some(d) => d,
                None => {
                    tracing::warn!("Diagnostic has no data, skipping");
                    continue;
                }
            };

            let instruction = data
                .get("instruction")
                .and_then(|v| v.as_str())
                .unwrap_or("implement");

            let panic_line = data
                .get("panic_line")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let panic_start = data
                .get("panic_start_char")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let panic_end = data
                .get("panic_end_char")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            // Create workspace edit to replace panic with TODO comment
            let new_text = format!("// TODO: {}", instruction);

            let edit = TextEdit {
                range: Range {
                    start: Position {
                        line: panic_line,
                        character: panic_start,
                    },
                    end: Position {
                        line: panic_line,
                        character: panic_end,
                    },
                },
                new_text,
            };

            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), vec![edit]);

            let workspace_edit = WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            };

            // Create a code action with the workspace edit
            let action = CodeAction {
                title: format!("Generate: {}", instruction),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                command: None,
                edit: Some(workspace_edit),
                is_preferred: Some(true),
                disabled: None,
                data: None,
            };

            tracing::info!(
                "Created code action: title='{}', edit range={}:{}-{}:{}",
                action.title,
                panic_line,
                panic_start,
                panic_line,
                panic_end
            );

            actions.push(CodeActionOrCommand::CodeAction(action));
        }

        tracing::info!("Returning {} code actions", actions.len());
        Ok(Some(actions))
    }
}
