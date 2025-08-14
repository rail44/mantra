use anyhow::Result;
use futures::future::join_all;
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
        // Phase 1: Collect all targets and their information
        let generation_tasks = self.prepare_generation_tasks()?;

        if generation_tasks.is_empty() {
            return Ok(self.editor.source().to_string());
        }

        // Phase 2: Generate code for all targets in parallel
        let generated_events = self.generate_all_targets(generation_tasks).await?;

        // Phase 3: Apply all edits sequentially
        for event in generated_events {
            self.apply_generated_event(event)?;
        }

        // Return the final edited source
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

    /// Generate code for all targets in parallel
    async fn generate_all_targets(
        &self,
        tasks: Vec<GenerationTask>,
    ) -> Result<Vec<GeneratedEvent>> {
        // Build target map once for all tasks
        let target_map = TargetMap::build(&self.tree, self.editor.source())?;
        let package_name = target_map.package_name().to_string();

        // Extract all necessary data upfront
        let generation_data: Vec<_> = tasks
            .into_iter()
            .filter_map(|task| {
                target_map
                    .get(task.checksum)
                    .map(|(target, node)| (task.checksum, target.clone(), *node))
            })
            .collect();

        let file_path = self.file_path.clone();
        let source = self.editor.source().to_string();
        let target_generator = &self.target_generator;

        let generation_futures =
            generation_data
                .into_iter()
                .map(move |(checksum, target, node)| {
                    let file_path = file_path.clone();
                    let source = source.clone();
                    let package_name = package_name.clone();

                    async move {
                        // Generate code for this target
                        let generated_code = target_generator
                            .generate(&target, &package_name, &file_path, &source, &node)
                            .await?;

                        Ok::<_, anyhow::Error>(GeneratedEvent {
                            checksum,
                            target_name: target.name.clone(),
                            signature: target.signature.clone(),
                            generated_code,
                        })
                    }
                });

        // Execute all generation tasks in parallel
        let results = join_all(generation_futures).await;

        // Collect results, propagating any errors
        results.into_iter().collect::<Result<Vec<_>>>()
    }

    /// Apply a generated event to the document
    fn apply_generated_event(&mut self, event: GeneratedEvent) -> Result<()> {
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
            self.apply_edit_event(edit_event, func_start, body_info)?;

            tracing::debug!(
                "Applied edit for {} (checksum: {:x})",
                event.target_name,
                event.checksum
            );
        }

        Ok(())
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
