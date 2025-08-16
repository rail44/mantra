use anyhow::Result;
use std::path::Path;
use tokio::sync::{mpsc, oneshot};
use tree_sitter::{InputEdit, Point, Tree};

use crate::config::Config;
use crate::editor::{indent_code, IncrementalEditor};
use crate::generation::EditEvent;
use crate::llm::LLMClient;
use crate::lsp::{
    client::{TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier},
    Client as LspClient, Position, Range, TextDocumentItem,
};
use crate::parser::{target_map::TargetMap, GoParser};

/// Commands that can be sent to a DocumentManager actor
pub enum DocumentCommand {
    /// Find a symbol within a range and return its position
    FindSymbol {
        range: Range,
        symbol: String,
        response: oneshot::Sender<Result<Position>>,
    },
    /// Get text within a range
    GetText {
        range: Range,
        response: oneshot::Sender<Result<String>>,
    },
    /// Get the full source text
    GetSource {
        response: oneshot::Sender<Result<String>>,
    },
    /// Get a copy of the parse tree
    GetTree {
        response: oneshot::Sender<Result<Tree>>,
    },
    /// Get definition block at a position
    GetDefinitionBlock {
        position: Position,
        response: oneshot::Sender<Result<(Range, String)>>,
    },
    /// Generate code for all targets
    GenerateAll {
        response: oneshot::Sender<Result<String>>,
    },
    /// Shutdown the actor
    Shutdown,
}

/// Manages document state including Tree-sitter tree and coordinates generation
pub struct DocumentManager {
    /// Parser instance
    parser: GoParser,
    /// Current parse tree
    tree: Tree,
    /// Text editor
    editor: IncrementalEditor,
    /// File path
    file_path: std::path::PathBuf,
    /// LSP client (optional, for LSP mode)
    lsp_client: Option<LspClient>,
    /// LLM client (shared across all target generators)
    llm_client: LLMClient,
    /// Document version for LSP
    document_version: i32,
    /// Workspace channel for sending commands
    workspace_tx: Option<mpsc::Sender<crate::workspace::WorkspaceCommand>>,
}

impl DocumentManager {
    /// Create InputEdit for an insert operation
    fn create_input_edit_for_insert(&self, position: usize, text: &str) -> InputEdit {
        let (start_line, start_col) = self.editor.byte_to_line_col(position);
        let start_point = Point::new(start_line, start_col);

        // Calculate new end position
        let new_end_byte = position + text.len();
        let lines: Vec<&str> = text.lines().collect();
        let new_end_point = if lines.len() > 1 {
            // Multi-line insert
            Point::new(
                start_line + lines.len() - 1,
                lines.last().map(|s| s.len()).unwrap_or(0),
            )
        } else {
            // Single line insert
            Point::new(start_line, start_col + text.len())
        };

        InputEdit {
            start_byte: position,
            old_end_byte: position,
            new_end_byte,
            start_position: start_point,
            old_end_position: start_point,
            new_end_position: new_end_point,
        }
    }

    /// Create InputEdit for a replace operation
    fn create_input_edit_for_replace(&self, start: usize, end: usize, text: &str) -> InputEdit {
        let (start_line, start_col) = self.editor.byte_to_line_col(start);
        let (end_line, end_col) = self.editor.byte_to_line_col(end);
        let start_point = Point::new(start_line, start_col);
        let old_end_point = Point::new(end_line, end_col);

        // Calculate new end position
        let new_end_byte = start + text.len();
        let lines: Vec<&str> = text.lines().collect();
        let new_end_point = if lines.len() > 1 {
            // Multi-line replacement
            Point::new(
                start_line + lines.len() - 1,
                lines.last().map(|s| s.len()).unwrap_or(0),
            )
        } else {
            // Single line replacement
            Point::new(start_line, start_col + text.len())
        };

        InputEdit {
            start_byte: start,
            old_end_byte: end,
            new_end_byte,
            start_position: start_point,
            old_end_position: old_end_point,
            new_end_position: new_end_point,
        }
    }
    /// Create a new document manager with LSP support  
    pub async fn new(config: Config, file_path: &Path) -> Result<Self> {
        // Create a dummy workspace_tx for standalone usage
        let (workspace_tx, _) = mpsc::channel(32);
        Self::new_with_workspace(config, file_path, workspace_tx).await
    }

    /// Create a new document manager with LSP support (internal constructor)
    async fn new_with_workspace(
        config: Config,
        file_path: &Path,
        workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
    ) -> Result<Self> {
        let source = std::fs::read_to_string(file_path)?;
        let mut parser = GoParser::new()?;
        let tree = parser.parse(&source)?;
        let editor = IncrementalEditor::new(source.clone());

        // Initialize LSP client
        let lsp_client = LspClient::new("gopls", &[]).await?;

        // Get absolute path for LSP
        let absolute_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(file_path)
        };

