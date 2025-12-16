#[cfg(test)]
mod tests {
    use crate::llm::types::CompletionResponse;

    #[test]
    fn test_parse_real_response() {
        let json_response = r#"{
            "id": "chatcmpl-8xYZ123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo-0613",
            "system_fingerprint": "fp_44709d6fcb",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "func GetUser(id string) (*User, error) {\n    return nil, nil\n}"
                    },
                    "logprobs": null,
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 150,
                "completion_tokens": 45,
                "total_tokens": 195
            }
        }"#;

        let response: Result<CompletionResponse, _> = serde_json::from_str(json_response);
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.choices.len(), 1);
        assert!(response.choices[0].message.content.contains("func GetUser"));
    }

    #[test]
    fn test_parse_openrouter_response() {
        // OpenRouterは基本的に同じ形式
        let json_response = r#"{
            "id": "gen-abc123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "openai/gpt-3.5-turbo",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Generated code here"
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;

        let response: Result<CompletionResponse, _> = serde_json::from_str(json_response);
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Generated code here");
    }
}
