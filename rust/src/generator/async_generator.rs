use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use std::collections::HashMap;

use crate::config::Config;
use crate::llm::{LLMClient, Message, CompletionRequest};
use crate::parser::{GoParser, target::Target, checksum::calculate_checksum};
use crate::lsp::{EditCommand, Position, Range};

/// Document state tracking
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub uri: String,
    pub version: u32,
    pub content: String,
    pub tree: Option<tree_sitter::Tree>,
}

/// Event from the editor
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// Document was opened
    DidOpen { uri: String, content: String },
    
    /// Document was changed
    DidChange { 
        uri: String, 
        version: u32,
        content: String 
    },
    
    /// User requested generation for a specific function
    GenerateRequest { 
        uri: String,
        function_signature: String,
    },
    
    /// User accepted a generation
    AcceptGeneration {
        uri: String,
        generation_id: String,
    },
}

/// Generation result
#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub id: String,
    pub uri: String,
    pub target: Target,
    pub generated_code: String,
    pub checksum: u64,
    pub edit_command: EditCommand,
}

/// Async generator that responds to editor events
pub struct AsyncGenerator {
    config: Config,
    client: LLMClient,
    documents: Arc<RwLock<HashMap<String, DocumentState>>>,
    parser: Arc<RwLock<GoParser>>,
    event_rx: mpsc::Receiver<EditorEvent>,
    result_tx: mpsc::Sender<GenerationResult>,
}

impl AsyncGenerator {
    pub fn new(
        config: Config,
        event_rx: mpsc::Receiver<EditorEvent>,
        result_tx: mpsc::Sender<GenerationResult>,
    ) -> Result<Self> {
        let client = LLMClient::new(config.clone())?;
        let parser = GoParser::new()?;
        
        Ok(Self {
            config,
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            parser: Arc::new(RwLock::new(parser)),
            event_rx,
            result_tx,
        })
    }
    
    /// Main event loop
    pub async fn run(mut self) -> Result<()> {
        while let Some(event) = self.event_rx.recv().await {
            match event {
                EditorEvent::DidOpen { uri, content } => {
                    self.handle_did_open(uri, content).await?;
                }
                EditorEvent::DidChange { uri, version, content } => {
                    self.handle_did_change(uri, version, content).await?;
                }
                EditorEvent::GenerateRequest { uri, function_signature } => {
                    // Spawn generation task
                    let generator = self.clone_for_task();
                    tokio::spawn(async move {
                        if let Err(e) = generator.generate_for_function(uri, function_signature).await {
                            tracing::error!("Generation failed: {}", e);
                        }
                    });
                }
                EditorEvent::AcceptGeneration { .. } => {
                    // This would be handled by the editor/LSP server
                }
            }
        }
        Ok(())
    }
    
    /// Handle document open
    async fn handle_did_open(&self, uri: String, content: String) -> Result<()> {
        let mut parser = self.parser.write().await;
        let tree = parser.parse(&content)?;
        
        let mut docs = self.documents.write().await;
        docs.insert(uri.clone(), DocumentState {
            uri,
            version: 0,
            content,
            tree: Some(tree),
        });
        
        Ok(())
    }
    
    /// Handle document change
    async fn handle_did_change(&self, uri: String, version: u32, content: String) -> Result<()> {
        let mut parser = self.parser.write().await;
        let tree = parser.parse(&content)?;
        
        let mut docs = self.documents.write().await;
        docs.insert(uri.clone(), DocumentState {
            uri,
            version,
            content,
            tree: Some(tree),
        });
        
        Ok(())
    }
    
    /// Generate code for a specific function
    async fn generate_for_function(&self, uri: String, function_signature: String) -> Result<()> {
        // Get current document state
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found: {}", uri))?;
        
        let tree = doc.tree.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No parse tree for document"))?;
        
        // Find the target function
        let targets = self.extract_targets(&doc.content, tree)?;
        let target = targets.into_iter()
            .find(|t| t.signature == function_signature)
            .ok_or_else(|| anyhow::anyhow!("Function not found: {}", function_signature))?;
        
        // Calculate checksum
        let checksum = calculate_checksum(&target);
        
        // Generate code
        let generated_code = self.generate_implementation(&target).await?;
        
        // Find function position in document
        let (start_pos, end_pos) = self.find_function_body_range(&doc.content, tree, &function_signature)?;
        
        // Create edit command
        let edit_command = EditCommand::replace_function_body(
            uri.clone(),
            doc.version,
            start_pos,
            end_pos,
            format!("{{\n\t// mantra:checksum:{:x}\n{}}}", checksum, generated_code),
        );
        
        // Send result
        let result = GenerationResult {
            id: uuid::Uuid::new_v4().to_string(),
            uri,
            target,
            generated_code,
            checksum,
            edit_command,
        };
        
        self.result_tx.send(result).await?;
        Ok(())
    }
    
    /// Extract targets from parsed tree
    fn extract_targets(&self, source: &str, tree: &tree_sitter::Tree) -> Result<Vec<Target>> {
        // This would use the existing parser logic
        // Simplified for now
        Ok(Vec::new())
    }
    
    /// Find function body range in document
    fn find_function_body_range(
        &self,
        source: &str,
        tree: &tree_sitter::Tree,
        signature: &str,
    ) -> Result<(Position, Position)> {
        // Find the function node and get its body range
        // Convert byte positions to line/character positions
        // Simplified for now
        Ok((
            Position { line: 0, character: 0 },
            Position { line: 0, character: 0 },
        ))
    }
    
    /// Generate implementation for a target
    async fn generate_implementation(&self, target: &Target) -> Result<String> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message::system("You are a Go code generator. Generate only the function body."),
                Message::user(format!(
                    "Generate implementation for:\n{}\nInstruction: {}",
                    target.signature,
                    target.instruction
                )),
            ],
            temperature: 0.2,
            max_tokens: Some(1000),
        };
        
        let response = if self.config.url.contains("openrouter") {
            self.client.complete_openrouter(request).await?
        } else {
            self.client.complete(request).await?
        };
        
        Ok(response.choices.first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default())
    }
    
    /// Clone for spawning tasks
    fn clone_for_task(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: self.client.clone(),
            documents: Arc::clone(&self.documents),
            parser: Arc::clone(&self.parser),
            event_rx: /* would need a different approach */,
            result_tx: self.result_tx.clone(),
        }
    }
}