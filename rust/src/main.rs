use anyhow::Result;
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

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    server::run_server().await
}

/// Setup logging to file for LSP server
fn setup_logging() {
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
