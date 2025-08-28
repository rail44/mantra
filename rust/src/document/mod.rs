use anyhow::Result;
use lsp_types::{
    DidChangeTextDocumentParams, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
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

                        // Collect type references
                        let type_references = collect_type_references(&node, &tree.root_node());

                        // Create the base target for checksum calculation
                        let base_target = Target {
                            instruction: instruction.clone(),
                            signature: signature.clone(),
                            checksum: 0, // Will be calculated next
                            snapshot: snapshot.clone(),
                            byte_range: node.start_byte()..node.end_byte(),
                            type_references,
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
        let change =
            self.editor
                .apply_byte_edit(&target.byte_range, replacement, target.snapshot.fork())?;

        Ok(vec![change])
    }

    /// Get text content
    pub fn get_text(&self) -> String {
        self.editor.get_text()
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
            let document_service = self.clone();

            let clone = self.clone();
            set.spawn(Box::pin(async move {
                let new_body = spawn_generation_task(&target, llm_client, document_service).await?;
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

    /// Get content at a specific range (currently returns whole lines)
    pub fn get_content_at_range(&self, range: &lsp_types::Range) -> Result<String> {
        let doc = self
            .document
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        let rope = doc.editor.rope();

        // TODO: Handle exact character positions with UTF-16
        // For MVP, get the whole line(s) covered by the range
        let start_line = range.start.line as usize;
        let end_line = range.end.line as usize;

        let start_byte = rope.byte_of_line(start_line);
        let end_byte = if end_line + 1 < rope.line_len() {
            rope.byte_of_line(end_line + 1)
        } else {
            rope.byte_len()
        };

        Ok(rope.byte_slice(start_byte..end_byte).to_string())
    }

    /// Get definition location for a node at the given AST path
    pub async fn get_definition_at_path(
        &self,
        ast_path: &[crate::parser::target::PathSegment],
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        use crate::parser::ast_utils::find_node_by_path;

        // Get tree and snapshot
        let (tree, snapshot, uri) = {
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

            let tree = doc
                .editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
                .clone();

            let snapshot = doc.editor.fork();
            let uri = doc.uri.clone();

            (tree, snapshot, uri)
        };

        // Find node by path
        let node = find_node_by_path(&tree.root_node(), ast_path)
            .ok_or_else(|| anyhow::anyhow!("Node not found at path"))?;

        // Convert byte position to LSP position
        let position = snapshot.byte_to_lsp_position(node.start_byte());

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

    /// Get hover information for a type at a given AST path
    pub async fn get_hover_for_path(
        &self,
        ast_path: &[crate::parser::target::PathSegment],
    ) -> Result<Option<String>> {
        use crate::parser::ast_utils::find_node_by_path;

        tracing::trace!("Getting hover for path: {:?}", ast_path);

        // Get current document state
        let (tree, uri, snapshot) = {
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

            let tree = doc
                .editor
                .tree()
                .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?
                .clone();

            let uri = doc.uri.clone();
            let snapshot = doc.editor.fork();

            (tree, uri, snapshot)
        };

        // Find node by path
        if let Some(node) = find_node_by_path(&tree.root_node(), ast_path) {
            // Convert byte position to LSP position
            let position = snapshot.byte_to_lsp_position(node.start_byte());

            // Request hover from LSP
            let text_document = lsp_types::TextDocumentIdentifier { uri: uri.parse()? };

            if let Some(hover) = self.lsp_client.hover(text_document, position).await? {
                // Extract hover content
                return Ok(Some(extract_hover_content(hover)));
            }
        }

        Ok(None)
    }
}

/// Collect type references from a function/method declaration
fn collect_type_references(
    func_node: &tree_sitter::Node,
    root_node: &tree_sitter::Node,
) -> Vec<Vec<crate::parser::target::PathSegment>> {
    let mut type_references = Vec::new();

    // For method declarations, collect receiver type
    if func_node.kind() == "method_declaration" {
        if let Some(receiver_list) = func_node.child_by_field_name("receiver") {
            collect_types_from_node(&receiver_list, root_node, &mut type_references);
        }
    }

    // Collect parameter types
    if let Some(params) = func_node.child_by_field_name("parameters") {
        collect_types_from_node(&params, root_node, &mut type_references);
    }

    // Collect return types
    if let Some(result) = func_node.child_by_field_name("result") {
        collect_types_from_node(&result, root_node, &mut type_references);
    }

    type_references
}

/// Recursively collect type_identifier nodes
fn collect_types_from_node(
    node: &tree_sitter::Node,
    root_node: &tree_sitter::Node,
    type_references: &mut Vec<Vec<crate::parser::target::PathSegment>>,
) {
    use crate::parser::ast_utils::build_path_to_node;

    match node.kind() {
        "type_identifier" => {
            // Build path from root to this type node
            let path = build_path_to_node(node, root_node);
            type_references.push(path);
        }
        "qualified_type" => {
            // For qualified types like time.Duration, find the last type_identifier
            let mut cursor = node.walk();
            let mut last_type_identifier = None;
            for child in node.children(&mut cursor) {
                if child.kind() == "type_identifier" {
                    last_type_identifier = Some(child);
                }
            }

            if let Some(type_node) = last_type_identifier {
                let path = build_path_to_node(&type_node, root_node);
                type_references.push(path);
            }
        }
        _ => {
            // Recursively check children for other node types
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_types_from_node(&child, root_node, type_references);
            }
        }
    }
}

/// Extract hover content as a string
fn extract_hover_content(hover: lsp_types::Hover) -> String {
    use lsp_types::{HoverContents, MarkedString};

    match hover.contents {
        HoverContents::Scalar(scalar) => match scalar {
            MarkedString::String(s) => s,
            MarkedString::LanguageString(ls) => ls.value,
        },
        HoverContents::Array(array) => array
            .into_iter()
            .map(|ms| match ms {
                MarkedString::String(s) => s,
                MarkedString::LanguageString(ls) => ls.value,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        HoverContents::Markup(markup) => markup.value,
    }
}
