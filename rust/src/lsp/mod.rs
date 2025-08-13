/// LSP-style edit representation
use serde::{Deserialize, Serialize};

/// Position in a text document (0-indexed)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// Range in a text document
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Text edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// Document change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidChangeTextDocument {
    pub uri: String,
    pub version: u32,
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

/// Content change event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextDocumentContentChangeEvent {
    /// Full document sync
    Full {
        text: String,
    },
    /// Incremental sync
    Incremental {
        range: Range,
        text: String,
    },
}

/// Edit command that can be applied to a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditCommand {
    pub document_uri: String,
    pub version: u32,
    pub edits: Vec<TextEdit>,
}

impl EditCommand {
    /// Create an edit command for replacing a function body
    pub fn replace_function_body(
        uri: String,
        version: u32,
        start: Position,
        end: Position,
        new_body: String,
    ) -> Self {
        Self {
            document_uri: uri,
            version,
            edits: vec![TextEdit {
                range: Range { start, end },
                new_text: new_body,
            }],
        }
    }
}