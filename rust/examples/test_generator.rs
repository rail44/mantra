use mantra::config::Config;
use mantra::generator::DocumentManager;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Test with simple example
    let config_path = Path::new("../examples/simple");
    let config = Config::load(config_path)?;

    println!("Config loaded: model={}", config.model);

    let file_path = Path::new("../examples/simple/simple.go");
    println!("Processing file: {}", file_path.display());

    let mut doc_manager = DocumentManager::new(config, file_path)?;
    let result = doc_manager.generate_all().await?;

    println!("Generated output:");
    println!("{}", result);

    Ok(())
}