        let workspace_root = absolute_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        // Initialize LSP
        let _init_result = lsp_client
            .initialize(
                Some(std::process::id()),
                Some(format!("file://{}", workspace_root)),
                serde_json::json!({
                    "textDocument": {
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "synchronization": {
                            "didOpen": true,
                            "didChange": true
                        }
                    }
                }),
                Some(vec![serde_json::json!({
                    "uri": format!("file://{}", workspace_root),
                    "name": "workspace"
                })]),
            )
            .await?;
        lsp_client.initialized().await?;

        // Open the document
        let file_uri = format!("file://{}", absolute_path.to_string_lossy());
        lsp_client
            .did_open(TextDocumentItem {
                uri: file_uri,
                language_id: "go".to_string(),
                version: 1,
                text: source.clone(),
            })
            .await?;

        // Create LLM client (shared across all target generators)
        let llm_client = LLMClient::new(config.clone())?;

        Ok(Self {
            parser,
            tree,
            editor,
            file_path: file_path.to_path_buf(),
            lsp_client: Some(lsp_client),
            llm_client,
            document_version: 1,
            workspace_tx: Some(workspace_tx),
        })
    }

    /// Spawn a new DocumentManager actor and return the sender for commands
    pub async fn spawn(
        config: Config,
        file_path: &Path,
        workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
    ) -> Result<mpsc::Sender<DocumentCommand>> {
        let (tx, rx) = mpsc::channel(32);

        let mut manager = Self::new_with_workspace(config, file_path, workspace_tx.clone()).await?;

        tokio::spawn(async move {
            if let Err(e) = manager.run_actor(workspace_tx, rx).await {
                tracing::error!("DocumentManager actor failed: {}", e);
            }
        });

        Ok(tx)
    }

    /// Generate code for all targets in the document
    pub async fn generate_all(&mut self) -> Result<String> {
        // For now, just return the source unchanged
        // Real implementation would modify the source with generated code
        // But that requires passing workspace channel here, which is complex
        tracing::warn!("generate_all is not yet implemented - returning unchanged source");
        Ok(self.editor.source().to_string())
    }

    /// Prepare generation tasks by collecting target information
    fn prepare_generation_tasks(&self) -> Result<Vec<GenerationTask>> {
        let target_map = TargetMap::build(&self.tree, self.editor.source())?;

        tracing::info!("Found {} targets in file", target_map.len());
        for target in target_map.targets() {
            tracing::info!("  - {} ({})", target.name, target.instruction);
        }

        // Collect checksums for parallel generation
        let tasks = target_map
            .checksums()
            .map(|checksum| GenerationTask { checksum })
            .collect();

        Ok(tasks)
    }

    // TODO: Remove or refactor this method once Workspace-based generation is complete
    #[allow(dead_code)]
    async fn generate_all_targets(
        &self,
        _tasks: Vec<GenerationTask>,
    ) -> Result<Vec<GeneratedEvent>> {
        // Placeholder - actual implementation moved to Workspace
        Ok(vec![])
    }

    /// Apply a generated event to the document
    async fn apply_generated_event(&mut self, event: GeneratedEvent) -> Result<()> {
        // Re-build target map for current state
        let target_map = TargetMap::build(&self.tree, self.editor.source())?;

        // Find the node for this function by name
        let node_data = target_map
            .targets()
            .zip(target_map.nodes())
            .find(|(t, _)| t.name == event.target_name)
            .map(|(_, node)| {
                let func_start = node.start_byte();
                let body_info = node
                    .child_by_field_name("body")
                    .map(|b| (b.start_byte(), b.end_byte()));
                (func_start, body_info)
            });

        // Drop target_map before processing
        drop(target_map);

        if let Some((func_start, body_info)) = node_data {
            // Create EditEvent
            let edit_event = EditEvent::new(event.checksum, event.signature, event.generated_code);

            // Apply the edit event
            self.apply_edit_event(edit_event, func_start, body_info)
                .await?;

            tracing::debug!(
                "Applied edit for {} (checksum: {:x})",
                event.target_name,
                event.checksum
            );
        }

        Ok(())
    }

    /// Send LSP didChange notification
    async fn send_did_change(&mut self) -> Result<()> {
        if let Some(lsp_client) = &self.lsp_client {
            // Increment version
            self.document_version += 1;

            // Get file URI
            let absolute_path = if self.file_path.is_absolute() {
                self.file_path.clone()
            } else {
                std::env::current_dir()?.join(&self.file_path)
            };
            let file_uri = format!("file://{}", absolute_path.to_string_lossy());

            // Send full document content as change
            let content_change = TextDocumentContentChangeEvent {
                text: self.editor.source().to_string(),
            };

            lsp_client
                .did_change(
                    VersionedTextDocumentIdentifier {
                        uri: file_uri,
                        version: self.document_version,
                    },
                    vec![content_change],
                )
                .await?;

            tracing::debug!(
                "Sent didChange notification (version: {})",
                self.document_version
            );
        }
        Ok(())
    }

    /// Apply an EditEvent to the document
    async fn apply_edit_event(
        &mut self,
        event: EditEvent,
        func_start_byte: usize,
        body_info: Option<(usize, usize)>,
    ) -> Result<()> {
        // Check if there's already a checksum comment
        let lines: Vec<&str> = self.editor.source().lines().collect();
        let (func_line, _) = self.editor.byte_to_line_col(func_start_byte);
        let has_checksum = func_line > 0
            && lines
                .get(func_line - 1)
                .map(|line| line.contains("// mantra:checksum:"))
                .unwrap_or(false);

        // Track if any changes were made
        let mut changes_made = false;

        // Add checksum comment if not present
        if !has_checksum && func_line > 0 {
            let line_start_byte = self.editor.line_col_to_byte(func_line, 0);
            let checksum_comment = format!("// mantra:checksum:{:x}\n", event.checksum);

            // Create InputEdit for incremental parsing
            let input_edit = self.create_input_edit_for_insert(line_start_byte, &checksum_comment);

            // Apply edit to source
            self.editor.insert(line_start_byte, checksum_comment);

            // Update tree incrementally
            self.tree.edit(&input_edit);
            self.tree = self
                .parser
                .parse_incremental(self.editor.source(), Some(&self.tree))?;

            changes_made = true;
        }

        // Replace the function body if it exists
        if let Some((_body_start, _body_end)) = body_info {
            // Re-calculate positions after potential checksum insertion
            let (new_body_start, new_body_end) = {
                let target_map = TargetMap::build(&self.tree, self.editor.source())?;

                // Find the updated body position
                let body_pos = target_map
                    .targets()
                    .zip(target_map.nodes())
                    .find(|(t, _)| t.signature == event.signature)
                    .and_then(|(_, node)| {
                        node.child_by_field_name("body")
                            .map(|body| (body.start_byte(), body.end_byte()))
                    });

                body_pos
            }
            .unwrap_or((0, 0));

            if new_body_start > 0 {
                // Format the new body with proper indentation
                let formatted_body = if event.new_body.trim().is_empty() {
                    "{\n\tpanic(\"not implemented\")\n}".to_string()
                } else {
                    let indented = indent_code(&event.new_body, "\t");
                    format!("{{\n{}\n}}", indented)
                };

                // Create InputEdit for incremental parsing
                let input_edit = self.create_input_edit_for_replace(
                    new_body_start,
                    new_body_end,
                    &formatted_body,
                );

                // Apply edit to source
                self.editor
                    .replace(new_body_start, new_body_end, formatted_body);

                // Update tree incrementally
                self.tree.edit(&input_edit);
                self.tree = self
                    .parser
                    .parse_incremental(self.editor.source(), Some(&self.tree))?;

                changes_made = true;
            }
        }

        // Send didChange notification if LSP is active and changes were made
        if changes_made {
            self.send_did_change().await?;
        }

        Ok(())
    }

    /// Run the actor's event loop (internal)
    async fn run_actor(
        &mut self,
        _workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
        mut rx: mpsc::Receiver<DocumentCommand>,
    ) -> Result<()> {
        // Workspace channel is already set in constructor

        while let Some(command) = rx.recv().await {
            match command {
                DocumentCommand::FindSymbol {
                    range,
                    symbol,
                    response,
                } => {
                    let _ = response.send(self.find_symbol(range, symbol));
                }
                DocumentCommand::GetText { range, response } => {
                    let _ = response.send(self.get_text(range));
                }
                DocumentCommand::GetSource { response } => {
                    let _ = response.send(Ok(self.editor.source().to_string()));
                }
                DocumentCommand::GetTree { response } => {
                    // Parse a fresh copy since Tree doesn't implement Clone
                    let tree = self.parser.parse(self.editor.source());
                    let _ = response.send(tree);
                }
                DocumentCommand::GetDefinitionBlock { position, response } => {
                    let _ = response.send(self.get_definition_block(position));
                }
                DocumentCommand::GenerateAll { response } => {
                    let result = self.generate_all().await;
                    let _ = response.send(result);
                }
                DocumentCommand::Shutdown => {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Find a symbol within a range
    fn find_symbol(&self, range: Range, symbol: String) -> Result<Position> {
        // Convert LSP range to byte offsets
        let start_byte = self.position_to_byte(range.start)?;
        let end_byte = self.position_to_byte(range.end)?;

        // Find the symbol in the tree within the range
        let root = self.tree.root_node();
        if let Some(position) = self.find_symbol_in_node(&root, &symbol, start_byte, end_byte) {
            Ok(position)
        } else {
            anyhow::bail!("Symbol '{}' not found in range", symbol)
        }
    }

    /// Find symbol in node recursively
    fn find_symbol_in_node(
        &self,
        node: &tree_sitter::Node,
        symbol: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> Option<Position> {
        // Check if node is within range
        if node.end_byte() < start_byte || node.start_byte() > end_byte {
            return None;
        }

        // Check if this node is an identifier or type_identifier with matching text
        if node.kind() == "identifier" || node.kind() == "type_identifier" {
            if let Ok(text) = node.utf8_text(self.editor.source().as_bytes()) {
                if text == symbol {
                    let point = node.start_position();
                    return Some(Position {
                        line: point.row as u32,
                        character: point.column as u32,
                    });
                }
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(pos) = self.find_symbol_in_node(&child, symbol, start_byte, end_byte) {
                return Some(pos);
            }
        }

        None
    }

    /// Get text within a range
    fn get_text(&self, range: Range) -> Result<String> {
        let start_byte = self.position_to_byte(range.start)?;
        let end_byte = self.position_to_byte(range.end)?;

        let source = self.editor.source();
        if start_byte > source.len() || end_byte > source.len() {
            anyhow::bail!("Range out of bounds");
        }

        Ok(source[start_byte..end_byte].to_string())
    }

    /// Convert LSP Position to byte offset
    fn position_to_byte(&self, position: Position) -> Result<usize> {
        let (line, col) = (position.line as usize, position.character as usize);
        Ok(self.editor.line_col_to_byte(line, col))
    }

    /// Get the complete definition block at a position
    fn get_definition_block(&self, position: Position) -> Result<(Range, String)> {
        let byte_offset = self.position_to_byte(position)?;

        // Find the definition node containing this position
        let root = self.tree.root_node();
        let definition_node = Self::find_definition_node(&root, byte_offset)
            .ok_or_else(|| anyhow::anyhow!("No definition found at position"))?;

        // Convert node bounds to Range
        let start_point = definition_node.start_position();
        let end_point = definition_node.end_position();

        let range = Range {
            start: Position {
                line: start_point.row as u32,
                character: start_point.column as u32,
            },
            end: Position {
                line: end_point.row as u32,
                character: end_point.column as u32,
            },
        };

        // Extract the text
        let text = definition_node
            .utf8_text(self.editor.source().as_bytes())?
            .to_string();

        Ok((range, text))
    }

    /// Find the definition node (struct, function, etc.) containing the given byte
    fn find_definition_node<'a>(
        node: &tree_sitter::Node<'a>,
        byte_offset: usize,
    ) -> Option<tree_sitter::Node<'a>> {
        // Check if this node contains the byte offset
        if node.start_byte() > byte_offset || node.end_byte() < byte_offset {
            return None;
        }

        // Check if this is a complete definition node type
        let is_complete_definition = matches!(
            node.kind(),
            "type_declaration"  // Complete: type User struct { ... }
                | "function_declaration"  // Complete: func Foo() { ... }
                | "method_declaration"  // Complete: func (r Receiver) Method() { ... }
                | "const_declaration"  // Complete: const FOO = ...
                | "var_declaration" // Complete: var foo = ...
        );

        // If this is a complete definition, return it immediately
        if is_complete_definition {
            return Some(*node);
        }

        // Check if this is a partial definition that needs parent
        let needs_parent = matches!(
            node.kind(),
            "type_spec"  // Part of type_declaration
                | "struct_type"  // Just the struct body
                | "interface_type" // Just the interface body
        );

        // Try to find a more specific child node first
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.start_byte() <= byte_offset && child.end_byte() >= byte_offset {
                if let Some(def_node) = Self::find_definition_node(&child, byte_offset) {
                    return Some(def_node);
                }
            }
        }

        // If this is a partial definition that needs parent context,
        // we'll return None here and let the parent be found
        if needs_parent {
            return None;
        }

        None
    }
}

/// Task for generating code for a single target  
struct GenerationTask {
    checksum: u64,
}

/// Result of code generation for a single target
struct GeneratedEvent {
    checksum: u64,
    target_name: String,
    signature: String,
    generated_code: String,
}
