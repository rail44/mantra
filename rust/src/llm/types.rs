use serde::{Deserialize, Serialize};

/// Role in the message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
}

/// Provider specification for OpenRouter
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
}

/// Request for completion API
#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderSpec>,
}

/// Response from completion API - only fields we actually use
#[derive(Debug, Clone, Deserialize)]
pub struct CompletionResponse {
    pub choices: Vec<Choice>,
}

/// Choice in the response - only fields we actually use
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub message: Message,
}
