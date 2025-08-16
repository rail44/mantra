use mantra::config;

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

    // Use Actix system for all operations
    match args.command {
        Commands::Generate { file, config_dir } => generate_command(file, config_dir),
    }
}

fn generate_command(file: PathBuf, config_dir: PathBuf) -> Result<()> {
    use actix::prelude::*;
    use mantra::workspace::{GenerateFile, Shutdown, Workspace};

    // Load configuration
    info!("Loading configuration from: {}", config_dir.display());
    let config = config::Config::load(&config_dir)?;

    info!("Generating code for: {}", file.display());

    // Get workspace root (parent directory of the file)
    let workspace_root = file
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
        .to_path_buf();

    // Create Actix system
    let system = System::new();

    let result = system.block_on(async {
        // Create and start Workspace actor
        let addr = Workspace::start_actor(workspace_root, config).await?;

        // Send GenerateFile message
        let result = addr.send(GenerateFile { file_path: file }).await??;

        // Shutdown workspace
        addr.send(Shutdown).await?;

        // Stop the system
        System::current().stop();

        Ok::<String, anyhow::Error>(result)
    })?;

    // Output to stdout
    print!("{}", result);

    Ok(())
}
