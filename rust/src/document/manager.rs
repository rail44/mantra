use anyhow::Result;
use std::path::Path;
use tokio::sync::{mpsc, oneshot};
use tree_sitter::{InputEdit, Point, Tree};

use crate::config::Config;
use crate::editor::IncrementalEditor;
use crate::lsp::{Position, Range};
use crate::parser::GoParser;

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
    /// Apply generated code edit
    ApplyEdit {
        edit: crate::generation::EditEvent,
        response: oneshot::Sender<Result<()>>,
    },
    /// Get target information by checksum
    GetTargetInfo {
        checksum: u64,
        response: oneshot::Sender<Result<(crate::parser::target::Target, String, u32, u32)>>,
    },
    /// Get file URI
    GetFileUri {
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
    /// Document version for LSP
    document_version: i32,
    /// Workspace channel for sending commands
    workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
    /// Self channel for TargetGenerators
    self_tx: mpsc::Sender<DocumentCommand>,
}

impl DocumentManager {
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
    /// Create a new document manager (internal constructor)
    async fn new_internal(
        _config: Config,
        file_path: &Path,
        workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
        self_tx: mpsc::Sender<DocumentCommand>,
    ) -> Result<Self> {
        let source = std::fs::read_to_string(file_path)?;
        let mut parser = GoParser::new()?;
        let tree = parser.parse(&source)?;
        let editor = IncrementalEditor::new(source.clone());

        Ok(Self {
            parser,
            tree,
            editor,
            file_path: file_path.to_path_buf(),
            document_version: 1,
            workspace_tx,
            self_tx,
        })
    }

    /// Spawn a new DocumentManager actor and return the sender for commands
    pub async fn spawn(
        config: Config,
        file_path: &Path,
        workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
    ) -> Result<mpsc::Sender<DocumentCommand>> {
        let (tx, rx) = mpsc::channel(32);
        let self_tx = tx.clone();

        let mut manager =
            Self::new_internal(config, file_path, workspace_tx.clone(), self_tx).await?;

        tokio::spawn(async move {
            if let Err(e) = manager.run_actor(rx).await {
                tracing::error!("DocumentManager actor failed: {}", e);
            }
        });

        Ok(tx)
    }

    /// Apply edit to the document
    async fn apply_edit(&mut self, edit: crate::generation::EditEvent) -> Result<()> {
        use crate::generation::convert_to_lsp_edits;

        // Convert EditEvent to LSP text edits
        let lsp_edits = convert_to_lsp_edits(self.editor.source(), &self.tree, vec![edit])?;

        // Apply edits to the document
        for text_edit in lsp_edits {
            // Convert editor Position to LSP Position for position_to_byte
            let start_pos = Position {
                line: text_edit.range.start.line,
                character: text_edit.range.start.character,
            };
            let end_pos = Position {
                line: text_edit.range.end.line,
                character: text_edit.range.end.character,
            };

            // Convert LSP range to byte positions
            let start_byte = self.position_to_byte(start_pos)?;
            let end_byte = self.position_to_byte(end_pos)?;

            // Apply edit to the editor
            self.editor
                .replace(start_byte, end_byte, text_edit.new_text.clone());

            // Update parse tree
            let input_edit =
                self.create_input_edit_for_replace(start_byte, end_byte, &text_edit.new_text);
            self.tree.edit(&input_edit);
            // Re-parse the entire document
            self.tree = self.parser.parse(self.editor.source())?;
        }

        // Update version (LSP notifications are handled by Workspace)
        self.send_did_change().await?;

        Ok(())
    }

    /// Send LSP didChange notification via Workspace
    async fn send_did_change(&mut self) -> Result<()> {
        // Increment version
        self.document_version += 1;

        // Get file URI
        let absolute_path = if self.file_path.is_absolute() {
            self.file_path.clone()
        } else {
            std::env::current_dir()?.join(&self.file_path)
        };
        let file_uri = format!("file://{}", absolute_path.to_string_lossy());

        // Get LSP client from Workspace
        let (tx, rx) = oneshot::channel();
        self.workspace_tx
            .send(crate::workspace::WorkspaceCommand::GetLspClient { response: tx })
            .await?;
        let lsp_client = rx.await?;

        // Send didChange notification
        use crate::lsp::client::{TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier};

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

        Ok(())
    }

