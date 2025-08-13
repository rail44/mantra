mod config;
mod parser;
mod llm;
mod generator;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Mantra - AI-powered Go code generation tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    
    /// Log level (error, warn, info, debug, trace)
    #[arg(long, short = 'l', global = true)]
    log_level: Option<String>,
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
    
    // Setup logging
    let log_level = args.log_level
        .as_deref()
        .and_then(|s| s.parse::<Level>().ok())
        .unwrap_or(Level::WARN);
    
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    // Use tokio runtime for async operations
    tokio::runtime::Runtime::new()?.block_on(async {
        match args.command {
            Commands::Generate { file, config_dir } => {
                generate_command(file, config_dir).await
            }
        }
    })
}

async fn generate_command(file: PathBuf, config_dir: PathBuf) -> Result<()> {
    use generator::Generator;
    
    // Load configuration
    info!("Loading configuration from: {}", config_dir.display());
    let config = config::Config::load(&config_dir)?;
    
    info!("Generating code for: {}", file.display());
    
    // Create generator
    let generator = Generator::new(config)?;
    
    // Generate code
    let result = generator.generate_file(&file).await?;
    
    // Output to stdout
    print!("{}", result);
    
    Ok(())
}