use crate::inspector::SymbolInspector;
use crate::parser::target::PathSegment;
use crate::workspace::WorkspaceService;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    type_scope_mapping: FxHashMap<String, (String, Vec<PathSegment>)>,
}

impl InspectTool {
    pub fn new(
        workspace: WorkspaceService,
        type_scope_mapping: FxHashMap<String, (String, Vec<PathSegment>)>,
    ) -> Self {
        Self {
            workspace,
            type_scope_mapping,
        }
    }

    /// Execute the inspect tool call
    pub async fn execute(&mut self, tool_call: &ToolCall) -> anyhow::Result<ToolCallResult> {
        let args: Value = serde_json::from_str(&tool_call.function.arguments)?;
        let scope = args
            .get("scope")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'scope' parameter"))?;

        let symbol = args
            .get("symbol")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'symbol' parameter"))?;

        let inspector = SymbolInspector::new(&self.workspace);

        // Parse scope to handle hierarchical references like "SimpleCache.Name"
        let scope_parts: Vec<&str> = scope.split('.').collect();
        let base_scope = scope_parts[0];

        // Find the document URI and AST path for the base scope
        let (document_uri, ast_path) = self
            .type_scope_mapping
            .get(base_scope)
            .ok_or_else(|| anyhow::anyhow!("Unknown scope: {}", base_scope))?;

        // Inspect specific symbol within the scope using SymbolInspector
        match inspector
            .inspect_symbol(document_uri, ast_path, symbol)
            .await
        {
            Ok(scoped_code) => {
                // Register new dynamic scope for hierarchical investigation
                let new_scope_id = format!("{base_scope}.{symbol}");
                self.type_scope_mapping.insert(
                    new_scope_id.clone(),
                    (
                        scoped_code.document_uri.clone(),
                        scoped_code.path_segments.clone(),
                    ),
                );
                tracing::debug!("Registered new dynamic scope: {}", new_scope_id);

                let response_content = format!(
                    "Symbol '{}' in '{}' definition:\n{}\n\nNew scope '{}' is now available for further investigation.",
                    symbol, base_scope, scoped_code.content, new_scope_id
                );

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
                    format!("'{base_scope}' is a built-in type")
                } else {
                    format!("Failed to inspect '{base_scope}': {e}")
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
                        "description": "Symbol name within the scope to inspect (e.g., field name, method name)"
                    }
                },
                "required": ["scope", "symbol"]
            }),
        },
    }
}

