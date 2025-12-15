use anyhow::Result;
use lsp_types::{
    DidChangeTextDocumentParams, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::editor::crdt::{lsp_position_to_byte, CrdtEditor};
use crate::generation::spawn_generation_task;
use crate::inspector::ScopedCode;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Document managing a single document's state with automerge overlay support
pub struct Document {
    pub uri: String,
    /// Single editor with base + overlays (replaces `editor_sync` + `generation_editor`)
    pub editor: CrdtEditor,
    /// Set of checksums for currently pending generation tasks
    pending_generations: HashSet<u64>,
    /// Set of checksums that have been generated but not yet applied to the editor
    generated_not_applied: HashSet<u64>,
}

impl Document {
    pub fn new(file_path: &PathBuf, uri: String) -> Result<Self> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path.display(), e))?;

        Self::from_text(uri, &content)
    }

    /// Create a Document from provided text (without reading from disk)
    pub fn from_text(uri: String, content: &str) -> Result<Self> {
        let editor = CrdtEditor::new(content)?;

        Ok(Self {
            uri,
            editor,
            pending_generations: HashSet::new(),
            generated_not_applied: HashSet::new(),
        })
    }

    /// Mark a checksum as generated but not yet applied to editor
    pub fn mark_generated(&mut self, checksum: u64) {
        self.generated_not_applied.insert(checksum);
    }

    /// Check if a checksum is generated but not yet applied
    #[allow(dead_code)]
    pub fn is_generated_not_applied(&self, checksum: u64) -> bool {
        self.generated_not_applied.contains(&checksum)
    }

    /// Remove checksum from `generated_not_applied` (called after code action applied)
    pub fn mark_applied(&mut self, checksum: u64) {
        self.generated_not_applied.remove(&checksum);
    }

    /// Get targets for generation (uses base text)
    pub fn find_targets(&self) -> Result<Vec<Target>> {
        let tree = self
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;

        let rope = self.editor.rope();
        let targets = Target::find_targets(tree, rope, &self.uri);

        Ok(targets)
    }

    /// Apply generation result as an overlay
    pub fn apply_generation(
        &mut self,
        target: &Target,
        new_body: &str,
    ) -> TextDocumentContentChangeEvent {
        // Create replacement with checksum comment
        let replacement = format!(
            "// mantra:checksum:{:x}\n{} {{\n{}\n}}",
            target.checksum,
            target.signature.trim_end(),
            new_body.trim()
        );

        // Add as overlay keyed by signature (doesn't modify base)
        self.editor
            .add_overlay(&target.signature, target.checksum, &replacement);

        // Get LSP range for the change notification (in composed view coordinates)
        let start_pos = self.editor.byte_to_lsp_position(target.byte_range.start);
        let end_pos = self.editor.byte_to_lsp_position(target.byte_range.end);

        TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range::new(start_pos, end_pos)),
            range_length: None,
            text: replacement,
        }
    }

    /// Get base text (user's editor view)
    pub fn get_editor_text(&self) -> String {
        self.editor.get_text()
    }

    /// Get text with generated code (composed view = base + overlays)
    pub fn get_generation_text(&mut self) -> String {
        self.editor
            .composed_view()
            .unwrap_or_else(|_| self.editor.get_text())
    }

    /// Apply incremental change from LSP (user edit or code action)
    pub fn apply_incremental_change(
        &mut self,
        change: &TextDocumentContentChangeEvent,
    ) -> Result<()> {
        if let Some(range) = &change.range {
            let rope = self.editor.rope();
            let start_byte = lsp_position_to_byte(range.start, rope);
            let end_byte = lsp_position_to_byte(range.end, rope);

            // Apply to base first
            self.editor
                .apply_byte_edit_with_ops(&(start_byte..end_byte), &change.text)?;

            // After applying the change, check if any overlay's checksum now exists in base
            // This handles code action application where the overlay content is now in base
            self.editor.remove_overlays_matching_base();

            // Clean up generated_not_applied to keep in sync
            let current_targets = self.find_targets()?;
            let current_checksums: HashSet<u64> =
                current_targets.iter().map(|t| t.checksum).collect();
            self.generated_not_applied
                .retain(|checksum| current_checksums.contains(checksum));
        } else {
            // Full document replacement
            self.editor = CrdtEditor::new(&change.text)?;
            self.generated_not_applied.clear();
        }
        Ok(())
    }

    /// Start tracking a generation task
    pub fn start_generation(&mut self, checksum: u64) {
        self.pending_generations.insert(checksum);
    }

    /// Complete a generation task
    pub fn complete_generation(&mut self, checksum: u64) {
        self.pending_generations.remove(&checksum);
    }

    /// Check if a generation is currently pending for a checksum
    pub fn is_pending_generation(&self, checksum: u64) -> bool {
        self.pending_generations.contains(&checksum)
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
    workspace: WorkspaceService,
}

