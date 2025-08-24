use anyhow::{Context as AnyhowContext, Result};
use lsp_types::{
    DidChangeTextDocumentParams, Range, TextDocumentContentChangeEvent,
    VersionedTextDocumentIdentifier,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::task::JoinSet;
use tracing::error;
use tree_sitter::Tree;

use crate::editor::crdt::CrdtEditor;
use crate::generation::{spawn_generation_task, ApplyGeneration, EditEvent};
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::{target::Target, target_map::TargetMap, GoParser};

/// Document managing a single document's state with CRDT support
pub struct Document {
    pub uri: String,
    pub file_path: PathBuf,
    pub parser: GoParser,
    pub tree: Tree,
    pub editor: CrdtEditor,
}

impl Document {
    pub fn new(file_path: PathBuf, uri: String) -> Result<Self> {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path.display(), e))?;

        let mut parser = GoParser::new()?;
        let tree = parser
            .parse(&content)
            .with_context(|| "Failed to parse Go source")?;

        // Initialize CRDT editor
        let editor = CrdtEditor::new(&content);

        Ok(Self {
            uri,
            file_path,
            parser,
            tree,
            editor,
        })
    }

    /// Get targets for generation
    pub fn find_targets(&self) -> Result<Vec<(u64, Target, usize, usize)>> {
        let source = self.editor.get_text();
        let target_map = TargetMap::build(&self.tree, &source)?;

        let mut targets = Vec::new();
        for (checksum, (target, node)) in target_map.iter() {
            targets.push((
                *checksum,
                target.clone(),
                node.start_byte(),
                node.end_byte(),
            ));
        }
        Ok(targets)
    }

    pub fn apply_generation(
        &mut self,
        msg: ApplyGeneration,
    ) -> Result<Vec<TextDocumentContentChangeEvent>> {
        // Find the function in the current tree
        let source = self.editor.get_text();

        // Get body positions and apply edit
        let changes = {
            let target_map = TargetMap::build(&self.tree, &source)?;

            let Some((_target, node)) = target_map.get(msg.checksum) else {
                return Err(anyhow::anyhow!(
                    "Function with checksum {:x} not found",
                    msg.checksum
                ));
            };

            // Get the function start and end for full replacement
            let func_start_byte = node.start_byte();
            let func_end_byte = node.end_byte();

            // Extract signature (everything before the body)
            let func_text = &source[func_start_byte..func_end_byte];
            let func_signature = if let Some(brace_pos) = func_text.find('{') {
                &func_text[..brace_pos]
            } else {
                func_text
            };

            // Create replacement with checksum comment
            let replacement = format!(
                "// mantra:checksum:{:x}\n{} {{\n{}\n}}",
                msg.checksum,
                func_signature.trim_end(),
                msg.new_body.trim()
            );

            // Create text edit for full function replacement
            let start_pos = self.editor.byte_to_lsp_position(func_start_byte);
            let end_pos = self.editor.byte_to_lsp_position(func_end_byte);
            let text_edit = lsp_types::TextEdit::new(Range::new(start_pos, end_pos), replacement);

            // Apply edit using CRDT and get changes
            let snapshot = self.editor.fork();
            self.editor.apply_text_edits(&[text_edit], snapshot)
        };

        // Re-parse the document after modification
        let new_source = self.editor.get_text();
        self.tree = self.parser.parse(&new_source)?;

        Ok(changes)
    }

    /// Apply an edit event
    pub fn apply_edit(&mut self, edit: EditEvent) -> Result<Vec<TextDocumentContentChangeEvent>> {
        // Find the function in the current tree
        let source = self.editor.get_text();

        // Get body positions and apply edit
        let changes = {
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

                // Apply edit using CRDT and get changes
                let snapshot = self.editor.fork();
                self.editor.apply_text_edits(&[text_edit], snapshot)
            } else {
                error!("Function with checksum {:x} not found", edit.checksum);
                vec![]
            }
        };

        // Re-parse the document after target_map is dropped
        let new_source = self.editor.get_text();
        self.tree = self.parser.parse(&new_source)?;

        Ok(changes)
    }

    /// Get text content
    pub fn get_text(&self) -> String {
        self.editor.get_text()
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

/// Service wrapper for Document with external dependencies
#[derive(Clone)]
pub struct DocumentService {
    document: Arc<RwLock<Document>>,
    lsp_client: LspClient,
    llm_client: LLMClient,
}

impl DocumentService {
    pub fn new(document: Document, lsp_client: LspClient, llm_client: LLMClient) -> Self {
        Self {
            document: Arc::new(RwLock::new(document)),
            llm_client,
            lsp_client,
        }
    }

    pub async fn generate(&self) -> Result<String> {
        let targets = {
            let document = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
            let targets = document.find_targets()?;

            if targets.is_empty() {
                return Ok(document.get_text());
            }
            targets
        };

        // Spawn generation tasks
        let mut set: JoinSet<Result<()>> = JoinSet::new();
        for (checksum, target, _, _) in targets {
            let llm_client = self.llm_client.clone();

            let clone = self.clone().to_owned();
            set.spawn(Box::pin(async move {
                let result = spawn_generation_task(checksum, target, llm_client).await?;
                clone.apply_generation(result).await?;
                Ok(())
            }));
        }

        while let Some(res) = set.join_next().await {
            let _ = res?;
        }

        Ok(self
            .document
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?
            .get_text())
    }

    async fn apply_generation(&self, msg: ApplyGeneration) -> Result<()> {
        let changes = self
            .document
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?
            .apply_generation(msg)?;
        self.send_did_change(changes).await?;
        self.format_document().await?;
        Ok(())
    }

    async fn send_did_change(&self, changes: Vec<TextDocumentContentChangeEvent>) -> Result<()> {
        let (current_version, uri) = {
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
            let current_version = doc.editor.get_version();
            let uri: lsp_types::Uri = doc.uri.parse()?;
            (current_version, uri)
        };

        // Send incremental or full document update
        let content_changes = if changes.is_empty() {
            // Fallback to full document if no changes tracked
            let content = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?
                .get_text();
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content,
            }]
        } else {
            changes
        };

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: current_version,
            },
            content_changes,
        };

        self.lsp_client.did_change(params).await?;

        // Wait for diagnostics
        let uri_str = self
            .document
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?
            .uri
            .clone();
        self.lsp_client
            .wait_for_diagnostics_timeout(&uri_str, std::time::Duration::from_secs(2))
            .await?;

        Ok(())
    }

    /// Format document using LSP
    async fn format_document(&self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            tracing::trace!("Document formatting not supported");
            return Ok(());
        }

        let uri: lsp_types::Uri = self
            .document
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?
            .uri
            .parse()?;
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
                let changes = {
                    let mut doc = self
                        .document
                        .write()
                        .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;
                    tracing::debug!("Applying {} formatting edits", edits.len());
                    let snapshot = doc.editor.fork();
                    let changes = doc.editor.apply_text_edits(&edits, snapshot);

                    // Re-parse after formatting
                    let new_source = doc.get_text();
                    doc.tree = doc.parser.parse(&new_source)?;

                    changes
                };
                // Send incremental changes to LSP
                self.send_did_change(changes).await?;
            }
        }

        Ok(())
    }
}
