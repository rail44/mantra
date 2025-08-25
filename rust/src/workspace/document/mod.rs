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
use crate::generation::spawn_generation_task;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::{checksum::calculate_checksum, target::Target};

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
    pub fn find_targets(&self) -> Result<Vec<Target>> {
        let tree = self
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
            .clone();

        let rope = self.editor.rope();
        let snapshot = self.editor.fork();
        let mut targets = Vec::new();

        // Find all mantra comments and their associated functions
        let mut pending_instruction: Option<String> = None;
        let mut stack = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            match node.kind() {
                "comment" => {
                    let text = rope
                        .byte_slice(node.start_byte()..node.end_byte())
                        .to_string();
                    let text = text.trim();
                    if text.starts_with("// mantra:") {
                        let instruction = text.strip_prefix("// mantra:").unwrap().trim();
                        pending_instruction = Some(instruction.to_string());
                    }
                }

                "function_declaration" | "method_declaration" => {
                    if let Some(instruction) = pending_instruction.take() {
                        // Extract function name
                        let name = node
                            .child_by_field_name("name")
                            .map(|n| rope.byte_slice(n.start_byte()..n.end_byte()).to_string())
                            .unwrap_or_else(|| "unknown".to_string());

                        // Extract signature
                        let signature = if let Some(body_node) = node.child_by_field_name("body") {
                            let sig_start = node.start_byte();
                            let sig_end = body_node.start_byte();
                            rope.byte_slice(sig_start..sig_end)
                                .to_string()
                                .trim()
                                .to_string()
                        } else {
                            rope.byte_slice(node.start_byte()..node.end_byte())
                                .to_string()
                        };

                        // Create the base target for checksum calculation
                        let base_target = Target {
                            name: name.clone(),
                            instruction: instruction.clone(),
                            signature: signature.clone(),
                            checksum: 0, // Will be calculated next
                            snapshot: snapshot.clone(),
                            start_byte: node.start_byte(),
                            end_byte: node.end_byte(),
                        };

                        // Calculate checksum based on name, instruction, and signature
                        let checksum = calculate_checksum(&base_target);

                        targets.push(Target {
                            checksum,
                            ..base_target
                        });
                    }
                }

                _ => {}
            }

            // Add children to stack in reverse order for depth-first traversal
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        Ok(targets)
    }

    pub fn apply_generation(
        &mut self,
        target: &Target,
        new_body: String,
    ) -> Result<Vec<TextDocumentContentChangeEvent>> {
        // Create replacement with checksum comment using the signature from Target
        let replacement = format!(
            "// mantra:checksum:{:x}\n{} {{\n{}\n}}",
            target.checksum,
            target.signature.trim_end(),
            new_body.trim()
        );

        // Apply edit using byte offsets directly with a forked snapshot
        let change = self.editor.apply_byte_edit(
            target.start_byte,
            target.end_byte,
            replacement,
            target.snapshot.fork(),
        )?;

        Ok(vec![change])
    }

    // NOTE: apply_edit is deprecated - use apply_generation instead
    // It's kept for backward compatibility but should be removed in the future

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
            for target in &targets {
                document.start_generation(target.checksum);
            }

            targets
        };

        // Spawn generation tasks
        let mut set: JoinSet<Result<()>> = JoinSet::new();
        for target in targets {
            let llm_client = self.llm_client.clone();

            let clone = self.clone();
            set.spawn(Box::pin(async move {
                let new_body = spawn_generation_task(&target, llm_client).await?;
                clone.apply_generation(target, new_body).await?;
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

    async fn apply_generation(&self, target: Target, new_body: String) -> Result<()> {
        let checksum = target.checksum;
        tracing::debug!("Applying generation for checksum {:x}", checksum);

        let changes = {
            let mut doc = self
                .document
                .write()
                .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;
            let version_before = doc.editor.get_version();
            let changes = doc.apply_generation(&target, new_body)?;
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

        let (uri_str, version, snapshot) = {
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
            let snapshot = doc.editor.fork();
            (doc.uri.clone(), doc.editor.get_version(), snapshot)
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
