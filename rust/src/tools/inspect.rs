use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::lsp::{Position, Range, TextDocumentIdentifier};
use crate::workspace::Workspace;

/// Tool for inspecting symbols in code
#[derive(Default)]
pub struct InspectTool {
    /// Map of scope IDs to their locations
    scopes: HashMap<String, ScopeInfo>,
    /// Counter for generating unique scope IDs
    next_scope_id: usize,
}

/// Information about a scope
#[derive(Debug, Clone)]
struct ScopeInfo {
    /// File URI
    uri: String,
    /// Range in the file
    range: Range,
}

/// Request to inspect a symbol
#[derive(Debug, Deserialize)]
pub struct InspectRequest {
    /// Scope ID to inspect within
    pub scope_id: String,
    /// Symbol name to inspect
    pub symbol: String,
}

/// Response from inspection
#[derive(Debug, Serialize)]
pub struct InspectResponse {
    /// New scope ID for the definition
    pub scope_id: String,
    /// Code snippet of the definition
    pub code: String,
}

impl InspectTool {
    /// Create a new InspectTool
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a scope and return its ID
    pub fn register_scope(&mut self, uri: String, range: Range) -> String {
        let scope_id = format!("scope_{}", self.next_scope_id);
        self.next_scope_id += 1;

        self.scopes
            .insert(scope_id.clone(), ScopeInfo { uri, range });

        scope_id
    }

    /// Inspect a symbol within a scope
    pub async fn inspect(
        &mut self,
        request: InspectRequest,
        workspace: &mut Workspace,
    ) -> Result<InspectResponse> {
        // Get scope information
        let scope_info = self
            .scopes
            .get(&request.scope_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown scope ID: {}", request.scope_id))?
            .clone();

        // Get document actor
        let document = workspace.get_document(&scope_info.uri).await?;

        // Find symbol position within the scope
        let (tx, rx) = tokio::sync::oneshot::channel();
        document
            .send(crate::document::DocumentCommand::FindSymbol {
                range: scope_info.range.clone(),
                symbol: request.symbol.clone(),
                response: tx,
            })
            .await?;

        let symbol_position = rx.await??;

        // Use LSP to find definition
        let lsp_client = workspace.lsp_client();
        let definition = lsp_client
            .definition(
                TextDocumentIdentifier {
                    uri: scope_info.uri.clone(),
                },
                symbol_position,
            )
            .await?;

        let definition_location = definition
            .ok_or_else(|| anyhow::anyhow!("No definition found for symbol: {}", request.symbol))?;

        // Get the definition's complete block
        let definition_document = workspace.get_document(&definition_location.uri).await?;

        let (tx, rx) = tokio::sync::oneshot::channel();
        definition_document
            .send(crate::document::DocumentCommand::GetDefinitionBlock {
                position: definition_location.range.start,
                response: tx,
            })
            .await?;

        let (expanded_range, code) = rx.await??;

        // Register new scope for the definition
        let new_scope_id = self.register_scope(definition_location.uri, expanded_range);

        Ok(InspectResponse {
            scope_id: new_scope_id,
            code,
        })
    }

    /// Create initial scope for a target function
    pub fn create_initial_scope(&mut self, uri: String, start_line: u32, end_line: u32) -> String {
        let range = Range {
            start: Position {
                line: start_line,
                character: 0,
            },
            end: Position {
                line: end_line,
                character: 0,
            },
        };
        self.register_scope(uri, range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_registration() {
        let mut tool = InspectTool::new();

        let scope1 = tool.register_scope(
            "file:///test.go".to_string(),
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        );

        assert_eq!(scope1, "scope_0");

        let scope2 = tool.register_scope(
            "file:///test2.go".to_string(),
            Range {
                start: Position {
                    line: 5,
                    character: 0,
                },
                end: Position {
                    line: 15,
                    character: 0,
                },
            },
        );

        assert_eq!(scope2, "scope_1");
        assert_eq!(tool.scopes.len(), 2);
    }
}
