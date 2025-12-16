use anyhow::Result;
use futures::future::join_all;
use lsp_types::{
    DidChangeTextDocumentParams, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};
use parking_lot::RwLock;
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
    /// Single editor with base + overlays
    pub editor: CrdtEditor,
}

impl Document {
    /// Create a Document from provided text
    pub fn from_text(uri: String, content: &str) -> Result<Self> {
        let editor = CrdtEditor::new(content)?;

        Ok(Self { uri, editor })
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

        // Set overlay content (overlay was created in Generating status by start_generation)
        self.editor
            .set_overlay_content(&target.signature, &replacement);

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
            // Note: Stale overlay removal is done in DocumentService.remove_stale_overlays()
            // after all changes are applied, to avoid intermediate state issues
        } else {
            // Full document replacement
            self.editor = CrdtEditor::new(&change.text)?;
        }
        Ok(())
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

    /// Check if a target has overlay ready for code action
    pub fn is_generated(&self, signature: &str) -> bool {
        let document = self.document.read();
        document.editor.is_overlay_ready(signature)
    }

    /// Check if a target has already been applied (checksum exists in base)
    pub fn is_already_applied(&self, checksum: u64) -> bool {
        let document = self.document.read();
        document.editor.has_checksum_in_base(checksum)
    }

    /// Check if a generation is currently pending for a signature
    pub fn is_pending_generation(&self, signature: &str) -> bool {
        self.document.read().editor.is_pending(signature)
    }

    /// Remove stale overlays based on current targets
    /// Call this after all changes are applied to avoid intermediate state issues
    pub fn remove_stale_overlays(&self) {
        let mut document = self.document.write();
        if let Ok(targets) = document.find_targets() {
            document.editor.remove_stale_overlays(&targets);
        }
    }

    /// Convert byte position to LSP position
    pub fn byte_to_lsp_position(&self, byte_pos: usize) -> lsp_types::Position {
        self.document.read().editor.byte_to_lsp_position(byte_pos)
    }

    /// Convert byte range to LSP range
    pub fn byte_range_to_lsp_range(&self, range: &std::ops::Range<usize>) -> lsp_types::Range {
        self.document.read().editor.byte_range_to_lsp_range(range)
    }

    /// Get generated text by signature (from overlay)
    pub fn get_generated_text(&self, signature: &str) -> Result<String> {
        let doc = self.document.read();

        doc.editor
            .get_overlay(signature)
            .map(std::string::ToString::to_string)
            .ok_or_else(|| anyhow::anyhow!("Overlay not found for signature: {signature}"))
    }

    /// Call LLM to generate code for a target
    async fn generate_target_body(&self, target: &Target) -> Result<String> {
        let llm_client = self.llm_client.clone();
        let workspace = self.workspace.clone();

        spawn_generation_task(target, llm_client, &workspace).await
    }

    /// Generate code for a single target (independent, handles its own formatting)
    pub async fn generate_single(&self, target: Target) -> Result<()> {
        let signature = target.signature.clone();
        let checksum = target.checksum;

        // 1. Start generation (creates Generating overlay)
        self.document
            .write()
            .editor
            .start_generation(&signature, checksum);

        // 2. Run LLM with cancellation support
        let body = self.run_llm_with_cancellation(&target).await;

        // 3. Apply result or cancel
        let Some(body) = body else {
            self.document.write().editor.cancel_generation(&signature);
            // Even on failure, formatting may be unblocked for other overlays
            self.try_format_and_finalize().await;
            return Err(anyhow::anyhow!("Generation failed for {checksum:x}"));
        };

        // 4. Verify checksum still matches
        let should_apply = {
            let current_targets = self.find_targets().unwrap_or_default();
            current_targets
                .iter()
                .any(|t| t.signature == signature && t.checksum == checksum)
        };

        let is_cancelled = self
            .document
            .read()
            .editor
            .get_cancellation_token(&signature)
            .is_none_or(|t| t.is_cancelled());

        if !should_apply || is_cancelled {
            self.document.write().editor.cancel_generation(&signature);
            // Re-run formatting in case other overlays can now finalize
            self.try_format_and_finalize().await;
            return Err(anyhow::anyhow!("Generation skipped for {checksum:x}"));
        }

        // 5. Apply generation (sets to Formatting)
        let change = self.document.write().apply_generation(&target, &body);
        self.send_did_change(vec![change]).await.ok();

        // 6. Try to format (only succeeds if all overlays are Formatting)
        self.try_format_and_finalize().await;

        // 7. Wait until our overlay is Ready (formatting might be done by another task)
        while self.document.read().editor.is_pending(&signature) {
            // If other generations finished/cancelled, formatting may now proceed
            self.try_format_and_finalize().await;
            tokio::task::yield_now().await;
        }

        if self.is_generated(&signature) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Generation cancelled for {checksum:x}"))
        }
    }

    /// Run LLM generation with cancellation support
    async fn run_llm_with_cancellation(&self, target: &Target) -> Option<String> {
        let signature = &target.signature;

        // Skip if already generated
        if self.is_generated(signature) {
            return None;
        }

        // Get token from overlay
        let token = self
            .document
            .read()
            .editor
            .get_cancellation_token(signature)?;

        // LLM generation with cancellation
        tokio::select! {
            () = token.cancelled() => {
                tracing::info!(
                    signature = %signature,
                    checksum = format!("{:x}", target.checksum),
                    "Generation cancelled during LLM call"
                );
                None
            }
            result = self.generate_target_body(target) => {
                match result {
                    Ok(body) if !token.is_cancelled() => Some(body),
                    Ok(_) => {
                        tracing::info!(
                            signature = %signature,
                            "Generation result discarded (cancelled after LLM)"
                        );
                        None
                    }
                    Err(e) => {
                        tracing::error!("LLM generation failed for {:x}: {:?}", target.checksum, e);
                        None
                    }
                }
            }
        }
    }

    /// Try to format and finalize all Formatting overlays
    /// Only succeeds if all overlays are in Formatting status
    async fn try_format_and_finalize(&self) {
        // Atomically check should_format and set formatting_in_progress
        let should_format = self.document.write().editor.try_start_formatting();

        if !should_format {
            return;
        }

        tracing::debug!("Formatting generated code");
        self.format_document().await.ok();

        // Finalize: set all Formatting overlays to Ready
        self.document.write().editor.finalize_formatting();
    }

    /// Spawn background generation for multiple targets (independent tasks)
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
            let futures: Vec<_> = targets
                .into_iter()
                .map(|target| {
                    let c = clone.clone();
                    let checksum = target.checksum;
                    async move {
                        if c.generate_single(target).await.is_ok() {
                            Some(checksum)
                        } else {
                            None
                        }
                    }
                })
                .collect();

            let results = join_all(futures).await;
            let succeeded: Vec<u64> = results.into_iter().flatten().collect();
            let _ = tx.send(succeeded);
        });

        Some(rx)
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
            let doc = self.document.write();
            let composed = doc
                .editor
                .composed_view()
                .unwrap_or_else(|_| doc.editor.get_text());
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
                // Apply formatting edits to overlays (not base)
                // This preserves the overlay structure while formatting the generated code
                let formatted_text = {
                    let mut doc = self.document.write();
                    let composed = doc
                        .editor
                        .composed_view()
                        .unwrap_or_else(|_| doc.editor.get_text());
                    let composed_rope = crop::Rope::from(composed.as_str());
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