impl DocumentService {
    pub fn new(
        document: Document,
        lsp_client: LspClient,
        llm_client: LLMClient,
        workspace: WorkspaceService,
    ) -> Self {
        Self {
            document: Arc::new(RwLock::new(document)),
            llm_client,
            lsp_client,
            workspace,
        }
    }

    /// Apply incremental changes from LSP `did_change`
    pub fn apply_changes(&self, changes: &[TextDocumentContentChangeEvent]) -> Result<()> {
        let mut document = self.document.write();
        for change in changes {
            document.apply_incremental_change(change)?;
        }
        Ok(())
    }

    /// Get the current text content (base, synchronized with editor)
    pub fn get_text(&self) -> String {
        let document = self.document.read();
        document.get_editor_text()
    }

    /// Find targets in the document
    pub fn find_targets(&self) -> Result<Vec<Target>> {
        let document = self.document.read();
        document.find_targets()
    }

    /// Check if a target has already been generated (has overlay)
    pub fn is_generated(&self, checksum: u64) -> bool {
        let document = self.document.read();
        document.editor.has_overlay(checksum)
    }

    /// Check if a generation is currently pending for a checksum
    pub fn is_pending_generation(&self, checksum: u64) -> bool {
        self.document.read().is_pending_generation(checksum)
    }

    /// Check if a target is generated but not yet applied to editor
    #[allow(dead_code)]
    pub fn is_generated_not_applied(&self, checksum: u64) -> bool {
        self.document.read().is_generated_not_applied(checksum)
    }

    /// Mark a checksum as applied (remove from `generated_not_applied`)
    pub fn mark_applied(&self, checksum: u64) {
        self.document.write().mark_applied(checksum);
    }

    /// Get generated text by checksum (from composed view)
    pub fn get_generated_text_by_checksum(&self, checksum: u64) -> Result<String> {
        let mut doc = self.document.write();

        // Get composed view and parse it to find the target
        let composed = doc.get_generation_text();
        let checksum_comment = format!("// mantra:checksum:{checksum:x}");

        // Find the checksum comment and extract the function
        if let Some(start) = composed.find(&checksum_comment) {
            // Find the end of the function (next function or EOF)
            let rest = &composed[start..];
            // Simple heuristic: find next "// mantra:" or end
            let end = rest[checksum_comment.len()..]
                .find("// mantra:")
                .map_or(composed.len(), |i| start + checksum_comment.len() + i);

            Ok(composed[start..end].trim().to_string())
        } else {
            Err(anyhow::anyhow!(
                "Target with checksum {checksum:x} not found"
            ))
        }
    }

    /// Generate code for a single target
    async fn generate_target_body(&self, target: &Target) -> Result<String> {
        let llm_client = self.llm_client.clone();
        let workspace = self.workspace.clone();

        spawn_generation_task(target, llm_client, &workspace).await
    }

