use mantra::config::Config;
use mantra::llm::{CompletionRequest, LLMClient, Message};
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load the same config as Go version
    let config_path = Path::new("../examples/simple");
    let config = Config::load(config_path)?;

    println!("Loaded config:");
    println!("  Model: {}", config.model);
    println!("  URL: {}", config.url);
    println!(
        "  API Key: {}",
        if config.api_key.is_some() {
            "Set"
        } else {
            "Not set"
        }
    );

    if config.api_key.is_none() {
        println!("\nWarning: No API key found. Set OPENROUTER_API_KEY environment variable.");
        return Ok(());
    }

    // Create LLM client
    let client = LLMClient::new(config.clone())?;

    // Test request similar to what Mantra would send
    let request = CompletionRequest {
        model: config.model.clone(),
        messages: vec![
            Message::system("You are a Go code generator. Generate only the function body, no explanations."),
            Message::user("Generate Go code for this function:\n\n```go\n// Get value from cache. Return nil if not exists or expired.\nfunc (c *SimpleCache) Get(key string) any {\n    panic(\"not implemented\")\n}\n```\n\nRequirements:\n- Check if key exists in c.items map\n- Check expiration time\n- Return nil if expired or not exists\n- Delete expired items"),
        ],
        temperature: 0.2,
        max_tokens: Some(500),
    };

    println!("\nSending request to LLM...");

    // Send request to LLM (works with OpenRouter's OpenAI-compatible API)
    let response = client.complete(request).await?;

    println!("\nResponse received!");

    if let Some(choice) = response.choices.first() {
        println!("\nGenerated code:");
        println!("{}", choice.message.content);

        // Check if it looks like valid Go code
        if choice.message.content.contains("c.items[key]") {
            println!("\n✅ Response contains expected Go code patterns!");
        }
    }

    Ok(())
}
