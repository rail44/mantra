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
use crate::parser::ast_utils::find_function_at_byte_position;
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Document managing a single document's state with CRDT support
pub struct Document {
    pub uri: String,
    /// Editor synchronized with user's editor (for position calculation)
    pub editor_sync: CrdtEditor,
    /// Editor for generation (contains generated code, used for LSP communication)
    pub generation_editor: CrdtEditor,
    /// Set of checksums for currently pending generation tasks
    pending_generations: HashSet<u64>,
    /// Set of checksums that have been generated but not yet applied to the editor
    /// When code action is executed, the checksum is removed from this set
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
        let editor_sync = CrdtEditor::new(content)?;
        // Fork generation_editor from editor_sync to establish common ancestry
        // This enables cola's coordinate transformation when propagating edits
        let generation_editor = editor_sync.fork_editor()?;

        Ok(Self {
            uri,
            editor_sync,
            generation_editor,
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

    /// Get all checksums that are generated but not applied
    pub fn get_generated_not_applied(&self) -> &HashSet<u64> {
        &self.generated_not_applied
    }

    /// Get targets for generation
    /// Uses `editor_sync` which is synchronized with user's editor
    pub fn find_targets(&self) -> Result<Vec<Target>> {
        let tree = self
            .editor_sync
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;

        let rope = self.editor_sync.rope();
        let snapshot = self.editor_sync.fork();

        let targets = Target::find_targets(tree, rope, &snapshot, &self.uri);

        Ok(targets)
    }

    pub fn apply_generation(
        &mut self,
        target: &Target,
        new_body: &str,
    ) -> Result<Vec<TextDocumentContentChangeEvent>> {
        // Create replacement with checksum comment using the signature from Target
        let replacement = format!(
            "// mantra:checksum:{:x}\n{} {{\n{}\n}}",
            target.checksum,
            target.signature.trim_end(),
            new_body.trim()
        );

        let gen_tree = self
            .generation_editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available for generation_editor"))?;

        // Try to use anchor-based position resolution first
        // The anchor from editor_sync can be resolved in generation_editor because
        // they share a common ancestor and user edits are propagated
        let byte_range =
            if let Some(start_pos) = self.generation_editor.resolve_anchor(target.start_anchor) {
                // Find the function node at the resolved position
                if let Some(func_node) =
                    find_function_at_byte_position(&gen_tree.root_node(), start_pos)
                {
                    func_node.start_byte()..func_node.end_byte()
                } else {
                    // Anchor resolved but no function found at position - fall back to checksum search
                    tracing::debug!(
                    "Anchor resolved to {} but no function found, falling back to checksum search",
                    start_pos
                );
                    self.find_target_by_checksum(target.checksum)?
                }
            } else {
                // Anchor couldn't be resolved - fall back to checksum search
                tracing::debug!(
                "Anchor couldn't be resolved for checksum {:x}, falling back to checksum search",
                target.checksum
            );
                self.find_target_by_checksum(target.checksum)?
            };

        // Apply edit using the resolved byte range
        let snapshot = self.generation_editor.fork();
        let change = self
            .generation_editor
            .apply_byte_edit(&byte_range, &replacement, snapshot)?;

        Ok(vec![change])
    }

    /// Find target `byte_range` by checksum (fallback when anchor resolution fails)
    fn find_target_by_checksum(&self, checksum: u64) -> Result<std::ops::Range<usize>> {
        let gen_tree = self
            .generation_editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available for generation_editor"))?;
        let gen_rope = self.generation_editor.rope();
        let gen_snapshot = self.generation_editor.fork();

        let gen_targets = Target::find_targets(gen_tree, gen_rope, &gen_snapshot, &self.uri);
        let gen_target = gen_targets
            .into_iter()
            .find(|t| t.checksum == checksum)
            .ok_or_else(|| {
                anyhow::anyhow!("Target with checksum {checksum:x} not found in generation_editor")
            })?;

        Ok(gen_target.byte_range)
    }

    /// Get text synchronized with editor (for diagnostics position)
    pub fn get_editor_text(&self) -> String {
        self.editor_sync.get_text()
    }

    /// Get text with generated code (for LSP communication)
    pub fn get_generation_text(&self) -> String {
        self.generation_editor.get_text()
    }

    /// Apply incremental change from LSP
    pub fn apply_incremental_change(
        &mut self,
        change: &TextDocumentContentChangeEvent,
    ) -> Result<()> {
        if let Some(range) = &change.range {
            // Use editor_sync for position calculation (matches user's editor)
            let rope = self.editor_sync.rope();
            let start_byte = lsp_position_to_byte(range.start, rope);
            let end_byte = lsp_position_to_byte(range.end, rope);

            // Apply to editor_sync and get EditOperation with Insertion/Deletion
            let ops = self
                .editor_sync
                .apply_byte_edit_with_ops(&(start_byte..end_byte), &change.text)?;

            // Integrate the EditOperation into generation_editor
            // cola's coordinate transformation handles the case where generation_editor
            // has additional content (generated code) that editor_sync doesn't have
            self.generation_editor.integrate_ops(&ops)?;
        } else {
            // Full document replacement - recreate both editors with fork relationship
            self.editor_sync = CrdtEditor::new(&change.text)?;
            self.generation_editor = self.editor_sync.fork_editor()?;
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

    /// Get the current text content (synchronized with editor)
    pub fn get_text(&self) -> String {
        let document = self.document.read();
        document.get_editor_text()
    }

    /// Find targets in the document
    pub fn find_targets(&self) -> Result<Vec<crate::parser::target::Target>> {
        let document = self.document.read();
        document.find_targets()
    }

    /// Check if a target has already been generated (checksum comment exists in `generation_editor`)
    pub fn is_generated(&self, checksum: u64) -> bool {
        let document = self.document.read();
        let text = document.get_generation_text();
        let checksum_comment = format!("// mantra:checksum:{checksum:x}");
        text.contains(&checksum_comment)
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

    /// Get all checksums that are generated but not applied
    pub fn get_generated_not_applied_checksums(&self) -> HashSet<u64> {
        self.document.read().get_generated_not_applied().clone()
    }

    /// Get generated text by checksum from `generation_editor`
    pub fn get_generated_text_by_checksum(&self, checksum: u64) -> Result<String> {
        let doc = self.document.read();

        let tree = doc
            .generation_editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;
        let rope = doc.generation_editor.rope();
        let snapshot = doc.generation_editor.fork();

        let targets = Target::find_targets(tree, rope, &snapshot, &doc.uri);

        // Find the target with matching checksum
        let target = targets
            .into_iter()
            .find(|t| t.checksum == checksum)
            .ok_or_else(|| anyhow::anyhow!("Target with checksum {checksum:x} not found"))?;

        // Get the full range including checksum comment if it exists
        let start = target
            .checksum_comment_range
            .as_ref()
            .map_or(target.byte_range.start, |r| r.start);
        let end = target.byte_range.end;

        Ok(rope.byte_slice(start..end).to_string())
    }

    /// Generate code for a single target
    /// Returns the generated body text (not yet applied to CRDT)
    async fn generate_target_body(&self, target: &Target) -> Result<String> {
        let llm_client = self.llm_client.clone();
        let workspace = self.workspace.clone();

        spawn_generation_task(target, llm_client, &workspace).await
    }

    /// Generate all targets and apply to CRDT
    /// Returns the list of checksums that were successfully generated
    async fn generate_targets_sequential(&self, targets: Vec<Target>) -> Vec<u64> {
        use futures::future::join_all;

        // Mark all targets as pending first
        {
            let mut document = self.document.write();
            for target in &targets {
                document.start_generation(target.checksum);
            }
        }

        // Generate all targets in parallel (LLM calls only, no CRDT changes)
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

        // Apply all generations sequentially to CRDT
        let mut succeeded = Vec::new();
        for result in results {
            match result {
                Ok((target, Some(new_body))) => {
                    let checksum = target.checksum;
                    if self.apply_generation(target, &new_body).await.is_ok() {
                        {
                            let mut document = self.document.write();
                            document.mark_generated(checksum);
                        }
                        succeeded.push(checksum);
                    }
                }
                Ok((target, None)) => {
                    // Already generated - mark as succeeded
                    let mut document = self.document.write();
                    document.complete_generation(target.checksum);
                    succeeded.push(target.checksum);
                }
                Err((target, e)) => {
                    tracing::error!(
                        "LLM generation failed for checksum {:x}: {:?}",
                        target.checksum,
                        e
                    );
                    // Mark as complete (no longer pending) but don't add to succeeded
                    let mut document = self.document.write();
                    document.complete_generation(target.checksum);
                }
            }
        }

        // Log final CRDT state for debugging
        {
            let document = self.document.read();
            tracing::info!("CRDT after generation:\n{}", document.get_generation_text());
        }

        succeeded
    }

    /// Spawn background generation tasks
    /// Returns a receiver that will receive succeeded checksums when all generations complete
    /// Used by LSP for pre-generation
    pub fn spawn_background_generation(
        &self,
        targets: Vec<Target>,
    ) -> Option<oneshot::Receiver<Vec<u64>>> {
        if targets.is_empty() {
            return None;
        }

        let clone = self.clone();
        let (tx, rx) = oneshot::channel();

        // Spawn a single task that generates sequentially
        tokio::spawn(async move {
            let succeeded = clone.generate_targets_sequential(targets).await;
            // Signal completion with succeeded checksums (ignore error if receiver was dropped)
            let _ = tx.send(succeeded);
        });

        Some(rx)
    }

    /// Generate code for a single target and wait for completion
    /// Used by LSP code action when code wasn't pre-generated
    /// Returns Ok if generation succeeded, Err if it failed
    pub async fn generate_single(&self, target: Target) -> Result<()> {
        let checksum = target.checksum;
        let succeeded = self.generate_targets_sequential(vec![target]).await;
        if succeeded.contains(&checksum) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Generation failed for checksum {checksum:x}"
            ))
        }
    }

    /// Generate code for all targets in the document (CLI mode)
    /// Waits for all generations to complete
    pub async fn generate(&self) -> Result<String> {
        let targets = {
            let document = self.document.read();
            let targets = document.find_targets()?;

            if targets.is_empty() {
                return Ok(document.get_generation_text());
            }

            targets
        };

        // Generate all targets - for CLI mode we just proceed regardless of individual failures
        let _succeeded = self.generate_targets_sequential(targets).await;

        Ok(self.document.read().get_generation_text())
    }

    async fn apply_generation(&self, target: Target, new_body: &str) -> Result<()> {
        let checksum = target.checksum;
        tracing::debug!("Applying generation for checksum {:x}", checksum);

        let changes = {
            let mut doc = self.document.write();
            let version_before = doc.generation_editor.get_version();
            let changes = doc.apply_generation(&target, new_body).map_err(|e| {
                tracing::error!(
                    "Failed to apply generation for checksum {:x}: {:?}",
                    checksum,
                    e
                );
                e
            })?;
            // Mark this generation as complete
            doc.complete_generation(checksum);
            let version_after = doc.generation_editor.get_version();

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
            let doc = self.document.read();
            let current_version = doc.generation_editor.get_version();
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

    /// Format document if needed (when all generations are complete)
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
    /// Uses `generation_editor` since this is called for LSP communication with other servers
    pub fn get_full_definition_at(&self, range: &lsp_types::Range) -> Result<ScopedCode> {
        use crate::parser::ast_utils::{extract_definition_content, find_node_at_byte_position};

        // Use the existing tree from generation_editor (used for LSP communication)
        let doc = self.document.read();
        let tree = doc
            .generation_editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;
        let rope = doc.generation_editor.rope();

        // Calculate byte position from LSP position
        let line = range.start.line as usize;
        let line_start_byte = rope.byte_of_line(line);
        // Approximate character position (not handling UTF-16 properly yet)
        let byte_pos = line_start_byte + range.start.character as usize;

        // Find the node at this position
        let root = tree.root_node();
        let node = find_node_at_byte_position(&root, byte_pos)
            .ok_or_else(|| anyhow::anyhow!("No node at position"))?;

        // Get document URI
        let document_uri = doc.uri.clone();

        // Extract definition content
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

    /// Get definition location for a node or symbol within the node at the given AST path
    /// Uses `generation_editor` since this is called for LSP communication with other servers
    pub async fn get_definition_at_path_with_symbol(
        &self,
        ast_path: &[crate::parser::target::PathSegment],
        symbol_name: Option<&str>,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        use crate::parser::ast_utils::{find_node_by_path, get_definition_target_node};

        // Get tree, rope, snapshot and uri from generation_editor (used for LSP communication)
        let (tree, rope, snapshot, uri) = {
            let doc = self.document.read();

            let tree = doc
                .generation_editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
                .clone();

            let rope = doc.generation_editor.rope().clone();
            let snapshot = doc.generation_editor.fork();
            let uri = doc.uri.clone();

            (tree, rope, snapshot, uri)
        };

        // Find node by path
        let node = find_node_by_path(&tree.root_node(), ast_path)
            .ok_or_else(|| anyhow::anyhow!("Node not found at path"))?;

        // Get the target node for definition lookup
        let target_node = get_definition_target_node(node, symbol_name, &rope);

        // Convert byte position to LSP position
        let position = snapshot.byte_to_lsp_position(target_node.start_byte());

        // Request definition from LSP
        let text_document = lsp_types::TextDocumentIdentifier { uri: uri.parse()? };
        self.lsp_client.definition(text_document, position).await
    }

    /// Format document using LSP
    /// Applies formatting to `generation_editor` (used for LSP communication)
    async fn format_document(&self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            tracing::trace!("Document formatting not supported");
            return Ok(());
        }

        let (uri_str, version, snapshot) = {
            let doc = self.document.read();
            let snapshot = doc.generation_editor.fork();
            (
                doc.uri.clone(),
                doc.generation_editor.get_version(),
                snapshot,
            )
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
                let changes = {
                    let mut doc = self.document.write();
                    let current_version = doc.generation_editor.get_version();
                    tracing::debug!(
                        "Applying {} formatting edits (version: {} -> {})",
                        edits.len(),
                        version,
                        current_version
                    );
                    doc.generation_editor.apply_text_edits(&edits, snapshot)?
                };
                // Send incremental changes to LSP
                self.send_did_change(changes).await?;
            }
            Some(_) | None => {
                // Formatting returned empty edits or None
            }
        }

        Ok(())
    }
}
