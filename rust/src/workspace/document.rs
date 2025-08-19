use actix::prelude::*;
use anyhow::{Context as AnyhowContext, Result};
use std::path::PathBuf;
use tracing::{debug, error};

use super::actor::Workspace;
use super::generation_session::GenerationSessionManager;
use super::messages::{
    ApplyEdit, DocumentShutdown, GenerateAll, GetFileUri, GetSource, GetTargetInfo,
};
use crate::config::Config;
use crate::editor::TransactionalCrdtEditor;
use crate::generation::EditEvent;
use crate::lsp::Client as LspClient;
use crate::parser::{target_map::TargetMap, GoParser};
use tree_sitter::{InputEdit, Point, Tree};

// Helper function to apply LSP text edits to a string
fn apply_text_edits(text: &str, edits: &[lsp_types::TextEdit]) -> String {
    // Sort edits in reverse order to apply from end to start
    let mut sorted_edits = edits.to_vec();
    sorted_edits.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then(b.range.start.character.cmp(&a.range.start.character))
    });

    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();

    for edit in sorted_edits {
        let start_line = edit.range.start.line as usize;
        let start_char = edit.range.start.character as usize;
        let end_line = edit.range.end.line as usize;
        let end_char = edit.range.end.character as usize;

        // Skip edit if line indices are out of bounds
        if start_line >= lines.len() || end_line >= lines.len() {
            continue;
        }

        // Handle single-line edit
        if start_line == end_line {
            if let Some(line) = lines.get_mut(start_line) {
                let mut chars: Vec<char> = line.chars().collect();
                let line_len = chars.len();

                // Ensure valid range
                let safe_start = start_char.min(line_len);
                let safe_end = end_char.min(line_len);

                if safe_start <= safe_end {
                    chars.splice(safe_start..safe_end, edit.new_text.chars());
                    *line = chars.into_iter().collect();
                } else {
                    // Invalid range, just insert at safe_start
                    chars.splice(safe_start..safe_start, edit.new_text.chars());
                    *line = chars.into_iter().collect();
                }
            }
        } else {
            // Multi-line edit
            let new_lines: Vec<String> = edit.new_text.lines().map(|s| s.to_string()).collect();

            // Get the parts to keep from start and end lines
            let start_keep = if let Some(line) = lines.get(start_line) {
                line.chars()
                    .take(start_char.min(line.len()))
                    .collect::<String>()
            } else {
                String::new()
            };

            let end_keep = if let Some(line) = lines.get(end_line) {
                let line_len = line.chars().count();
                line.chars()
                    .skip(end_char.min(line_len))
                    .collect::<String>()
            } else {
                String::new()
            };

            // Combine the parts
            let mut replacement = Vec::new();
            if new_lines.is_empty() {
                replacement.push(format!("{}{}", start_keep, end_keep));
            } else {
                for (i, new_line) in new_lines.iter().enumerate() {
                    if i == 0 {
                        replacement.push(format!("{}{}", start_keep, new_line));
                    } else if i == new_lines.len() - 1 {
                        replacement.push(format!("{}{}", new_line, end_keep));
                    } else {
                        replacement.push(new_line.clone());
                    }
                }
            }

            // Replace the lines
            lines.splice(start_line..=end_line, replacement);
        }
    }

    lines.join("\n")
}

/// Document actor managing a single document with CRDT support and transaction management
pub struct DocumentActor {
    uri: String,
    workspace: Addr<Workspace>,
    lsp_client: LspClient, // LSPクライアントの参照
    parser: GoParser,
    tree: Tree,
    editor: TransactionalCrdtEditor, // Transactional CRDT editor for version control
    document_version: i32,
    #[allow(dead_code)]
    generation_manager: GenerationSessionManager, // Manages parallel generation sessions
}

impl DocumentActor {
    pub async fn new(
        _config: Config,
        file_path: PathBuf,
        uri: String,
        workspace: Addr<Workspace>,
        lsp_client: LspClient,
    ) -> Result<Self> {
        // Read the file content
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Initialize parser and parse the document
        let mut parser = GoParser::new()?;
        let tree = parser
            .parse(&content)
            .with_context(|| "Failed to parse Go source")?;

        // Initialize transactional CRDT editor for version control
        let mut editor = TransactionalCrdtEditor::new(&content);

        // Initialize generation session manager
        let generation_manager = GenerationSessionManager::new(&mut editor);

        Ok(Self {
            uri,
            workspace,
            lsp_client,
            parser,
            tree,
            editor,
            document_version: 1,
            generation_manager,
        })
    }
}

