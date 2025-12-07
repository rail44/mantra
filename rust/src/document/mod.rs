use anyhow::Result;
use crop::Rope;
use lsp_types::{
    DidChangeTextDocumentParams, Position, TextDocumentContentChangeEvent,
    VersionedTextDocumentIdentifier,
};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::editor::crdt::CrdtEditor;

/// Find the end of a function in Go code (simple brace matching)
fn find_function_end(text: &str) -> Option<usize> {
    let mut depth = 0;
    let mut found_first_brace = false;

    for (i, ch) in text.char_indices() {
        match ch {
            '{' => {
                depth += 1;
                found_first_brace = true;
            }
            '}' => {
                depth -= 1;
                if found_first_brace && depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Convert LSP position to byte position in rope
fn lsp_position_to_byte(position: Position, rope: &Rope) -> usize {
    let line_start_byte = rope.byte_of_line(position.line as usize);
    let line_start_utf16 = rope.utf16_code_unit_of_byte(line_start_byte);
    let target_utf16 = line_start_utf16 + position.character as usize;
    rope.byte_of_utf16_code_unit(target_utf16)
}

use crate::generation::spawn_generation_task;
use crate::inspector::ScopedCode;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::target::Target;
use crate::workspace::WorkspaceService;

/// Document managing a single document's state with CRDT support
pub struct Document {
    pub uri: String,
    pub editor: CrdtEditor,
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

    /// Check if a checksum is generated but not yet applied to editor
    pub fn is_generated_not_applied(&self, checksum: u64) -> bool {
        self.generated_not_applied.contains(&checksum)
    }

    /// Get targets for generation
    pub fn find_targets(&self) -> Result<Vec<Target>> {
        let tree = self
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;

        let rope = self.editor.rope();
        let snapshot = self.editor.fork();

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

        // Apply edit using byte offsets directly with a forked snapshot
        // The target.snapshot is from when the target was found (either initially or re-found)
        // cola CRDT handles coordinate transformation if document has changed
        let change = self.editor.apply_byte_edit(
            &target.byte_range,
            &replacement,
            target.snapshot.fork(),
        )?;

        Ok(vec![change])
    }

    /// Get text content
    pub fn get_text(&self) -> String {
        self.editor.get_text()
    }

    /// Apply incremental change from LSP
    pub fn apply_incremental_change(
        &mut self,
        change: &TextDocumentContentChangeEvent,
    ) -> Result<()> {
        match &change.range {
            Some(range) => {
                // Convert LSP positions to byte positions
                let rope = self.editor.rope();
                let start_byte = lsp_position_to_byte(range.start, rope);
                let end_byte = lsp_position_to_byte(range.end, rope);

                // Apply edit
                let snapshot = self.editor.fork();
                self.editor
                    .apply_byte_edit(&(start_byte..end_byte), &change.text, snapshot)?;
            }
            None => {
                // Full document replacement - recreate editor
                self.editor = CrdtEditor::new(&change.text)?;
            }
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

    /// Apply incremental changes from LSP did_change
    pub fn apply_changes(&self, changes: &[TextDocumentContentChangeEvent]) -> Result<()> {
        let mut document = self.document.write();

        for change in changes {
            document.apply_incremental_change(change)?;
        }
        Ok(())
    }

    /// Get the current text content
    pub fn get_text(&self) -> Result<String> {
        let document = self.document.read();
        Ok(document.get_text())
    }

    /// Find targets in the document
    pub fn find_targets(&self) -> Result<Vec<crate::parser::target::Target>> {
        let document = self.document.read();
        document.find_targets()
    }

    /// Check if a target has already been generated (checksum comment exists in CRDT)
    pub fn is_generated(&self, checksum: u64) -> bool {
        let document = self.document.read();
        let text = document.get_text();
        let checksum_comment = format!("// mantra:checksum:{:x}", checksum);
        text.contains(&checksum_comment)
    }

    /// Check if a target is generated but not yet applied to editor
    pub fn is_generated_not_applied(&self, checksum: u64) -> bool {
        let document = self.document.read();
        document.is_generated_not_applied(checksum)
    }

    /// Get generated text by checksum from CRDT
    pub fn get_generated_text_by_checksum(&self, checksum: u64) -> Result<String> {
        let doc = self.document.read();

        let text = doc.get_text();
        let checksum_comment = format!("// mantra:checksum:{:x}", checksum);

        if let Some(checksum_pos) = text.find(&checksum_comment) {
            // Find the end of the function by parsing the tree
            let tree = doc
                .editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;
            let rope = doc.editor.rope();
            let snapshot = doc.editor.fork();

            let targets = Target::find_targets(tree, rope, &snapshot, &doc.uri);

            // Find the target that starts right after the checksum comment
            // The checksum comment is on the line before the function
            for target in targets {
                let target_start = target.byte_range.start;
                // Check if checksum comment is just before this target
                if target_start > checksum_pos
                    && target_start < checksum_pos + checksum_comment.len() + 50
                {
                    // Return from checksum comment to end of function
                    let end = target.byte_range.end;
                    return Ok(text[checksum_pos..end].to_string());
                }
            }

            // Fallback: find the closing brace after checksum comment
            // This is a simple heuristic for Go code
            if let Some(func_end) = find_function_end(&text[checksum_pos..]) {
                return Ok(text[checksum_pos..checksum_pos + func_end].to_string());
            }
        }

        Err(anyhow::anyhow!(
            "Generated text not found for checksum {:x}",
            checksum
        ))
    }

    /// Generate code for a single target
    /// Returns the generated body text (not yet applied to CRDT)
    async fn generate_target_body(&self, target: &Target) -> Result<String> {
        let llm_client = self.llm_client.clone();
        let workspace = self.workspace.clone();

        spawn_generation_task(target, llm_client, &workspace).await
    }

    /// Generate all targets and apply to CRDT
    async fn generate_targets_sequential(&self, targets: Vec<Target>) -> Result<()> {
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
                        Err(e) => Err(e),
                    }
                }
            })
            .collect();

        let results = join_all(generation_futures).await;

        // Apply all generations sequentially to CRDT
        for result in results {
            match result {
                Ok((target, Some(new_body))) => {
                    self.apply_generation(target.clone(), &new_body).await?;
                    {
                        let mut document = self.document.write();
                        document.mark_generated(target.checksum);
                    }
                }
                Ok((target, None)) => {
                    let mut document = self.document.write();
                    document.complete_generation(target.checksum);
                }
                Err(e) => {
                    tracing::error!("LLM generation failed: {:?}", e);
                }
            }
        }

        // Log final CRDT state for debugging
        {
            let document = self.document.read();
            tracing::info!("CRDT after generation:\n{}", document.get_text());
        }

        Ok(())
    }

    /// Spawn background generation tasks
    /// Returns a receiver that will receive () when all generations complete
    /// Used by LSP for pre-generation
    pub fn spawn_background_generation(
        &self,
        targets: Vec<Target>,
    ) -> Option<oneshot::Receiver<()>> {
        if targets.is_empty() {
            return None;
        }

        let clone = self.clone();
        let (tx, rx) = oneshot::channel();

        // Spawn a single task that generates sequentially
        tokio::spawn(async move {
            if let Err(e) = clone.generate_targets_sequential(targets).await {
                tracing::error!("Background generation failed: {:?}", e);
            }
            // Signal completion (ignore error if receiver was dropped)
            let _ = tx.send(());
        });

        Some(rx)
    }

    /// Generate code for a single target and wait for completion
    /// Used by LSP code action when code wasn't pre-generated
    pub async fn generate_single(&self, target: Target) -> Result<()> {
        self.generate_targets_sequential(vec![target]).await
    }

    /// Generate code for all targets in the document (CLI mode)
    /// Waits for all generations to complete
    pub async fn generate(&self) -> Result<String> {
        let targets = {
            let document = self.document.read();
            let targets = document.find_targets()?;

            if targets.is_empty() {
                return Ok(document.get_text());
            }

            targets
        };

        self.generate_targets_sequential(targets).await?;

        Ok(self.document.read().get_text())
    }

    async fn apply_generation(&self, target: Target, new_body: &str) -> Result<()> {
        let checksum = target.checksum;
        tracing::debug!("Applying generation for checksum {:x}", checksum);

        let changes = {
            let mut doc = self.document.write();
            let version_before = doc.editor.get_version();
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
    pub fn get_full_definition_at(&self, range: &lsp_types::Range) -> Result<ScopedCode> {
        use crate::parser::ast_utils::{extract_definition_content, find_node_at_byte_position};

        // Use the existing tree from this document
        let doc = self.document.read();
        let tree = doc
            .editor
            .tree()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;
        let rope = doc.editor.rope();

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
    pub async fn get_definition_at_path_with_symbol(
        &self,
        ast_path: &[crate::parser::target::PathSegment],
        symbol_name: Option<&str>,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        use crate::parser::ast_utils::{find_node_by_path, get_definition_target_node};

        // Get tree, rope, snapshot and uri
        let (tree, rope, snapshot, uri) = {
            let doc = self.document.read();

            let tree = doc
                .editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
                .clone();

            let rope = doc.editor.rope().clone();
            let snapshot = doc.editor.fork();
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
    async fn format_document(&self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            tracing::trace!("Document formatting not supported");
            return Ok(());
        }

        let (uri_str, version, snapshot) = {
            let doc = self.document.read();
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
            }
            Some(_) | None => {
                // Formatting returned empty edits or None
            }
        }

        Ok(())
    }
}
