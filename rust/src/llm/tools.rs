use crate::inspector::SymbolInspector;
use crate::parser::target::PathSegment;
use crate::workspace::WorkspaceService;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Tool function definition for LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String, // "function"
    pub function: ToolFunction,
}

/// Tool call from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON string
}

/// Tool call result
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallResult {
    pub tool_call_id: String,
    pub role: String, // "tool"
    pub content: String,
}

/// Inspect tool with necessary context
pub struct InspectTool {
    workspace: WorkspaceService,
    document_uri: String,
    type_scope_mapping: HashMap<String, Vec<PathSegment>>,
}

impl InspectTool {
    pub fn new(
        workspace: WorkspaceService,
        document_uri: String,
        type_scope_mapping: HashMap<String, Vec<PathSegment>>,
    ) -> Self {
        Self {
            workspace,
            document_uri,
            type_scope_mapping,
        }
    }

    /// Execute the inspect tool call
    pub async fn execute(&self, tool_call: &ToolCall) -> anyhow::Result<ToolCallResult> {
        let args: Value = serde_json::from_str(&tool_call.function.arguments)?;
        let scope = args
            .get("scope")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'scope' parameter"))?;

        let symbol = args.get("symbol").and_then(|s| s.as_str());

        let inspector = SymbolInspector::new(&self.workspace);

        // Parse scope to handle hierarchical references like "SimpleCache.Name"
        let scope_parts: Vec<&str> = scope.split('.').collect();
        let base_scope = scope_parts[0];

        // Find the AST path for the base scope
        let ast_path = self
            .type_scope_mapping
            .get(base_scope)
            .ok_or_else(|| anyhow::anyhow!("Unknown scope: {}", base_scope))?;

        // Get the type definition using SymbolInspector
        match inspector
            .inspect_by_path(&self.document_uri, ast_path)
            .await
        {
            Ok(scoped_code) => {
                let response_content = if let Some(symbol) = symbol {
                    // For now, return the full type definition when a symbol is requested
                    // In the future, we can parse the type to extract specific field/method info
                    format!(
                        "Type '{}' with symbol '{}':\n{}\n\nFor nested inspection, use scope '{}.{}'",
                        base_scope, symbol, scoped_code.content, scope, symbol
                    )
                } else {
                    // Return the full type definition
                    format!("Type '{}' definition:\n{}", base_scope, scoped_code.content)
                };

                Ok(ToolCallResult {
                    tool_call_id: tool_call.id.clone(),
                    role: "tool".to_string(),
                    content: response_content,
                })
            }
            Err(e) => {
                // Try to provide helpful error message
                let error_msg = if base_scope == "string"
                    || base_scope == "int"
                    || base_scope == "bool"
                    || base_scope == "float64"
                    || base_scope == "float32"
                {
                    format!("'{}' is a built-in type", base_scope)
                } else {
                    format!("Failed to inspect '{}': {}", base_scope, e)
                };

                Ok(ToolCallResult {
                    tool_call_id: tool_call.id.clone(),
                    role: "tool".to_string(),
                    content: error_msg,
                })
            }
        }
    }
}

/// Create an inspect tool for type investigation
pub fn create_inspect_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "inspect".to_string(),
            description: "Inspect a type or symbol within a scope to get detailed information about its structure".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "description": "The scope ID to inspect (e.g., 'SimpleCache' or 'SimpleCache.FieldName')"
                    },
                    "symbol": {
                        "type": "string", 
                        "description": "Optional symbol name within the scope to focus on"
                    }
                },
                "required": ["scope"]
            }),
        },
    }
}

// Note: execute_tool_call is removed in favor of InspectTool::execute
// The dispatcher logic should be handled by the caller with proper context
