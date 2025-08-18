use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::tools::inspect::{InspectRequest, InspectResponse};
use actix::prelude::*;
use anyhow::Result;
use lsp_types::Range;
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

/// Register a scope in InspectTool
#[derive(Message, Debug)]
#[rtype(result = "String")]
pub struct RegisterScope {
    pub uri: String,
    pub range: Range,
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

/// Apply edit to document
#[derive(Message, Debug)]
#[rtype(result = "Result<()>")]
pub struct ApplyEdit {
    pub edit: crate::generation::EditEvent,
}

/// Get target info by checksum
#[derive(Message, Debug)]
#[rtype(result = "Result<(crate::parser::target::Target, String, u32, u32)>")]
pub struct GetTargetInfo {
    pub checksum: u64,
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

/// Format generated code using LSP
#[derive(Message, Debug)]
#[rtype(result = "Result<String>")]
pub struct FormatGeneratedCode {
    pub code: String,
}
