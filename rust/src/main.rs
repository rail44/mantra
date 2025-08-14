use mantra::{config, document};

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Mantra - AI-powered Go code generation tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate code for a Go file
    Generate {
        /// Go file to process
        file: PathBuf,

        /// Config directory (contains mantra.toml)
        #[arg(long, short = 'c', default_value = ".")]
        config_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging with RUST_LOG environment variable
    // Default to "warn" if RUST_LOG is not set
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Use tokio runtime for async operations
    tokio::runtime::Runtime::new()?.block_on(async {
        match args.command {
            Commands::Generate { file, config_dir } => generate_command(file, config_dir).await,
        }
    })
}

async fn generate_command(file: PathBuf, config_dir: PathBuf) -> Result<()> {
    use document::DocumentManager;

    // Load configuration
    info!("Loading configuration from: {}", config_dir.display());
    let config = config::Config::load(&config_dir)?;

    info!("Generating code for: {}", file.display());

    // Create document manager with LSP support
    let mut doc_manager = DocumentManager::new(config, &file).await?;

    // Generate code
    let result = doc_manager.generate_all().await?;

    // Output to stdout
    print!("{}", result);

    Ok(())
}