    /// Generate all targets and apply to overlays
    async fn generate_targets_sequential(&self, targets: Vec<Target>) -> Vec<u64> {
        use futures::future::join_all;

        // Mark all targets as pending
        {
            let mut document = self.document.write();
            for target in &targets {
                document.start_generation(target.checksum);
            }
        }

        // Generate all targets in parallel (LLM calls only)
        let generation_futures: Vec<_> = targets
            .into_iter()
            .map(|target| {
                let clone = self.clone();
                async move {
                    if clone.is_generated(target.checksum) {
                        return Ok((target, None));
                    }
                    match clone.generate_target_body(&target).await {
                        Ok(body) => Ok((target, Some(body))),
                        Err(e) => Err((target, e)),
                    }
                }
            })
            .collect();

        let results = join_all(generation_futures).await;

        // Apply all generations
        let mut succeeded = Vec::new();
        for result in results {
            match result {
                Ok((target, Some(new_body))) => {
                    let checksum = target.checksum;
                    if self.is_generated(checksum) {
                        tracing::debug!("Skipping {:x} - already generated", checksum);
                        let mut document = self.document.write();
                        document.complete_generation(checksum);
                        succeeded.push(checksum);
                        continue;
                    }
                    if self.apply_generation(target, &new_body).await.is_ok() {
                        {
                            let mut document = self.document.write();
                            document.mark_generated(checksum);
                        }
                        succeeded.push(checksum);
                    }
                }
                Ok((target, None)) => {
                    let mut document = self.document.write();
                    document.complete_generation(target.checksum);
                    succeeded.push(target.checksum);
                }
                Err((target, e)) => {
                    tracing::error!("Generation failed for {:x}: {:?}", target.checksum, e);
                    let mut document = self.document.write();
                    document.complete_generation(target.checksum);
                }
            }
        }

        // Log final state
        {
            let mut document = self.document.write();
            tracing::info!("After generation:\n{}", document.get_generation_text());
        }

        succeeded
    }

