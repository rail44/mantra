use actix::prelude::*;
use anyhow::{Context as AnyhowContext, Result};
use std::path::PathBuf;
use tracing::{debug, error};

use super::actor::Workspace;
use super::messages::{
    ApplyEdit, DocumentShutdown, GenerateAll, GetFileUri, GetSource, GetTargetInfo, SendDidChange,
};
use crate::config::Config;
use crate::editor::crdt::{CrdtEditor, Snapshot};
use crate::generation::EditEvent;
use crate::lsp::Client as LspClient;
use crate::parser::{target_map::TargetMap, GoParser};
use tree_sitter::Tree;

/// Document actor managing a single document with CRDT support
pub struct DocumentActor {
    uri: String,
    workspace: Addr<Workspace>,
    lsp_client: LspClient,
    parser: GoParser,
    tree: Tree,
    editor: CrdtEditor,
    /// Snapshot of the version that LSP knows about
    lsp_snapshot: Snapshot,
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

        // Initialize CRDT editor
        let editor = CrdtEditor::from_text(&content);
        let lsp_snapshot = editor.create_snapshot();

        Ok(Self {
            uri,
            workspace,
            lsp_client,
            parser,
            tree,
            editor,
            lsp_snapshot,
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
        let target_map = match TargetMap::build(&self.tree, &source) {
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

        // Capture snapshot and position info at the beginning of generation
        let generation_snapshot = self.editor.create_snapshot();
        
        // Get position information for all targets before generation
        let source = self.editor.get_text().to_string();
        let target_positions: std::collections::HashMap<u64, (usize, usize)> = {
            use crate::parser::target_map::TargetMap;
            if let Ok(target_map) = TargetMap::build(&self.tree, &source) {
                targets.iter().filter_map(|(checksum, _)| {
                    target_map.get(*checksum).map(|(_, node)| {
                        (*checksum, (node.start_byte(), node.end_byte()))
                    })
                }).collect()
            } else {
                std::collections::HashMap::new()
            }
        };

        // Generate code for all targets using TargetGenerator
        Box::pin(
            async move {
                let mut edits = Vec::new();

                for (checksum, signature) in targets {
                    debug!("Starting generation for checksum {:x}", checksum);

                    // Get position info for this target
                    let (start_byte, end_byte) = target_positions.get(&checksum)
                        .copied()
                        .unwrap_or((0, 0));

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
                                generation_snapshot.clone(),
                                start_byte,
                                end_byte,
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
    type Result = Result<()>;

    fn handle(&mut self, msg: ApplyEdit, _ctx: &mut Context<Self>) -> Self::Result {
        // Apply the edit using snapshot-based positioning
        self.apply_edit_internal(msg.edit)?;
        
        // Update LSP snapshot after the edit
        self.lsp_snapshot = self.editor.create_snapshot();
        
        Ok(())
    }
}

impl Handler<GetTargetInfo> for DocumentActor {
    type Result = Result<(crate::parser::target::Target, String, u32, u32)>;

    fn handle(&mut self, msg: GetTargetInfo, _ctx: &mut Context<Self>) -> Self::Result {
        let source = self.editor.get_text();
        let target_map = TargetMap::build(&self.tree, &source)?;

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

impl Handler<SendDidChange> for DocumentActor {
    type Result = ResponseActFuture<Self, Result<()>>;

    fn handle(&mut self, _msg: SendDidChange, _ctx: &mut Context<Self>) -> Self::Result {
        // Note: This sends a full document update to LSP
        // The editor's version will be incremented by the next edit operation
        let current_version = self.editor.get_version();

        let uri_result = self.uri.parse();
        let uri = match uri_result {
            Ok(uri) => uri,
            Err(e) => {
                error!("Failed to parse URI: {}", e);
                return Box::pin(
                    fut::ready(Err(anyhow::anyhow!("Failed to parse URI: {}", e))).into_actor(self),
                );
            }
        };

        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier {
                uri,
                version: current_version,
            },
            content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                range: None, // Full document update - incremental changes require tracking edit ranges
                range_length: None,
                text: self.editor.get_text().to_string(),
            }],
        };

        let lsp_client = self.lsp_client.clone();
        let uri_str = self.uri.clone();
        let version = current_version;

        Box::pin(
            async move {
                if let Err(e) = lsp_client.did_change(params).await {
                    error!("Failed to send didChange notification: {}", e);
                    return Err(e);
                }

                debug!(
                    "Sent didChange notification for version {} to {}",
                    version, uri_str
                );

                Ok(())
            }
            .into_actor(self),
        )
    }
}

impl DocumentActor {

    /// Apply an edit to the document (internal method)
    fn apply_edit_internal(&mut self, edit: EditEvent) -> Result<()> {
        let checksum = edit.checksum;
        let content = edit.new_body.clone();
        let snapshot = edit.snapshot;
        let func_start_byte = edit.start_byte;
        let func_end_byte = edit.end_byte;

        // Create replacement content: function with checksum comment + signature + new body
        let source = snapshot.rope.to_string();
        let func_text = &source[func_start_byte..func_end_byte];
        
        // Extract signature (everything before the body)
        let func_signature = if let Some(brace_pos) = func_text.find('{') {
            &func_text[..brace_pos]
        } else {
            func_text
        };

        let replacement = format!(
            "// mantra:checksum:{:x}\n{} {{\n{}}}",
            checksum,
            func_signature.trim_end(),
            content
        );

        // Create TextEdit for the replacement using snapshot-based positions
        let text_edit = lsp_types::TextEdit {
            range: lsp_types::Range {
                start: crate::editor::crdt::CrdtEditor::byte_to_lsp_position_with_rope(func_start_byte, &snapshot.rope),
                end: crate::editor::crdt::CrdtEditor::byte_to_lsp_position_with_rope(func_end_byte, &snapshot.rope),
            },
            new_text: replacement.clone(),
        };

        // Apply the edit using snapshot-based CRDT
        self.editor.apply_text_edit(&text_edit, snapshot)?;

        // Reparse the tree
        let new_source = self.editor.get_text();
        self.tree = self
            .parser
            .parse_incremental(&new_source, Some(&self.tree))
            .with_context(|| "Failed to reparse after edit")?;

        debug!("Applied edit for checksum {:x}", checksum);

        Ok(())
    }

}
