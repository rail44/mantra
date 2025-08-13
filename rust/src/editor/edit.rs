use serde::{Deserialize, Serialize};

/// Position in a text document (0-indexed)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Range in a text document
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// Text edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    #[serde(rename = "newText")]
    pub new_text: String,
}

impl TextEdit {
    pub fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }

    /// Create a replacement edit
    pub fn replace(start: Position, end: Position, new_text: String) -> Self {
        Self::new(Range::new(start, end), new_text)
    }
}

/// Versioned text document identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: Option<u32>,
}

/// Text document edit (multiple edits to a single document)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentEdit {
    #[serde(rename = "textDocument")]
    pub text_document: VersionedTextDocumentIdentifier,
    pub edits: Vec<TextEdit>,
}

impl TextDocumentEdit {
    pub fn new(uri: String, version: Option<u32>, edits: Vec<TextEdit>) -> Self {
        Self {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            edits,
        }
    }
}

/// Document changes (can be multiple documents)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentChanges {
    Edits(Vec<TextDocumentEdit>),
}

/// Workspace edit (LSP-style)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    pub label: Option<String>,
    #[serde(rename = "documentChanges")]
    pub document_changes: Option<DocumentChanges>,
}

impl WorkspaceEdit {
    /// Create a new workspace edit
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            document_changes: Some(DocumentChanges::Edits(Vec::new())),
        }
    }

    /// Add a document edit
    pub fn add_document_edit(&mut self, edit: TextDocumentEdit) {
        if let Some(DocumentChanges::Edits(ref mut edits)) = self.document_changes {
            edits.push(edit);
        }
    }

    /// Create a workspace edit for replacing a function body
    pub fn replace_function_body(
        uri: String,
        version: Option<u32>,
        start: Position,
        end: Position,
        new_body: String,
        checksum: u64,
    ) -> Self {
        let mut edit = Self::new(format!("Replace function body (checksum: {:x})", checksum));

        let text_edit = TextEdit::replace(start, end, new_body);
        let doc_edit = TextDocumentEdit::new(uri, version, vec![text_edit]);

        edit.add_document_edit(doc_edit);
        edit
    }
}
