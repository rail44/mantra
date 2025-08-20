use anyhow::{Context as AnyhowContext, Result};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::editor::crdt::CrdtEditor;
use crate::generation::EditEvent;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::{target::Target, target_map::TargetMap, GoParser};
use crate::workspace::service::{Handler, Service};
use lsp_types::{
    DidChangeTextDocumentParams, Range, TextDocumentContentChangeEvent,
    VersionedTextDocumentIdentifier,
};
use tree_sitter::Tree;

impl Service for Document {}

pub struct GenerateAll {}

impl Handler<GenerateAll> for Document {
    type Response = Result<String>;

    async fn handle(&mut self, _message: GenerateAll) -> Self::Response {
        info!("GenerateAll: {}", self.uri);

        let source = self.editor.get_text();

        // Create channel for edit events
        let (tx, mut rx) = mpsc::channel::<EditEvent>(32);

        // Spawn generation tasks in a scope to drop target_map early
        {
            let target_map = TargetMap::build(&self.tree, &source)?;

            if target_map.is_empty() {
                return Ok(source.clone());
            }

            for (checksum, (target, node)) in target_map.iter() {
                let tx = tx.clone();
                let checksum = *checksum;
                let target = target.clone();
                let start_byte = node.start_byte();
                let end_byte = node.end_byte();
                let snapshot = self.editor.fork();
                let llm_client = self.llm_client.clone();

                tokio::spawn(async move {
                    match Self::generate_for_target_static(&llm_client, &target).await {
                        Ok(new_body) => {
                            let event = EditEvent::new(
                                checksum,
                                target.signature.clone(),
                                new_body,
                                snapshot,
                                start_byte,
                                end_byte,
                            );
                            let _ = tx.send(event).await;
                        }
                        Err(e) => {
                            error!("Failed to generate for {}: {}", target.name, e);
                        }
                    }
                });
            }
        }

        // Drop the sender to close channel after all tasks complete
        drop(tx);

        // Apply edits as they arrive
        while let Some(event) = rx.recv().await {
            if let Err(e) = self.apply_edit(event).await {
                error!("Failed to apply edit: {}", e);
            }
        }

        Ok(self.editor.get_text())
    }
}

/// Document managing a single document with CRDT support
pub struct Document {
    uri: String,
    file_path: PathBuf,
    lsp_client: LspClient,
    llm_client: LLMClient,
    parser: GoParser,
    tree: Tree,
    editor: CrdtEditor,
    /// Snapshot of the version that LSP knows about
    lsp_snapshot: CrdtEditor,
}

impl Document {
    pub async fn new(
        _config: Config,
        file_path: PathBuf,
        uri: String,
        lsp_client: LspClient,
        llm_client: LLMClient,
    ) -> Result<Self> {
        // Read the file content
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Initialize parser and parse the document
        let mut parser = GoParser::new()?;
        let tree = parser
            .parse(&content)
            .with_context(|| "Failed to parse Go source")?;

        // Initialize CRDT editor
        let editor = CrdtEditor::new(&content);
        let lsp_snapshot = editor.fork();

        Ok(Self {
            uri,
            file_path,
            lsp_client,
            llm_client,
            parser,
            tree,
            editor,
            lsp_snapshot,
        })
    }

    /// Generate code for a specific target (static version for spawned tasks)
    async fn generate_for_target_static(llm_client: &LLMClient, target: &Target) -> Result<String> {
        debug!("Generating for target: {}", target.name);

        // Build prompt
        let prompt = crate::generation::build_prompt(target);

        // Generate using LLM
        let request = crate::llm::CompletionRequest {
            model: llm_client.model().to_string(),
            provider: llm_client
                .openrouter_config()
                .map(|config| crate::llm::ProviderSpec {
                    only: Some(config.providers.clone()),
                }),
            messages: vec![crate::llm::Message::user(prompt)],
            max_tokens: Some(2000),
            temperature: 0.7,
        };

        let response = llm_client.complete(request).await?;

        if let Some(choice) = response.choices.first() {
            Ok(crate::generation::clean_generated_code(
                choice.message.content.clone(),
            ))
        } else {
            Err(anyhow::anyhow!("No response from LLM"))
        }
    }

    /// Apply an edit event
    pub async fn apply_edit(&mut self, edit: EditEvent) -> Result<()> {
        debug!("ApplyEdit: checksum={:x}", edit.checksum);

        // Find the function in the current tree
        let source = self.editor.get_text();

        // Get body positions and apply edit
        {
            let target_map = TargetMap::build(&self.tree, &source)?;

            if let Some((_target, node)) = target_map.get(edit.checksum) {
                // Get function body node
                let body_node = node.child_by_field_name("body");
                let body_start = body_node
                    .as_ref()
                    .map(|n| n.start_byte())
                    .unwrap_or(node.end_byte());
                let body_end = body_node
                    .as_ref()
                    .map(|n| n.end_byte())
                    .unwrap_or(node.end_byte());

                let body = format!("{{\n{}\n}}", edit.new_body.trim());

                // Create text edit
                let start_pos = self.editor.byte_to_lsp_position(body_start);
                let end_pos = self.editor.byte_to_lsp_position(body_end);
                let text_edit = lsp_types::TextEdit::new(Range::new(start_pos, end_pos), body);

                // Apply edit using CRDT
                self.editor.apply_text_edit(text_edit, edit.snapshot);
            } else {
                error!("Function with checksum {:x} not found", edit.checksum);
                return Ok(());
            }
        }

        // Re-parse the document after target_map is dropped
        let new_source = self.editor.get_text();
        self.tree = self.parser.parse(&new_source)?;

        // Send changes to LSP
        self.send_did_change().await?;

        // Format document
        self.format_document().await?;

        Ok(())
    }

    /// Send didChange notification to LSP
    async fn send_did_change(&mut self) -> Result<()> {
        let current_version = self.editor.get_version();
        let uri: lsp_types::Uri = self.uri.parse()?;

        // Get the current document text
        let content = self.editor.get_text();

        // Send full document update
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri,
                version: current_version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content,
            }],
        };

        self.lsp_client.did_change(params).await?;

        // Update LSP snapshot
        self.lsp_snapshot = self.editor.fork();

        // Wait for diagnostics
        if let Ok(diagnostics) = self
            .lsp_client
            .wait_for_diagnostics_timeout(&self.uri, std::time::Duration::from_secs(2))
            .await
        {
            if !diagnostics.diagnostics.is_empty() {
                info!(
                    "Diagnostics for {}: {:?}",
                    self.uri, diagnostics.diagnostics
                );
            }
        }

        Ok(())
    }

    /// Format document using LSP
    async fn format_document(&mut self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            debug!("Document formatting not supported");
            return Ok(());
        }

        let uri: lsp_types::Uri = self.uri.parse()?;
        let formatting_options = lsp_types::FormattingOptions {
            tab_size: 4,
            insert_spaces: false,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            properties: Default::default(),
        };

        if let Some(edits) = self
            .lsp_client
            .format_document(
                lsp_types::TextDocumentIdentifier { uri },
                formatting_options,
            )
            .await?
        {
            if !edits.is_empty() {
                debug!("Applying {} formatting edits", edits.len());
                let snapshot = self.editor.fork();
                self.editor.apply_text_edits(&edits, snapshot);

                // Re-parse after formatting
                let new_source = self.editor.get_text();
                self.tree = self.parser.parse(&new_source)?;

                // Send updated content to LSP
                self.send_did_change().await?;
            }
        }

        Ok(())
    }

    /// Get the file URI
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Get the file path
    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}