    /// Run the actor's event loop (internal)
    async fn run_actor(&mut self, mut rx: mpsc::Receiver<DocumentCommand>) -> Result<()> {
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
                    tracing::info!("Received GenerateAll command");
                    // Spawn generation as a separate task to not block the actor
                    let workspace_tx = self.workspace_tx.clone();
                    let self_tx = self.self_tx.clone();
                    let source = self.editor.source().to_string();
                    let tree = self.parser.parse(&source)?;

                    tracing::info!("Spawning generate_all_async task");
                    tokio::spawn(async move {
                        tracing::info!("Started generate_all_async task");
                        let result =
                            Self::generate_all_async(source, tree, workspace_tx, self_tx).await;
                        tracing::info!("Completed generate_all_async task");
                        let _ = response.send(result);
                    });
                    tracing::info!("GenerateAll handler completed, actor loop continues");
                }
                DocumentCommand::ApplyEdit { edit, response } => {
                    let result = self.apply_edit(edit).await;
                    let _ = response.send(result);
                }
                DocumentCommand::GetTargetInfo { checksum, response } => {
                    tracing::debug!("Received GetTargetInfo command for checksum {:x}", checksum);
                    let result = self.get_target_info(checksum);
                    let _ = response.send(result);
                    tracing::debug!("Sent GetTargetInfo response for checksum {:x}", checksum);
                }
                DocumentCommand::GetFileUri { response } => {
                    let file_uri = format!("file://{}", self.file_path.display());
                    let _ = response.send(Ok(file_uri));
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

    /// Get target information by checksum
    fn get_target_info(
        &self,
        checksum: u64,
    ) -> Result<(crate::parser::target::Target, String, u32, u32)> {
        use crate::parser::target_map::TargetMap;

        let target_map = TargetMap::build(&self.tree, self.editor.source())?;

        if let Some((target, node)) = target_map.get(checksum) {
            let start_line = node.start_position().row as u32;
            let end_line = node.end_position().row as u32;
            let package_name = target_map.package_name().to_string();

            Ok((target.clone(), package_name, start_line, end_line))
        } else {
            anyhow::bail!("Target with checksum {:x} not found", checksum)
        }
    }

    /// Generate code for all targets asynchronously
    async fn generate_all_async(
        source: String,
        tree: Tree,
        workspace_tx: mpsc::Sender<crate::workspace::WorkspaceCommand>,
        self_tx: mpsc::Sender<DocumentCommand>,
    ) -> Result<String> {
        use crate::generation::TargetGenerator;
        use crate::parser::target_map::TargetMap;

        tracing::info!("generate_all_async started");

        // Build target map
        let target_map = TargetMap::build(&tree, &source)?;
        tracing::info!("Found {} targets in document", target_map.len());

        // Collect generation tasks
        let mut generation_tasks = Vec::new();
        for (checksum, (target, _node)) in target_map.iter() {
            generation_tasks.push((*checksum, target.signature.clone()));
        }

        tracing::info!("Creating futures for {} targets", generation_tasks.len());

        // Create futures for parallel generation
        let mut futures = Vec::new();
        for (checksum, signature) in generation_tasks {
            let workspace_tx = workspace_tx.clone();
            let document_tx = self_tx.clone();

            let future = async move {
                tracing::info!("Starting generation for checksum {:x}", checksum);
                let target_generator = TargetGenerator::new(checksum, document_tx);

                match target_generator.generate(workspace_tx).await {
                    Ok(generated_code) => {
                        tracing::info!("Successfully generated code for checksum {:x}", checksum);
                        Some(crate::generation::EditEvent::new(
                            checksum,
                            signature,
                            generated_code,
                        ))
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to generate code for checksum {:x}: {}",
                            checksum,
                            e
                        );
                        None
                    }
                }
            };

            futures.push(future);
        }

        // Execute futures sequentially for now (to avoid deadlock)
        // TODO: Implement proper parallel execution
        let mut edits = Vec::new();
        for future in futures {
            if let Some(edit) = future.await {
                edits.push(edit);
            }
        }

        // Apply all edits sequentially
        // We need to send ApplyEdit commands back to the DocumentManager
        for edit in edits {
            let (tx, rx) = oneshot::channel();
            self_tx
                .send(DocumentCommand::ApplyEdit { edit, response: tx })
                .await?;
            rx.await??;
        }

        // Get the final source
        let (tx, rx) = oneshot::channel();
        self_tx
            .send(DocumentCommand::GetSource { response: tx })
            .await?;
        rx.await?
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