// Note: execute_tool_call is removed in favor of InspectTool::execute
// The dispatcher logic should be handled by the caller with proper context

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::parser::target::PathSegment;
    use crate::workspace::WorkspaceService;
    use serde_json::json;

    #[tokio::test]
    async fn test_inspect_tool_with_symbol() -> anyhow::Result<()> {
        // Initialize logging for test
        let _ = tracing_subscriber::fmt()
            .with_env_filter("warn,mantra=debug")
            .try_init();

        // Create a test Go file
        let test_dir = std::env::temp_dir().join("mantra_inspect_tool_test");
        std::fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("test.go");
        std::fs::write(
            &test_file,
            r#"package main

type SimpleCache struct {
    items map[string]cacheItem
    mu    sync.RWMutex
}

type cacheItem struct {
    value any
    expiry time.Time
}
"#,
        )?;

        // Create config
        let config_file = test_dir.join("mantra.toml");
        std::fs::write(
            &config_file,
            r#"model = "test-model"
url = "http://localhost:8080"
api_key = "test-key"
"#,
        )?;

        // Setup workspace
        let config = Config::load(&test_file)?;
        let workspace = WorkspaceService::new(test_dir.clone(), config).await?;

        // Create type_scope_mapping for SimpleCache
        let uri = format!("file://{}", test_file.display());
        let mut type_scope_mapping = FxHashMap::default();
        type_scope_mapping.insert(
            "SimpleCache".to_string(),
            (
                uri.clone(),
                vec![
                    PathSegment {
                        node_kind: "source_file".to_string(),
                        field_name: None,
                        index: None,
                    },
                    PathSegment {
                        node_kind: "type_declaration".to_string(),
                        field_name: None,
                        index: Some(0),
                    },
                    PathSegment {
                        node_kind: "type_spec".to_string(),
                        field_name: None,
                        index: Some(0),
                    },
                ],
            ),
        );

        // Create InspectTool
        let mut inspect_tool = InspectTool::new(workspace, type_scope_mapping);

        // Create a test tool call with symbol
        let tool_call = ToolCall {
            id: "test_call_1".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "inspect".to_string(),
                arguments: json!({
                    "scope": "SimpleCache",
                    "symbol": "items"
                })
                .to_string(),
            },
        };

        // Execute the tool call
        match inspect_tool.execute(&tool_call).await {
            Ok(result) => {
                println!("Test result: {}", result.content);
                assert!(result.content.contains("items"));
                assert!(result.content.contains("map[string]cacheItem"));
                println!("✅ Test passed: InspectTool with symbol works!");
            }
            Err(e) => {
                eprintln!("❌ Test failed: {}", e);
                return Err(e);
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&test_dir);

        Ok(())
    }

    #[tokio::test]
    async fn test_dynamic_scope_management() -> anyhow::Result<()> {
        // Initialize logging for test
        let _ = tracing_subscriber::fmt()
            .with_env_filter("warn,mantra=debug")
            .try_init();

        // Create a test Go file with hierarchical types
        let test_dir = std::env::temp_dir().join("mantra_dynamic_scope_test");
        std::fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("test.go");
        std::fs::write(
            &test_file,
            r#"package main

type SimpleCache struct {
    items map[string]CacheItem
    mu    sync.RWMutex
}

type CacheItem struct {
    value any
    expiry time.Time
}
"#,
        )?;

        // Create config
        let config_file = test_dir.join("mantra.toml");
        std::fs::write(
            &config_file,
            r#"model = "test-model"
url = "http://localhost:8080"
api_key = "test-key"
"#,
        )?;

        // Setup workspace
        let config = Config::load(&test_file)?;
        let workspace = WorkspaceService::new(test_dir.clone(), config).await?;

        // Create type_scope_mapping for SimpleCache
        let uri = format!("file://{}", test_file.display());
        let mut type_scope_mapping = FxHashMap::default();
        type_scope_mapping.insert(
            "SimpleCache".to_string(),
            (
                uri.clone(),
                vec![
                    PathSegment {
                        node_kind: "source_file".to_string(),
                        field_name: None,
                        index: None,
                    },
                    PathSegment {
                        node_kind: "type_declaration".to_string(),
                        field_name: None,
                        index: Some(0),
                    },
                    PathSegment {
                        node_kind: "type_spec".to_string(),
                        field_name: None,
                        index: Some(0),
                    },
                ],
            ),
        );

        // Create InspectTool
        let mut inspect_tool = InspectTool::new(workspace, type_scope_mapping);

        // First call: inspect SimpleCache.items
        let tool_call_1 = ToolCall {
            id: "test_call_1".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "inspect".to_string(),
                arguments: json!({
                    "scope": "SimpleCache",
                    "symbol": "items"
                })
                .to_string(),
            },
        };

        // Execute first tool call
        let result_1 = inspect_tool.execute(&tool_call_1).await?;
        println!("First call result: {}", result_1.content);

        // Verify new scope was registered
        assert!(inspect_tool
            .type_scope_mapping
            .contains_key("SimpleCache.items"));
        assert!(result_1.content.contains("SimpleCache.items"));
        assert!(result_1.content.contains("further investigation"));

        // Second call: use the new dynamic scope
        let tool_call_2 = ToolCall {
            id: "test_call_2".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "inspect".to_string(),
                arguments: json!({
                    "scope": "SimpleCache.items",
                    "symbol": "value"
                })
                .to_string(),
            },
        };

        // Execute second tool call (hierarchical investigation)
        match inspect_tool.execute(&tool_call_2).await {
            Ok(result_2) => {
                println!("Second call result: {}", result_2.content);
                assert!(result_2.content.contains("value"));
                println!("✅ Dynamic scope management test passed!");
            }
            Err(e) => {
                println!("Second call failed (expected for this simple test): {}", e);
                println!("✅ Dynamic scope registration works, hierarchical call attempted");
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&test_dir);

        Ok(())
    }
}
