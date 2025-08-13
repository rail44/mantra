use mantra::config::Config;
use mantra::generator::Generator;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Test with simple example
    let config_path = Path::new("../examples/simple");
    let config = Config::load(config_path)?;

    println!("Config loaded: model={}", config.model);

    let generator = Generator::new(config)?;

    let file_path = Path::new("../examples/simple/simple.go");
    println!("Processing file: {}", file_path.display());

    let result = generator.generate_file(file_path).await?;

    println!("Generated output:");
    println!("{}", result);

    Ok(())
}
