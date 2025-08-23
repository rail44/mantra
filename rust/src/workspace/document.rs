use anyhow::{Context as AnyhowContext, Result};
use std::path::PathBuf;
use tracing::{debug, error};

use crate::editor::crdt::CrdtEditor;
use crate::generation::EditEvent;
use crate::parser::{target::Target, target_map::TargetMap, GoParser};
use lsp_types::Range;
use tree_sitter::Tree;

/// Document managing a single document's state with CRDT support
pub struct Document {
    pub uri: String,
    pub file_path: PathBuf,
    pub parser: GoParser,
    pub tree: Tree,
    pub editor: CrdtEditor,
    /// Snapshot of the version that LSP knows about
    pub lsp_snapshot: CrdtEditor,
}

impl Document {
    pub fn new(file_path: PathBuf, uri: String, content: &str) -> Result<Self> {
        // Initialize parser and parse the document
        let mut parser = GoParser::new()?;
        let tree = parser
            .parse(&content)
            .with_context(|| "Failed to parse Go source")?;

        // Initialize CRDT editor
        let editor = CrdtEditor::new(&content);
        let lsp_snapshot = editor.fork();

        Ok(Self {
            uri,
            file_path,
            parser,
            tree,
            editor,
            lsp_snapshot,
        })
    }

    /// Get targets for generation  
    pub fn find_targets(&self) -> Result<Vec<(u64, Target, usize, usize)>> {
        let source = self.editor.get_text();
        let target_map = TargetMap::build(&self.tree, &source)?;

        let mut targets = Vec::new();
        for (checksum, (target, node)) in target_map.iter() {
            targets.push((
                *checksum,
                target.clone(),
                node.start_byte(),
                node.end_byte(),
            ));
        }
        Ok(targets)
    }

    /// Apply an edit event
    pub fn apply_edit(&mut self, edit: EditEvent) -> Result<()> {
        debug!("ApplyEdit: checksum={:x}", edit.checksum);

        // Find the function in the current tree
        let source = self.editor.get_text();

        // Get body positions and apply edit
        {
            let target_map = TargetMap::build(&self.tree, &source)?;

            if let Some((_target, node)) = target_map.get(edit.checksum) {
                // Get function body node
                let body_node = node.child_by_field_name("body");
                let body_start = body_node
                    .as_ref()
                    .map(|n| n.start_byte())
                    .unwrap_or(node.end_byte());
                let body_end = body_node
                    .as_ref()
                    .map(|n| n.end_byte())
                    .unwrap_or(node.end_byte());

                let body = format!("{{\n{}\n}}", edit.new_body.trim());

                // Create text edit
                let start_pos = self.editor.byte_to_lsp_position(body_start);
                let end_pos = self.editor.byte_to_lsp_position(body_end);
                let text_edit = lsp_types::TextEdit::new(Range::new(start_pos, end_pos), body);

                // Apply edit using CRDT
                self.editor.apply_text_edit(text_edit, edit.snapshot);
            } else {
                error!("Function with checksum {:x} not found", edit.checksum);
                return Ok(());
            }
        }

        // Re-parse the document after target_map is dropped
        let new_source = self.editor.get_text();
        self.tree = self.parser.parse(&new_source)?;

        Ok(())
    }

    /// Get text content
    pub fn get_text(&self) -> String {
        self.editor.get_text()
    }

    /// Get the file URI
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Get the file path
    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}