impl Actor for DocumentActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("DocumentActor started for: {}", self.uri);
    }
}

impl Handler<GenerateAll> for DocumentActor {
    type Result = ResponseActFuture<Self, Result<String>>;

    fn handle(&mut self, _msg: GenerateAll, ctx: &mut Context<Self>) -> Self::Result {
        debug!("GenerateAll for: {}", self.uri);

        // Build target map
        let source = self.editor.get_text();
        let target_map = match TargetMap::build(&self.tree, source) {
            Ok(map) => map,
            Err(e) => {
                error!("Failed to build target map: {}", e);
                return Box::pin(fut::ready(Err(e)).into_actor(self));
            }
        };

        // Get all targets
        let targets: Vec<(u64, String)> = target_map
            .iter()
            .map(|(checksum, (target, _))| (*checksum, target.signature.clone()))
            .collect();

        if targets.is_empty() {
            debug!("No targets found in document");
            return Box::pin(fut::ready(Ok(source.to_string())).into_actor(self));
        }

        let workspace = self.workspace.clone();
        let document_addr = ctx.address();

        // Generate code for all targets using TargetGenerator
        Box::pin(
            async move {
                let mut edits = Vec::new();

                for (checksum, signature) in targets {
                    debug!("Starting generation for checksum {:x}", checksum);

                    // Get target info for formatting  
                    let _target_info = match document_addr.send(GetTargetInfo { checksum }).await {
                        Ok(Ok((_, _, start_line, end_line))) => Some((start_line, end_line)),
                        _ => None,
                    };

                    match crate::generation::generate_for_target(
                        checksum,
                        document_addr.clone(),
                        workspace.clone(),
                    )
                    .await
                    {
                        Ok(generated_code) => {
                            debug!("Successfully generated code for checksum {:x}", checksum);
                            // Apply generated code and format
                            let formatted_code = generated_code;
                            let edit = crate::generation::EditEvent::new(
                                checksum,
                                signature,
                                formatted_code,
                            );
                            edits.push(edit);
                        }
                        Err(e) => {
                            error!("Failed to generate code for checksum {:x}: {}", checksum, e);
                        }
                    }
                }

                edits
            }
            .into_actor(self)
            .then(|edits, act, ctx| {
                // Send ApplyEdit messages to self for each edit to trigger formatting
                let mut edit_futures = Vec::new();

                for edit in edits {
                    let apply_edit_msg = ApplyEdit { edit };
                    let future = ctx.address().send(apply_edit_msg);
                    edit_futures.push(future);
                }

                // Wait for all edits to complete
                async move {
                    for future in edit_futures {
                        if let Err(e) = future.await {
                            error!("Failed to send ApplyEdit message: {}", e);
                        }
                    }
                    Ok(())
                }
                .into_actor(act)
                .map(|_result: Result<()>, act, _ctx| Ok(act.editor.get_text().to_string()))
            }),
        )
    }
}

impl Handler<ApplyEdit> for DocumentActor {
    type Result = ResponseActFuture<Self, Result<()>>;

