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

    // Use tokio runtime for async operations
    tokio::runtime::Runtime::new()?.block_on(async {
        match args.command {
            Commands::Generate { file, config_dir } => generate_command(file, config_dir).await,
        }
    })
}

async fn generate_command(file: PathBuf, config_dir: PathBuf) -> Result<()> {
    use mantra::workspace::{Workspace, WorkspaceCommand};
    use tokio::sync::oneshot;

    // Load configuration
    info!("Loading configuration from: {}", config_dir.display());
    let config = config::Config::load(&config_dir)?;

    info!("Generating code for: {}", file.display());

    // Get workspace root (parent directory of the file)
    let workspace_root = file
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
        .to_path_buf();

    // Spawn Workspace actor
    let workspace_tx = Workspace::spawn(workspace_root, config).await?;

    // Send generate command to workspace
    let (response_tx, response_rx) = oneshot::channel();
    workspace_tx
        .send(WorkspaceCommand::GenerateFile {
            file_path: file.clone(),
            response: response_tx,
        })
        .await?;

    // Wait for result
    let result = response_rx.await??;

    // Output to stdout
    print!("{}", result);

    // Shutdown workspace
    workspace_tx.send(WorkspaceCommand::Shutdown).await?;

    Ok(())
}
