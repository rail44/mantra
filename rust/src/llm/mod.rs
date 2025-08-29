pub mod client;
pub mod error;
pub mod tools;
pub mod types;

#[cfg(test)]
mod response_test;

pub use client::LLMClient;
pub use tools::{create_inspect_tool, InspectTool, Tool, ToolCall, ToolCallResult};
pub use types::{CompletionRequest, Message, ProviderSpec};
