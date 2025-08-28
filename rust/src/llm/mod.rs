pub mod client;
pub mod error;
pub mod tools;
pub mod types;

#[cfg(test)]
mod response_test;

pub use client::LLMClient;
pub use tools::{create_dummy_tool, execute_tool_call, Tool, ToolCall, ToolCallResult};
pub use types::{CompletionRequest, Message, ProviderSpec};
