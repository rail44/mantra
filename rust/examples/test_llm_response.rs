use mantra::config::Config;
use mantra::llm::{CompletionRequest, LLMClient, Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Test with a simple request to see actual response structure
    let config = Config {
        model: "gpt-3.5-turbo".to_string(),
        url: std::env::var("OPENAI_API_URL")
            .unwrap_or_else(|_| "https://api.openai.com".to_string()),
        dest: "./generated".to_string(),
        api_key: std::env::var("OPENAI_API_KEY").ok(),
        log_level: Some("debug".to_string()),
        plain: false,
        openrouter: None,
    };

    let client = LLMClient::new(config)?;

    let request = CompletionRequest {
        model: "gpt-3.5-turbo".to_string(),
        messages: vec![
            Message::system("You are a helpful assistant."),
            Message::user("Say 'Hello, World!' and nothing else."),
        ],
        temperature: 0.0,
        max_tokens: Some(10),
    };

    println!("Sending request...");
    let response = client.complete(request).await?;

    // Print the full response structure
    println!("Full response: {:#?}", response);

    // Print just the generated content
    if let Some(choice) = response.choices.first() {
        println!("\nGenerated content: {}", choice.message.content);
    }

    Ok(())
}
