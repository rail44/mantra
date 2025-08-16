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
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    file_path: PathBuf,
    uri: String,
    workspace: Addr<Workspace>,
    parser: GoParser,
    tree: Tree,
    editor: IncrementalEditor,
    #[allow(dead_code)]
    document_version: i32,
}

impl DocumentActor {
    pub async fn new(
        config: Config,
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
            config,
            file_path,
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

                    // Create a bridge for TargetGenerator to communicate with DocumentActor
                    let target_gen = TargetGeneratorBridge::new(checksum, document_addr.clone());

                    match target_gen.generate(workspace.clone()).await {
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
// TargetGenerator Bridge
// ============================================================================

/// Bridge between TargetGenerator and DocumentActor
struct TargetGeneratorBridge {
    checksum: u64,
    document_addr: Addr<DocumentActor>,
}

impl TargetGeneratorBridge {
    fn new(checksum: u64, document_addr: Addr<DocumentActor>) -> Self {
        Self {
            checksum,
            document_addr,
        }
    }

    async fn generate(&self, workspace: Addr<super::actor::Workspace>) -> Result<String> {
        // Get target info from DocumentActor
        let (target, _package_name, _start_line, _end_line) = self
            .document_addr
            .send(GetTargetInfo {
                checksum: self.checksum,
            })
            .await??;

        debug!("Got target info for {}", target.name);

        // Build prompt (simplified for now - InspectTool integration will come later)
        let prompt = build_prompt(&target);

        // Get LLM client and generate code
        let llm_client = workspace.send(super::messages::GetLlmClient).await?;
        let request = crate::llm::CompletionRequest {
            model: llm_client.model().to_string(),
            provider: None,
            messages: vec![crate::llm::Message::user(prompt)],
            max_tokens: Some(2000),
            temperature: 0.7,
        };

        let response = llm_client.complete(request).await?;

        if let Some(choice) = response.choices.first() {
            Ok(clean_generated_code(choice.message.content.clone()))
        } else {
            Err(anyhow::anyhow!("No response from LLM"))
        }
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

        // Find the target by checksum
        let source = self.editor.source();

        // Get the node and body range, then drop target_map
        let (body_start, body_end) = {
            let target_map = TargetMap::build(&self.tree, source)?;

            if let Some((_target, node)) = target_map.get(checksum) {
                // Get the function body node
                let body_node = node
                    .child_by_field_name("body")
                    .with_context(|| "Function has no body")?;

                // Find the opening and closing braces
                let mut cursor = body_node.walk();
                let mut body_start = body_node.start_byte() + 1; // Skip opening brace
                let mut body_end = body_node.end_byte() - 1; // Skip closing brace

                // Find exact positions of braces
                for child in body_node.children(&mut cursor) {
                    if child.kind() == "{" {
                        body_start = child.end_byte();
                    } else if child.kind() == "}" {
                        body_end = child.start_byte();
                    }
                }

                // If we couldn't find braces, use simplified approach
                if body_start >= body_end {
                    let body_text = body_node.utf8_text(source.as_bytes())?;
                    if let Some(open_pos) = body_text.find('{') {
                        body_start = body_node.start_byte() + open_pos + 1;
                    }
                    if let Some(close_pos) = body_text.rfind('}') {
                        body_end = body_node.start_byte() + close_pos;
                    }
                }

                (body_start, body_end)
            } else {
                error!("Target not found for checksum {:x}", checksum);
                return Ok(());
            }
        }; // Drop target_map here

        // Create input edit for tree-sitter
        let input_edit = self.create_input_edit_for_replace(body_start, body_end, &content);

        // Apply edit to the tree
        self.tree.edit(&input_edit);

        // Apply edit to the editor
        self.editor.replace(body_start, body_end, content);

        // Reparse the tree
        let new_source = self.editor.source();
        self.tree = self
            .parser
            .parse_incremental(new_source, Some(&self.tree))
            .with_context(|| "Failed to reparse after edit")?;

        // Increment document version
        self.document_version += 1;

        debug!("Applied edit for checksum {:x}", checksum);

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

// ============================================================================
// Helper Functions
// ============================================================================

/// Build a prompt for code generation
fn build_prompt(target: &crate::parser::target::Target) -> String {
    format!(
        "Generate the Go implementation for this function:\n\n\
         Function signature: {}\n\
         Instruction: {}\n\n\
         Return only the code that goes inside the function body (without the curly braces).\n\
         For example, if the function should add two numbers, just return: return a + b",
        target.signature, target.instruction
    )
}

/// Clean generated code from LLM response
fn clean_generated_code(code: String) -> String {
    let mut cleaned = code.trim().to_string();

    // Remove markdown code blocks if present
    if cleaned.starts_with("```") {
        if let Some(start) = cleaned.find('\n') {
            cleaned = cleaned[start + 1..].to_string();
        }
    }
    if cleaned.ends_with("```") {
        if let Some(end) = cleaned.rfind("\n```") {
            cleaned = cleaned[..end].to_string();
        } else {
            cleaned = cleaned[..cleaned.len() - 3].to_string();
        }
    }

    // Ensure proper indentation
    let lines: Vec<&str> = cleaned.lines().collect();
    let mut result = String::new();
    for line in lines {
        if !line.is_empty() {
            result.push('\t');
            result.push_str(line);
        }
        result.push('\n');
    }

    result
}
