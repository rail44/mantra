use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod config;
mod core;
mod document;
mod editor;
mod generation;
mod inspector;
mod language;
mod llm;
mod lsp;
mod parser;
mod server;
mod workspace;

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
    /// Start the LSP server (for editor integration)
    Lsp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Process commands
    match args.command {
        Commands::Generate { file } => {
            setup_stderr_logging();
            generate_command(file).await
        }
        Commands::Lsp => {
            setup_file_logging();
            server::run_server().await
        }
    }
}

/// Setup logging to stderr (for CLI commands)
fn setup_stderr_logging() {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,mantra=info"));

    let format_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
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
}

/// Setup logging to file (for LSP server)
fn setup_file_logging() {
    use std::fs::OpenOptions;

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/mantra-lsp.log")
        .expect("Failed to open log file");

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,mantra=debug"));

    let format_layer = fmt::layer()
        .with_writer(log_file)
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(format_layer)
        .init();
}

async fn generate_command(file: PathBuf) -> Result<()> {
    use crate::core::metrics::Timer;
    use crate::workspace::WorkspaceService;

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

    // Create workspace
    let workspace = WorkspaceService::new(workspace_root, config).await?;

    // Generate code
    let result = workspace.generate_file(file).await?;

    // Output to stdout
    print!("{result}");

    total_timer.stop();
    Ok(())
}
