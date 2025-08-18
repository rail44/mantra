use mantra::config;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Setup structured logging with RUST_LOG environment variable
    // Default to "warn" if RUST_LOG is not set
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,mantra=info"));

    // Configure structured logging
    let format_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .compact();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(format_layer)
        .init();

    // Use Actix system for all operations
    match args.command {
        Commands::Generate { file } => generate_command(file),
    }
}

fn generate_command(file: PathBuf) -> Result<()> {
    use actix::prelude::*;
    use mantra::core::metrics::Timer;
    use mantra::workspace::{GenerateFile, Shutdown, Workspace};

    let total_timer = Timer::start("total_generation");

    // Load configuration by searching from the file's directory upward
    let config = config::Config::load(&file)?;
    info!("Configuration loaded successfully");

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

    total_timer.stop();
    Ok(())
}
