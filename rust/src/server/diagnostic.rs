use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use serde::{Deserialize, Serialize};

/// Metadata attached to mantra diagnostics for code action processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MantraDiagnosticData {
    pub instruction: String,
    /// Checksum in hex format
    pub checksum: String,
    /// End position of the target (function end)
    pub target_end: Position,
    /// Start position for editing (includes any preceding checksum comments)
    pub edit_start: Position,
}

impl MantraDiagnosticData {
    pub fn new(
        instruction: String,
        checksum: u64,
        target_end: Position,
        edit_start: Position,
    ) -> Self {
        Self {
            instruction,
            checksum: format!("{checksum:x}"),
            target_end,
            edit_start,
        }
    }

    /// Parse checksum from hex string
    pub fn parse_checksum(&self) -> Option<u64> {
        u64::from_str_radix(&self.checksum, 16).ok()
    }

    /// Try to extract MantraDiagnosticData from a diagnostic's data field
    pub fn from_diagnostic(diagnostic: &Diagnostic) -> Option<Self> {
        diagnostic
            .data
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Create a diagnostic for a mantra target
pub fn create_diagnostic(
    instruction: &str,
    checksum: u64,
    func_start: Position,
    func_end: Position,
    edit_start: Position,
) -> Diagnostic {
    let data = MantraDiagnosticData::new(instruction.to_string(), checksum, func_end, edit_start);

    Diagnostic {
        range: Range {
            start: Position {
                line: func_start.line,
                character: 0,
            },
            end: Position {
                line: func_start.line,
                character: func_start.character,
            },
        },
        severity: Some(DiagnosticSeverity::HINT),
        source: Some("mantra".to_string()),
        message: format!("Generate implementation: {instruction}"),
        data: Some(serde_json::to_value(&data).expect("serialization should not fail")),
        ..Default::default()
    }
}