    fn handle(&mut self, msg: ApplyEdit, _ctx: &mut Context<Self>) -> Self::Result {
        // Apply the edit first
        if let Err(e) = self.apply_edit_internal(msg.edit) {
            return Box::pin(fut::ready(Err(e)).into_actor(self));
        }

        // Then format the document
        let document_uri = self.uri.clone();
        let lsp_client = self.lsp_client.clone();
        let current_text = self.editor.get_text().to_string();

        Box::pin(
            async move {
                // Check LSP formatting capabilities
                let supports_range = lsp_client.supports_range_formatting().await;
                let supports_document = lsp_client.supports_document_formatting().await;

                if !supports_document && !supports_range {
                    debug!("LSP doesn't support any formatting");
                    return Ok(None);
                }

                debug!("Applying LSP formatting after edit");

                // Send didChange notification
                let params = lsp_types::DidChangeTextDocumentParams {
                    text_document: lsp_types::VersionedTextDocumentIdentifier {
                        uri: document_uri.parse()?,
                        version: 0, // TODO: Track version properly
                    },
                    content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: current_text.clone(),
                    }],
                };

                lsp_client.did_change(params).await?;

                // Request formatting
                let text_document = lsp_types::TextDocumentIdentifier {
                    uri: document_uri.parse()?,
                };

                let formatting_options = lsp_types::FormattingOptions {
                    tab_size: 4,
                    insert_spaces: false, // Go uses tabs
                    trim_trailing_whitespace: Some(true),
                    insert_final_newline: Some(true),
                    trim_final_newlines: Some(true),
                    ..Default::default()
                };

                let edits = if supports_range && !supports_document {
                    // Use range formatting if document formatting is not available
                    let range = lsp_types::Range {
                        start: lsp_types::Position {
                            line: 0,
                            character: 0,
                        },
                        end: lsp_types::Position {
                            line: u32::MAX,
                            character: u32::MAX,
                        },
                    };
                    lsp_client
                        .range_formatting(text_document, range, formatting_options)
                        .await?
                } else {
                    // Prefer document formatting
                    lsp_client
                        .format_document(text_document, formatting_options)
                        .await?
                };

                if let Some(text_edits) = edits {
                    if !text_edits.is_empty() {
                        debug!("Applying {} formatting edits", text_edits.len());
                        Ok(Some(text_edits))
                    } else {
                        debug!("No formatting changes from LSP");
                        Ok(None)
                    }
                } else {
                    debug!("No formatting edits returned from LSP");
                    Ok(None)
                }
            }
            .into_actor(self)
            .map(
                |result: Result<Option<Vec<lsp_types::TextEdit>>, anyhow::Error>, act, _ctx| {
                    match result {
                        Ok(Some(text_edits)) => {
                            // Apply formatting edits to CRDT editor
                            let formatted_text =
                                apply_text_edits(act.editor.get_text(), &text_edits);

                            // Replace entire document with formatted version
                            let doc_len = act.editor.get_text().len();
                            if let Err(e) = act.editor.replace(0, doc_len, &formatted_text) {
                                error!("Failed to apply formatting: {}", e);
                                return Err(anyhow::anyhow!("Failed to apply formatting: {}", e));
                            }

                            // Reparse the tree
                            match act
                                .parser
                                .parse_incremental(&formatted_text, Some(&act.tree))
                            {
                                Ok(new_tree) => {
                                    act.tree = new_tree;
                                }
                                Err(e) => {
                                    error!("Failed to reparse after formatting: {}", e);
                                    return Err(anyhow::anyhow!(
                                        "Failed to reparse after formatting: {}",
                                        e
                                    ));
                                }
                            }
                            Ok(())
                        }
                        Ok(None) => {
                            // No formatting edits
                            Ok(())
                        }
                        Err(e) => {
                            error!("Formatting failed: {}", e);
                            // Continue without formatting
                            Ok(())
                        }
                    }
                },
            ),
        )
    }
}

impl Handler<GetTargetInfo> for DocumentActor {
    type Result = Result<(crate::parser::target::Target, String, u32, u32)>;

    fn handle(&mut self, msg: GetTargetInfo, _ctx: &mut Context<Self>) -> Self::Result {
        let source = self.editor.get_text();
        let target_map = TargetMap::build(&self.tree, source)?;

        if let Some((target, node)) = target_map.get(msg.checksum) {
            let start_line = node.start_position().row as u32;
            let end_line = node.end_position().row as u32;
            let package_name = target_map.package_name().to_string();

            Ok((target.clone(), package_name, start_line, end_line))
        } else {
            Err(anyhow::anyhow!(
                "Target with checksum {:x} not found",
                msg.checksum
            ))
        }
    }
}

impl Handler<GetFileUri> for DocumentActor {
    type Result = Result<String>;

    fn handle(&mut self, _msg: GetFileUri, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.uri.clone())
    }
}

impl Handler<GetSource> for DocumentActor {
    type Result = Result<String>;

    fn handle(&mut self, _msg: GetSource, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.editor.get_text().to_string())
    }
}

impl Handler<DocumentShutdown> for DocumentActor {
    type Result = ();

    fn handle(&mut self, _msg: DocumentShutdown, ctx: &mut Context<Self>) -> Self::Result {
        debug!("Shutting down DocumentActor: {}", self.uri);
        ctx.stop();
    }
}

