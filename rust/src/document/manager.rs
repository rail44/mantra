use anyhow::Result;
use std::path::Path;
use tree_sitter::{InputEdit, Point, Tree};

use crate::config::Config;
use crate::editor::{indent_code, IncrementalEditor};
use crate::generation::{EditEvent, TargetGenerator};
use crate::parser::{target_map::TargetMap, GoParser};

/// Manages document state including Tree-sitter tree and coordinates generation
pub struct DocumentManager {
    /// Configuration
    _config: Config,
    /// Target generator for LLM operations
    target_generator: TargetGenerator,
    /// Parser instance
    parser: GoParser,
    /// Current parse tree
    tree: Tree,
    /// Text editor
    editor: IncrementalEditor,
    /// File path
    file_path: std::path::PathBuf,
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
    /// Create a new document manager for a file
    pub fn new(config: Config, file_path: &Path) -> Result<Self> {
        let target_generator = TargetGenerator::new(config.clone())?;
        let source = std::fs::read_to_string(file_path)?;
        let mut parser = GoParser::new()?;
        let tree = parser.parse(&source)?;
        let editor = IncrementalEditor::new(source);

        Ok(Self {
            _config: config,
            target_generator,
            parser,
            tree,
            editor,
            file_path: file_path.to_path_buf(),
        })
    }

    /// Generate code for all targets in the document
    pub async fn generate_all(&mut self) -> Result<String> {
        // Collect targets and their information before processing
        let targets_to_process: Vec<_> = {
            let target_map = TargetMap::build(&self.tree, self.editor.source())?;

            tracing::info!("Found {} targets in file", target_map.len());
            for target in target_map.targets() {
                tracing::info!("  - {} ({})", target.name, target.instruction);
            }

            // Collect all necessary information while target_map is valid
            target_map
                .checksums()
                .map(|checksum| {
                    let (target, node) = target_map.get(checksum).unwrap();
                    (
                        checksum,
                        target.clone(),
                        target_map.package_name().to_string(),
                        node.start_byte(),
                        node.end_byte(),
                    )
                })
                .collect()
        };

        // Process each target incrementally
        for (checksum, target, package_name, _start_byte, _end_byte) in targets_to_process {
            // Re-build target map for current state to get updated node
            let target_map = TargetMap::build(&self.tree, self.editor.source())?;

            // Find the node for this function by name and extract all needed info
            let node_data = target_map
                .targets()
                .zip(target_map.nodes())
                .find(|(t, _)| t.name == target.name)
                .map(|(_, node)| {
                    let func_start = node.start_byte();
                    let body_info = node
                        .child_by_field_name("body")
                        .map(|b| (b.start_byte(), b.end_byte()));
                    (node.clone(), func_start, body_info)
                });

            // Drop target_map before processing
            drop(target_map);

            if let Some((node, func_start, body_info)) = node_data {
                // Generate code for this target
                let generated_code = self
                    .target_generator
                    .generate(
                        &target,
                        &package_name,
                        &self.file_path,
                        self.editor.source(),
                        &node,
                    )
                    .await?;

                // Create EditEvent
                let event = EditEvent::new(checksum, target.signature.clone(), generated_code);

                // Apply the edit event
                self.apply_edit_event(event, func_start, body_info)?;

                tracing::debug!(
                    "Applied edit for {} (checksum: {:x})",
                    target.name,
                    checksum
                );
            }
        }

        // Return the final edited source
        Ok(self.editor.source().to_string())
    }

    /// Apply an EditEvent to the document
    fn apply_edit_event(
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
            }
        }

        Ok(())
    }
}
