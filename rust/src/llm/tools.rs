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

/// Create a dummy no-op tool for testing
pub fn create_dummy_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "dummy".to_string(),
            description: "A dummy tool that does nothing, for testing tool calls".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Any message to echo back"
                    }
                },
                "required": []
            }),
        },
    }
}

/// Execute tool call (dispatcher)
pub fn execute_tool_call(tool_call: &ToolCall) -> anyhow::Result<ToolCallResult> {
    match tool_call.function.name.as_str() {
        "dummy" => execute_dummy_tool(tool_call),
        _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_call.function.name)),
    }
}

/// Execute dummy tool
fn execute_dummy_tool(tool_call: &ToolCall) -> anyhow::Result<ToolCallResult> {
    let args: Value = serde_json::from_str(&tool_call.function.arguments)?;
    let message = args
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("No message provided");

    Ok(ToolCallResult {
        tool_call_id: tool_call.id.clone(),
        role: "tool".to_string(),
        content: format!("Dummy tool executed with message: {}", message),
    })
}
