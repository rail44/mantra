use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use actix::prelude::*;
use anyhow::Result;
use std::path::PathBuf;

// ============================================================================
// Message Definitions
// ============================================================================

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

// Document messages are no longer needed since Document is not an Actor
