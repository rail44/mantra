use anyhow::Result;
use lsp_types::{
    DidChangeTextDocumentParams, Range, TextDocumentContentChangeEvent,
    VersionedTextDocumentIdentifier,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::task::JoinSet;

use crate::editor::crdt::CrdtEditor;
use crate::generation::{spawn_generation_task, ApplyGeneration, EditEvent};
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::{target::Target, target_map::TargetMap};

/// Document managing a single document's state with CRDT support
pub struct Document {
    pub uri: String,
    pub file_path: PathBuf,
    pub editor: CrdtEditor,
    /// Set of checksums for currently pending generation tasks
    pending_generations: HashSet<u64>,
}

impl Document {
    pub fn new(file_path: PathBuf, uri: String) -> Result<Self> {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path.display(), e))?;

        let editor = CrdtEditor::new(&content)?;

        Ok(Self {
            uri,
            file_path,
            editor,
            pending_generations: HashSet::new(),
        })
    }

    /// Get targets for generation
    pub fn find_targets(&self) -> Result<Vec<(u64, Target, usize, usize)>> {
        let source = self.editor.get_text();
        let tree = self
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
            .clone();
        let target_map = TargetMap::build(&tree, &source)?;

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
        // Find the function in the current tree (source used only for TargetMap)
        let source = self.editor.get_text();

        // Get body positions and apply edit
        let changes = {
            let tree = self
                .editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
                .clone();
            let target_map = TargetMap::build(&tree, &source)?;

            let Some((_target, node)) = target_map.get(msg.checksum) else {
                return Err(anyhow::anyhow!(
                    "Function with checksum {:x} not found",
                    msg.checksum
                ));
            };

            // Get the function start and end for full replacement
            let func_start_byte = node.start_byte();
            let func_end_byte = node.end_byte();

            // Extract signature (everything before the body) using rope slice
            let func_text = self.editor.get_text_range(func_start_byte, func_end_byte);
            let func_signature = if let Some(brace_pos) = func_text.find('{') {
                &func_text[..brace_pos]
            } else {
                &func_text
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
            self.editor.apply_text_edits(&[text_edit], snapshot)?
        };

        Ok(changes)
    }

    /// Apply an edit event
    pub fn apply_edit(&mut self, edit: EditEvent) -> Result<Vec<TextDocumentContentChangeEvent>> {
        // Find the function in the current tree
        let source = self.editor.get_text();

        // Get body positions and apply edit
        let tree = self
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
            .clone();
        let target_map = TargetMap::build(&tree, &source)?;

        let Some((_target, node)) = target_map.get(edit.checksum) else {
            return Err(anyhow::anyhow!(
                "Function with checksum {:x} not found",
                edit.checksum
            ));
        };

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
        let changes = self.editor.apply_text_edits(&[text_edit], snapshot)?;

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

    /// Start tracking a generation task
    pub fn start_generation(&mut self, checksum: u64) {
        self.pending_generations.insert(checksum);
    }

    /// Complete a generation task
    pub fn complete_generation(&mut self, checksum: u64) {
        self.pending_generations.remove(&checksum);
    }

    /// Check if formatting should be applied
    pub fn should_format(&self) -> bool {
        self.pending_generations.is_empty()
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
            let mut document = self
                .document
                .write()
                .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;
            let targets = document.find_targets()?;

            if targets.is_empty() {
                return Ok(document.get_text());
            }

            // Mark all generations as pending
            for (checksum, _, _, _) in &targets {
                document.start_generation(*checksum);
            }

            targets
        };

        // Spawn generation tasks
        let mut set: JoinSet<Result<()>> = JoinSet::new();
        for (checksum, target, _, _) in targets {
            let llm_client = self.llm_client.clone();

            let clone = self.clone();
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
        let checksum = msg.checksum;
        tracing::debug!("Applying generation for checksum {:x}", checksum);

        let changes = {
            let mut doc = self
                .document
                .write()
                .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;
            let version_before = doc.editor.get_version();
            let changes = doc.apply_generation(msg)?;
            // Mark this generation as complete
            doc.complete_generation(checksum);
            let version_after = doc.editor.get_version();

            tracing::debug!(
                "Generation applied for checksum {:x} (version: {} -> {})",
                checksum,
                version_before,
                version_after
            );

            changes
        };

        self.send_did_change(changes).await?;

        // Check if we should format after this generation completes
        self.format_if_needed().await?;

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
            tracing::debug!(
                "Sending full document update (version: {})",
                current_version
            );
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
            tracing::debug!(
                "Sending {} incremental changes (version: {})",
                changes.len(),
                current_version
            );
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

        Ok(())
    }

    /// Format document if needed (when all generations are complete)
    async fn format_if_needed(&self) -> Result<()> {
        let should_format = {
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
            doc.should_format()
        };

        if should_format {
            tracing::debug!("All generations complete, formatting document");
            self.format_document().await?;
        }

        Ok(())
    }

    /// Format document using LSP
    async fn format_document(&self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            tracing::trace!("Document formatting not supported");
            return Ok(());
        }

        let (uri_str, version) = {
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
            (doc.uri.clone(), doc.editor.get_version())
        };

        let uri: lsp_types::Uri = uri_str.parse()?;
        tracing::debug!(
            "Requesting formatting for {} (version: {})",
            uri_str,
            version
        );

        let formatting_options = lsp_types::FormattingOptions {
            tab_size: 4,
            insert_spaces: false,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            properties: Default::default(),
        };

        match self
            .lsp_client
            .format_document(
                lsp_types::TextDocumentIdentifier { uri },
                formatting_options,
            )
            .await?
        {
            Some(edits) if !edits.is_empty() => {
                let changes = {
                    let mut doc = self
                        .document
                        .write()
                        .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;
                    let current_version = doc.editor.get_version();
                    tracing::debug!(
                        "Applying {} formatting edits (version: {} -> {})",
                        edits.len(),
                        version,
                        current_version
                    );
                    let snapshot = doc.editor.fork();
                    doc.editor.apply_text_edits(&edits, snapshot)?
                };
                // Send incremental changes to LSP
                self.send_did_change(changes).await?;
                tracing::debug!("Formatting applied successfully");
            }
            Some(_) => {
                tracing::debug!("Formatting returned empty edits");
            }
            None => {
                tracing::debug!("Formatting returned None");
            }
        }

        Ok(())
    }
}
