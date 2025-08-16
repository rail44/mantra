use actix::prelude::*;
use anyhow::Result;
use std::path::PathBuf;
use tracing::debug;

use super::actor::Workspace;
use super::messages::{DocumentShutdown, GenerateAll};
use crate::config::Config;

// ============================================================================
// Document Actor
// ============================================================================

/// Document actor managing a single document
pub struct DocumentActor {
    #[allow(dead_code)] // Will be used in full implementation
    config: Config,
    #[allow(dead_code)]
    file_path: PathBuf,
    uri: String,
    #[allow(dead_code)]
    workspace: Addr<Workspace>,
}

impl DocumentActor {
    pub async fn new(
        config: Config,
        file_path: PathBuf,
        uri: String,
        workspace: Addr<Workspace>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            file_path,
            uri,
            workspace,
        })
    }
}

impl Actor for DocumentActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("DocumentActor started for: {}", self.uri);
    }
}

impl Handler<GenerateAll> for DocumentActor {
    type Result = ResponseFuture<Result<String>>;

    fn handle(&mut self, _msg: GenerateAll, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("GenerateAll for: {}", self.uri);

        // TODO: Implement actual code generation logic
        // This will need to be migrated from DocumentManager
        Box::pin(async move { Ok("// Generated code placeholder\n".to_string()) })
    }
}

impl Handler<DocumentShutdown> for DocumentActor {
    type Result = ();

    fn handle(&mut self, _msg: DocumentShutdown, ctx: &mut Context<Self>) -> Self::Result {
        debug!("Shutting down DocumentActor: {}", self.uri);
        ctx.stop();
    }
}
