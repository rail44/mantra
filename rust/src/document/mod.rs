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
}

impl Document {
    pub fn new(file_path: &PathBuf, uri: String) -> Result<Self> {
        let content = fs::read_to_string(file_path)
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
                clone.apply_generation(target, &new_body).await?;
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

    async fn apply_generation(&self, target: Target, new_body: &str) -> Result<()> {
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
    pub fn get_full_definition_at(&self, range: &lsp_types::Range) -> Result<ScopedCode> {
        use crate::parser::ast_utils::{extract_definition_content, find_node_at_byte_position};

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
            let doc = self
                .document
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

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
            }
            Some(_) | None => {
                // Formatting returned empty edits or None
            }
        }

        Ok(())
    }
}
