mod config;
mod parser;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Mantra - AI-powered Go code generation tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Target package directory
    #[arg(default_value = ".")]
    target: PathBuf,
    
    /// Log level (error, warn, info, debug, trace)
    #[arg(long, short = 'l')]
    log_level: Option<String>,
    
    /// Plain output mode (no TUI)
    #[arg(long, short = 'p')]
    plain: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Setup logging
    let log_level = args.log_level
        .as_deref()
        .and_then(|s| s.parse::<Level>().ok())
        .unwrap_or(Level::INFO);
    
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    // Load configuration
    info!("Loading configuration from target: {}", args.target.display());
    let config = config::Config::load(&args.target)?;
    
    info!("Configuration loaded successfully:");
    info!("  Model: {}", config.model);
    info!("  URL: {}", config.url);
    info!("  Dest: {}", config.dest);
    info!("  API Key: {}", if config.api_key.is_some() { "Set" } else { "Not set" });
    
    // TODO: Implement the rest of the application
    println!("Mantra Rust version - Configuration loaded successfully!");
    
    Ok(())
}