    /// Spawn background generation tasks
    pub fn spawn_background_generation(
        &self,
        targets: Vec<Target>,
    ) -> Option<oneshot::Receiver<Vec<u64>>> {
        if targets.is_empty() {
            return None;
        }

        let clone = self.clone();
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let succeeded = clone.generate_targets_sequential(targets).await;
            let _ = tx.send(succeeded);
        });

        Some(rx)
    }

    /// Generate code for a single target and wait for completion
    pub async fn generate_single(&self, target: Target) -> Result<()> {
        let checksum = target.checksum;
        let succeeded = self.generate_targets_sequential(vec![target]).await;
        if succeeded.contains(&checksum) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Generation failed for {checksum:x}"))
        }
    }

    /// Generate all targets in the document (CLI mode)
    pub async fn generate(&self) -> Result<String> {
        let targets = {
            let document = self.document.read();
            let targets = document.find_targets()?;

            if targets.is_empty() {
                return Ok(document.get_editor_text());
            }

            targets
        };

        let _succeeded = self.generate_targets_sequential(targets).await;

        Ok(self.document.write().get_generation_text())
    }

    async fn apply_generation(&self, target: Target, new_body: &str) -> Result<()> {
        let checksum = target.checksum;
        tracing::debug!("Applying generation for {:x}", checksum);

        let change = {
            let mut doc = self.document.write();
            let version_before = doc.editor.get_version();
            let change = doc.apply_generation(&target, new_body);
            doc.complete_generation(checksum);
            let version_after = doc.editor.get_version();

            tracing::debug!(
                "Generation applied for {:x} (version: {} -> {})",
                checksum,
                version_before,
                version_after
            );

            change
        };

        self.send_did_change(vec![change]).await?;
        self.format_if_needed().await?;

        Ok(())
    }

    async fn send_did_change(&self, changes: Vec<TextDocumentContentChangeEvent>) -> Result<()> {
        let (current_version, uri) = {
            let doc = self.document.read();
            let current_version = doc.editor.get_version();
            let uri: lsp_types::Uri = doc.uri.parse()?;
            (current_version, uri)
        };

        if !changes.is_empty() {
            let params = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: current_version,
                },
                content_changes: changes,
            };

            self.lsp_client.did_change(params).await?;
        }

        Ok(())
    }

    /// Format document if needed
    async fn format_if_needed(&self) -> Result<()> {
        let should_format = {
            let doc = self.document.read();
            doc.should_format()
        };

        if should_format {
            tracing::debug!("All generations complete, formatting document");
            self.format_document().await?;
        }

        Ok(())
    }

    /// Get the full definition at a range using tree-sitter
    pub fn get_full_definition_at(&self, range: &lsp_types::Range) -> Result<ScopedCode> {
        use crate::parser::ast_utils::{extract_definition_content, find_node_at_byte_position};

        let doc = self.document.read();
        let tree = doc
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;
        let rope = doc.editor.rope();

        let line = range.start.line as usize;
        let line_start_byte = rope.byte_of_line(line);
        let byte_pos = line_start_byte + range.start.character as usize;

        let root = tree.root_node();
        let node = find_node_at_byte_position(&root, byte_pos)
            .ok_or_else(|| anyhow::anyhow!("No node at position"))?;

        let document_uri = doc.uri.clone();

        if let Some((content, path_segments)) = extract_definition_content(node, rope, &root) {
            Ok(ScopedCode {
                content,
                document_uri,
                path_segments,
            })
        } else {
            Err(anyhow::anyhow!(
                "Could not find definition node at position. Found '{}' instead",
                node.kind()
            ))
        }
    }

    /// Get definition location for a node at the given AST path
    pub async fn get_definition_at_path(
        &self,
        ast_path: &[crate::parser::target::PathSegment],
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        self.get_definition_at_path_with_symbol(ast_path, None)
            .await
    }

    /// Get definition location for a node or symbol within the node
    pub async fn get_definition_at_path_with_symbol(
        &self,
        ast_path: &[crate::parser::target::PathSegment],
        symbol_name: Option<&str>,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        use crate::parser::ast_utils::{find_node_by_path, get_definition_target_node};

        let (tree, rope, uri) = {
            let doc = self.document.read();

            let tree = doc
                .editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
                .clone();

            let rope = doc.editor.rope().clone();
            let uri = doc.uri.clone();

            (tree, rope, uri)
        };

        let node = find_node_by_path(&tree.root_node(), ast_path)
            .ok_or_else(|| anyhow::anyhow!("Node not found at path"))?;

        let target_node = get_definition_target_node(node, symbol_name, &rope);

        let position = {
            let doc = self.document.read();
            doc.editor.byte_to_lsp_position(target_node.start_byte())
        };

        let text_document = lsp_types::TextDocumentIdentifier { uri: uri.parse()? };
        self.lsp_client.definition(text_document, position).await
    }

    /// Format document using LSP
    async fn format_document(&self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            tracing::trace!("Document formatting not supported");
            return Ok(());
        }

        // First, sync the composed view to gopls (full document replacement)
        // This is needed because overlays modify the document but gopls doesn't know about them
        let (uri_str, version, composed_text) = {
            let mut doc = self.document.write();
            let composed = doc.get_generation_text();
            (doc.uri.clone(), doc.editor.get_version(), composed)
        };

        let uri: lsp_types::Uri = uri_str.parse()?;

        // Send full document sync to gopls before formatting
        let full_sync_params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: version + 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None, // None means full document replacement
                range_length: None,
                text: composed_text,
            }],
        };
        self.lsp_client.did_change(full_sync_params).await?;

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
            properties: std::collections::HashMap::new(),
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
                let current_version = {
                    let doc = self.document.read();
                    doc.editor.get_version()
                };
                tracing::debug!(
                    "Applying {} formatting edits (version: {} -> {})",
                    edits.len(),
                    version,
                    current_version
                );

                // Apply formatting edits to overlays (not base)
                // This preserves the overlay structure while formatting the generated code
                let formatted_text = {
                    let mut doc = self.document.write();
                    let composed_rope = crop::Rope::from(doc.get_generation_text().as_str());
                    doc.editor
                        .apply_format_edits_to_overlays(&edits, &composed_rope)?
                };

                // Send the formatted text to gopls
                let full_sync = TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: formatted_text,
                };
                self.send_did_change(vec![full_sync]).await?;
            }
            Some(_) | None => {}
        }

        Ok(())
    }
}
