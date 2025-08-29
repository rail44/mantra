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
use crate::parser::{
    checksum::calculate_checksum,
    target::{Target, TypeReference},
};
use crate::workspace::WorkspaceService;

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
                        let type_references = self.collect_type_references(&node);

                        // Create the base target for checksum calculation
                        let base_target = Target {
                            uri: self.uri.clone(),
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

    /// Collect type references from a function/method declaration
    fn collect_type_references(&self, func_node: &tree_sitter::Node) -> Vec<TypeReference> {
        let tree = self.editor.tree().unwrap();
        let rope = self.editor.rope();
        let root_node = tree.root_node();
        let mut type_references = Vec::new();

        // For method declarations, collect receiver type
        if func_node.kind() == "method_declaration" {
            if let Some(receiver_list) = func_node.child_by_field_name("receiver") {
                self.collect_types_from_node(
                    &receiver_list,
                    &root_node,
                    rope,
                    &mut type_references,
                );
            }
        }

        // Collect parameter types
        if let Some(params) = func_node.child_by_field_name("parameters") {
            self.collect_types_from_node(&params, &root_node, rope, &mut type_references);
        }

        // Collect return types
        if let Some(result) = func_node.child_by_field_name("result") {
            self.collect_types_from_node(&result, &root_node, rope, &mut type_references);
        }

        type_references
    }

    /// Recursively collect type nodes and create TypeReference objects
    fn collect_types_from_node(
        &self,
        node: &tree_sitter::Node,
        root_node: &tree_sitter::Node,
        rope: &crop::Rope,
        type_references: &mut Vec<TypeReference>,
    ) {
        use crate::parser::ast_utils::build_path_to_node;

        match node.kind() {
            "type_identifier" | "pointer_type" | "slice_type" | "array_type" | "channel_type"
            | "map_type" => {
                // Build path from root to this type node
                let path = build_path_to_node(node, root_node);
                // Extract type name from the entire type node (preserves modifiers like *, [])
                let scope_id = rope
                    .byte_slice(node.start_byte()..node.end_byte())
                    .to_string();
                type_references.push(TypeReference { path, scope_id });
            }
            "qualified_type" => {
                // For qualified types like time.Duration, get the entire qualified type
                let path = build_path_to_node(node, root_node);
                let scope_id = rope
                    .byte_slice(node.start_byte()..node.end_byte())
                    .to_string();
                type_references.push(TypeReference { path, scope_id });
            }
            _ => {
                // Recursively check children for other node types
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.collect_types_from_node(&child, root_node, rope, type_references);
                }
            }
        }
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
            let workspace = self.workspace.clone();

            let clone = self.clone();
            set.spawn(Box::pin(async move {
                let new_body = spawn_generation_task(&target, llm_client, &workspace).await?;
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

    /// Get the full definition at a range using tree-sitter
    pub async fn get_full_definition_at(&self, range: &lsp_types::Range) -> Result<String> {
        // Use the existing tree from this document
        let doc = self.document.read().unwrap();
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
        let node = root
            .descendant_for_byte_range(byte_pos, byte_pos)
            .ok_or_else(|| anyhow::anyhow!("No node at position"))?;

        // Walk up the tree to find a definition node
        // In Go, we're looking for type_spec, const_spec, var_spec, function_declaration, method_declaration
        let mut definition_range = None;
        let mut current = Some(node);

        while let Some(n) = current {
            match n.kind() {
                // Type definitions
                "type_spec" | "type_declaration" => {
                    definition_range = Some((n.start_byte(), n.end_byte()));
                    break;
                }
                // Constant definitions
                "const_spec" | "const_declaration" => {
                    definition_range = Some((n.start_byte(), n.end_byte()));
                    break;
                }
                // Variable definitions
                "var_spec" | "var_declaration" => {
                    definition_range = Some((n.start_byte(), n.end_byte()));
                    break;
                }
                // Function/method definitions
                "function_declaration" | "method_declaration" => {
                    // For functions, we typically want just the signature, not the body
                    if let Some(params) = n.child_by_field_name("parameters") {
                        // Get from start of function to end of parameters
                        definition_range = Some((n.start_byte(), params.end_byte()));
                    } else {
                        definition_range = Some((n.start_byte(), n.end_byte()));
                    }
                    break;
                }
                // Interface method specifications
                "method_spec" => {
                    definition_range = Some((n.start_byte(), n.end_byte()));
                    break;
                }
                // Field declarations in structs
                "field_declaration" => {
                    definition_range = Some((n.start_byte(), n.end_byte()));
                    break;
                }
                _ => {
                    current = n.parent();
                }
            }
        }

        // If we found a definition, return its content
        if let Some((start, end)) = definition_range {
            Ok(rope.byte_slice(start..end).to_string())
        } else {
            // If we couldn't find a definition node, the position might be pointing
            // to an identifier that is the definition itself
            if node.kind() == "type_identifier" || node.kind() == "identifier" {
                // Return just the identifier
                Ok(rope
                    .byte_slice(node.start_byte()..node.end_byte())
                    .to_string())
            } else {
                Err(anyhow::anyhow!(
                    "Could not find definition node at position. Found '{}' instead",
                    node.kind()
                ))
            }
        }
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
