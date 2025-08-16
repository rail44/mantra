use actix::prelude::*;
use anyhow::Result;
use std::path::PathBuf;

use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::tools::inspect::{InspectRequest, InspectResponse};

use super::document::DocumentActor;

// ============================================================================
// Message Definitions
// ============================================================================

/// Get or create a Document actor
#[derive(Message, Debug)]
#[rtype(result = "Result<Addr<DocumentActor>>")]
pub struct GetDocument {
    pub uri: String,
}

/// Get LSP client (clone)
#[derive(Message, Debug)]
#[rtype(result = "LspClient")]
pub struct GetLspClient;

/// Get LLM client (clone)
#[derive(Message, Debug)]
#[rtype(result = "LLMClient")]
pub struct GetLlmClient;

/// Register a scope in InspectTool
#[derive(Message, Debug)]
#[rtype(result = "String")]
pub struct RegisterScope {
    pub uri: String,
    pub range: crate::lsp::Range,
}

/// Inspect a symbol
#[derive(Message, Debug)]
#[rtype(result = "Result<InspectResponse>")]
pub struct InspectSymbol {
    pub request: InspectRequest,
}

/// Generate code for a file
#[derive(Message, Debug)]
#[rtype(result = "Result<String>")]
pub struct GenerateFile {
    pub file_path: PathBuf,
}

/// Shutdown the workspace
#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Shutdown;

// Document messages

/// Generate all targets
#[derive(Message, Debug)]
#[rtype(result = "Result<String>")]
pub struct GenerateAll;

/// Shutdown document
#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct DocumentShutdown;
