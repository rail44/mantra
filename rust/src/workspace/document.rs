use actix::prelude::*;
use anyhow::{Context as AnyhowContext, Result};
use std::path::PathBuf;
use tracing::{debug, error};

use super::actor::Workspace;
use super::messages::{
    ApplyEdit, DocumentShutdown, GenerateAll, GetFileUri, GetSource, GetTargetInfo,
};
use crate::config::Config;
use crate::editor::IncrementalEditor;
use crate::generation::EditEvent;
use crate::parser::{target_map::TargetMap, GoParser};
use tree_sitter::{InputEdit, Point, Tree};

// ============================================================================
// Document Actor
// ============================================================================

/// Document actor managing a single document
pub struct DocumentActor {
    uri: String,
    workspace: Addr<Workspace>,
    parser: GoParser,
    tree: Tree,
    editor: IncrementalEditor,
    document_version: i32,
}

impl DocumentActor {
    pub async fn new(
        _config: Config,
        file_path: PathBuf,
        uri: String,
        workspace: Addr<Workspace>,
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

        // Initialize editor with the content
        let editor = IncrementalEditor::new(content);

        Ok(Self {
            uri,
            workspace,
            parser,
            tree,
            editor,
            document_version: 1,
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
        let source = self.editor.source();
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

                    match crate::generation::generate_for_target(
                        checksum,
                        document_addr.clone(),
                        workspace.clone(),
                    )
                    .await
                    {
                        Ok(generated_code) => {
                            debug!("Successfully generated code for checksum {:x}", checksum);
                            let edit = crate::generation::EditEvent::new(
                                checksum,
                                signature,
                                generated_code,
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
            .map(|edits, act, _ctx| {
                // Apply all edits
                for edit in edits {
                    if let Err(e) = act.apply_edit_internal(edit) {
                        error!("Failed to apply edit: {}", e);
                    }
                }
                Ok(act.editor.source().to_string())
            }),
        )
    }
}

impl Handler<ApplyEdit> for DocumentActor {
    type Result = Result<()>;

    fn handle(&mut self, msg: ApplyEdit, _ctx: &mut Context<Self>) -> Self::Result {
        self.apply_edit_internal(msg.edit)
    }
}

impl Handler<GetTargetInfo> for DocumentActor {
    type Result = Result<(crate::parser::target::Target, String, u32, u32)>;

    fn handle(&mut self, msg: GetTargetInfo, _ctx: &mut Context<Self>) -> Self::Result {
        let source = self.editor.source();
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
        Ok(self.editor.source().to_string())
    }
}

impl Handler<DocumentShutdown> for DocumentActor {
    type Result = ();

    fn handle(&mut self, _msg: DocumentShutdown, ctx: &mut Context<Self>) -> Self::Result {
        debug!("Shutting down DocumentActor: {}", self.uri);
        ctx.stop();
    }
}

// ============================================================================
// Helper Methods
// ============================================================================

impl DocumentActor {
    /// Apply an edit to the document (internal method)
    fn apply_edit_internal(&mut self, edit: EditEvent) -> Result<()> {
        let checksum = edit.checksum;
        let content = edit.new_body;

        // Find the target by checksum - extract all needed data and drop target_map
        let source = self.editor.source().to_string();
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
                        .replace(checksum_line_start, func_end_byte, replacement);
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
                        .replace(func_start_byte, func_end_byte, replacement);
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
                    .replace(body_content_start, body_content_end, content);
            }

            // Reparse the tree
            let new_source = self.editor.source();
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
        } else {
            error!("Target not found for checksum {:x}", checksum);
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
