use anyhow::{Context as AnyhowContext, Result};
use lsp_types::{
    DidChangeTextDocumentParams, Range, TextDocumentContentChangeEvent,
    VersionedTextDocumentIdentifier,
};
use std::path::PathBuf;
use tracing::{debug, error, info};
use tree_sitter::Tree;

use crate::editor::crdt::CrdtEditor;
use crate::generation::EditEvent;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::{target::Target, target_map::TargetMap, GoParser};
use crate::workspace::service::{Client as ServiceClient, Handler, Service};

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
            .parse(content)
            .with_context(|| "Failed to parse Go source")?;

        // Initialize CRDT editor
        let editor = CrdtEditor::new(content);
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

/// Service wrapper for Document with external dependencies
pub struct DocumentService {
    document: Document,
    lsp_client: LspClient,
    llm_client: LLMClient,
    self_client: Option<ServiceClient<DocumentService>>,
}

impl Service for DocumentService {}

/// Message to start generation for all targets
pub struct StartGeneration;

/// Message to apply a generation result
pub struct ApplyGeneration {
    pub result: EditEvent,
}

/// Message to get the current content
pub struct GetContent;

impl Handler<StartGeneration> for DocumentService {
    type Response = ();

    async fn handle(&mut self, _: StartGeneration) -> Self::Response {
        info!("StartGeneration: {}", self.document.uri);

        let targets = match self.document.find_targets() {
            Ok(targets) => targets,
            Err(e) => {
                error!("Failed to find targets: {}", e);
                return;
            }
        };

        if targets.is_empty() {
            debug!("No targets found");
            return;
        }

        let self_client = match &self.self_client {
            Some(client) => client.clone(),
            None => {
                error!("Self client not initialized");
                return;
            }
        };

        // Spawn generation tasks
        for (checksum, target, start_byte, end_byte) in targets {
            let self_client = self_client.clone();
            let llm_client = self.llm_client.clone();
            let snapshot = self.document.editor.fork();

            tokio::spawn(async move {
                match Self::generate_for_target(&llm_client, &target).await {
                    Ok(new_body) => {
                        let event = EditEvent::new(
                            checksum,
                            target.signature.clone(),
                            new_body,
                            snapshot,
                            start_byte,
                            end_byte,
                        );
                        let _ = self_client.request(ApplyGeneration { result: event }).await;
                    }
                    Err(e) => {
                        error!("Failed to generate for {}: {}", target.name, e);
                    }
                }
            });
        }
    }
}

impl Handler<ApplyGeneration> for DocumentService {
    type Response = ();

    async fn handle(&mut self, msg: ApplyGeneration) -> Self::Response {
        debug!("ApplyGeneration: checksum={:x}", msg.result.checksum);

        if let Err(e) = self.document.apply_edit(msg.result) {
            error!("Failed to apply edit: {}", e);
            return;
        }

        // Send changes to LSP
        if let Err(e) = self.send_did_change().await {
            error!("Failed to send didChange: {}", e);
        }

        // Format document
        if let Err(e) = self.format_document().await {
            error!("Failed to format document: {}", e);
        }
    }
}

impl Handler<GetContent> for DocumentService {
    type Response = String;

    async fn handle(&mut self, _: GetContent) -> Self::Response {
        self.document.get_text()
    }
}

impl DocumentService {
    /// Create a new DocumentService
    pub async fn new(
        file_path: PathBuf,
        uri: String,
        lsp_client: LspClient,
        llm_client: LLMClient,
    ) -> Result<Self> {
        // Read the file content
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path.display(), e))?;

        // Create document
        let document = Document::new(file_path, uri, &content)?;

        Ok(Self {
            document,
            lsp_client,
            llm_client,
            self_client: None,
        })
    }

    /// Spawn the service with self-reference support
    pub fn spawn(self) -> ServiceClient<DocumentService> {
        Service::spawn_with_self(self, |service, client| {
            service.self_client = Some(client);
        })
    }

    /// Generate code for a specific target
    async fn generate_for_target(llm_client: &LLMClient, target: &Target) -> Result<String> {
        debug!("Generating for target: {}", target.name);

        // Build prompt
        let prompt = crate::generation::build_prompt(target);

        // Generate using LLM
        let request = crate::llm::CompletionRequest {
            model: llm_client.model().to_string(),
            provider: llm_client
                .openrouter_config()
                .map(|config| crate::llm::ProviderSpec {
                    only: Some(config.providers.clone()),
                }),
            messages: vec![crate::llm::Message::user(prompt)],
            max_tokens: Some(2000),
            temperature: 0.7,
        };

        let response = llm_client.complete(request).await?;

        if let Some(choice) = response.choices.first() {
            Ok(crate::generation::clean_generated_code(
                choice.message.content.clone(),
            ))
        } else {
            Err(anyhow::anyhow!("No response from LLM"))
        }
    }

    /// Send didChange notification to LSP
    async fn send_did_change(&mut self) -> Result<()> {
        let current_version = self.document.editor.get_version();
        let uri: lsp_types::Uri = self.document.uri.parse()?;

        // Get the current document text
        let content = self.document.get_text();

        // Send full document update
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri,
                version: current_version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content,
            }],
        };

        self.lsp_client.did_change(params).await?;

        // Update LSP snapshot
        self.document.lsp_snapshot = self.document.editor.fork();

        // Wait for diagnostics
        if let Ok(diagnostics) = self
            .lsp_client
            .wait_for_diagnostics_timeout(&self.document.uri, std::time::Duration::from_secs(2))
            .await
        {
            if !diagnostics.diagnostics.is_empty() {
                info!(
                    "Diagnostics for {}: {:?}",
                    self.document.uri, diagnostics.diagnostics
                );
            }
        }

        Ok(())
    }

    /// Format document using LSP
    async fn format_document(&mut self) -> Result<()> {
        if !self.lsp_client.supports_document_formatting().await {
            debug!("Document formatting not supported");
            return Ok(());
        }

        let uri: lsp_types::Uri = self.document.uri.parse()?;
        let formatting_options = lsp_types::FormattingOptions {
            tab_size: 4,
            insert_spaces: false,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            properties: Default::default(),
        };

        if let Some(edits) = self
            .lsp_client
            .format_document(
                lsp_types::TextDocumentIdentifier { uri },
                formatting_options,
            )
            .await?
        {
            if !edits.is_empty() {
                debug!("Applying {} formatting edits", edits.len());
                let snapshot = self.document.editor.fork();
                self.document.editor.apply_text_edits(&edits, snapshot);

                // Re-parse after formatting
                let new_source = self.document.get_text();
                self.document.tree = self.document.parser.parse(&new_source)?;

                // Send updated content to LSP
                self.send_did_change().await?;
            }
        }

        Ok(())
    }
}
