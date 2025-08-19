use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use actix::prelude::*;
use anyhow::Result;
use std::path::PathBuf;

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

/// Apply edit to document
#[derive(Message, Debug)]
#[rtype(result = "Result<()>")]
pub struct ApplyEdit {
    pub edit: crate::generation::EditEvent,
}

/// Get file URI
#[derive(Message, Debug)]
#[rtype(result = "Result<String>")]
pub struct GetFileUri;

/// Get source text
#[derive(Message, Debug)]
#[rtype(result = "Result<String>")]
pub struct GetSource;

/// Shutdown document
#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct DocumentShutdown;

/// Send didChange notification to LSP
#[derive(Message, Debug)]
#[rtype(result = "Result<()>")]
pub struct SendDidChange;

/// Format document using LSP
#[derive(Message, Debug)]
#[rtype(result = "Result<()>")]
pub struct FormatDocument;