impl DocumentActor {
    /// Apply an edit to the document (internal method)
    fn apply_edit_internal(&mut self, edit: EditEvent) -> Result<()> {
        let checksum = edit.checksum;
        let content = edit.new_body.clone();
        let transaction_id = format!("edit_{:x}", checksum);

        // Start a transaction for this edit
        self.editor.begin_transaction(
            transaction_id.clone(),
            format!("Apply edit for checksum {:x}", checksum),
        )?;
        // Find the target by checksum - extract all needed data and drop target_map
        let source = self.editor.get_text().to_string();
        let (target_name, func_start_byte, func_end_byte, body_start_byte, body_text) = {
            let target_map = TargetMap::build(&self.tree, &source)?;

            if let Some((target, node)) = target_map.get(checksum) {
                let target_name = target.name.clone();
                let func_start = node.start_byte();
                let func_end = node.end_byte();

                // Get the function body node
                let body_node = node
                    .child_by_field_name("body")
                    .with_context(|| "Function has no body")?;

                let body_start = body_node.start_byte();
                let body_text = body_node.utf8_text(source.as_bytes())?.to_string();

                (
                    Some(target_name),
                    func_start,
                    func_end,
                    body_start,
                    body_text,
                )
            } else {
                (None, 0, 0, 0, String::new())
            }
        }; // target_map is dropped here

        if let Some(target_name) = target_name {
            // Find body boundaries (inside the braces)
            let open_brace_offset = body_text.find('{').unwrap_or(0);
            let close_brace_offset = body_text.rfind('}').unwrap_or(body_text.len());

            let body_content_start = body_start_byte + open_brace_offset + 1;
            let body_content_end = body_start_byte + close_brace_offset;

            // Check if we need to add checksum comment
            let (func_line, _) = self.editor.byte_to_line_col(func_start_byte);

            // Create a combined edit: replace function with checksum comment + signature + new body
            let func_signature = &source[func_start_byte..body_start_byte];

            if func_line > 0 {
                // Check if checksum comment already exists
                let lines: Vec<&str> = source.lines().collect();
                let has_checksum = lines
                    .get(func_line.saturating_sub(1))
                    .map(|line| line.contains("// mantra:checksum:"))
                    .unwrap_or(false);

                if has_checksum {
                    // Update existing checksum: replace from checksum line to end of function
                    let checksum_line_start = source
                        .lines()
                        .take(func_line - 1)
                        .map(|l| l.len() + 1)
                        .sum::<usize>();

                    let replacement = format!(
                        "// mantra:checksum:{:x}\n{} {{\n{}}}",
                        checksum,
                        func_signature.trim_end(),
                        content
                    );

                    let input_edit = self.create_input_edit_for_replace(
                        checksum_line_start,
                        func_end_byte,
                        &replacement,
                    );

                    self.tree.edit(&input_edit);
                    self.editor
                        .replace(checksum_line_start, func_end_byte, &replacement)
                        .with_context(|| "Failed to replace text")?;
                } else {
                    // Add new checksum: replace entire function
                    let replacement = format!(
                        "// mantra:checksum:{:x}\n{} {{\n{}}}",
                        checksum,
                        func_signature.trim_end(),
                        content
                    );

                    let input_edit = self.create_input_edit_for_replace(
                        func_start_byte,
                        func_end_byte,
                        &replacement,
                    );

                    self.tree.edit(&input_edit);
                    self.editor
                        .replace(func_start_byte, func_end_byte, &replacement)
                        .with_context(|| "Failed to replace text")?;
                }
            } else {
                // First line of file - just replace body
                let input_edit = self.create_input_edit_for_replace(
                    body_content_start,
                    body_content_end,
                    &content,
                );

                self.tree.edit(&input_edit);
                self.editor
                    .replace(body_content_start, body_content_end, &content)
                    .with_context(|| "Failed to replace text")?;
            }

            // Reparse the tree
            let new_source = self.editor.get_text();
            self.tree = self
                .parser
                .parse_incremental(new_source, Some(&self.tree))
                .with_context(|| "Failed to reparse after edit")?;

            // Increment document version
            self.document_version += 1;

            debug!(
                "Applied edit for checksum {:x} to {}",
                checksum, target_name
            );

            // Commit the transaction
            self.editor.commit_transaction(&transaction_id)?;
        } else {
            error!("Target not found for checksum {:x}", checksum);
            // Rollback the transaction if target not found
            self.editor.rollback_transaction(&transaction_id)?;
        }

        Ok(())
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
}
