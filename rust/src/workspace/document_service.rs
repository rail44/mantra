use anyhow::Result;
use std::path::PathBuf;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::editor::crdt::CrdtEditor;
use crate::generation::EditEvent;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::parser::target::Target;
use crate::workspace::document::Document;
use crate::workspace::service::{Client as ServiceClient, Handler, Service};
use lsp_types::{
    DidChangeTextDocumentParams, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};

/// Service wrapper for Document with external dependencies
pub struct DocumentService {
    document: Document,
    config: Config,
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
            let editor_text = self.document.get_text();

            tokio::spawn(async move {
                match Self::generate_for_target(&llm_client, &target).await {
                    Ok(new_body) => {
                        // Create snapshot in the spawned task
                        let snapshot = CrdtEditor::new(&editor_text);
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
        config: Config,
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
            config,
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
