use actix::prelude::*;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::llm::LLMClient;
use crate::lsp::Client as LspClient;
use crate::tools::inspect::InspectTool;

// Re-export for backward compatibility during migration
pub use self::actor::*;
pub use self::messages::*;

mod actor;
mod document;
mod messages;

#[cfg(test)]
mod tests;

// Keep the old module available temporarily for comparison
// mod mod_old;
