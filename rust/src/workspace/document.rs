use anyhow::{Context as AnyhowContext, Result};

use actix::prelude::*;
use anyhow::{Context as AnyhowContext, Result};
use std::path::PathBuf;
use tracing::{debug, error, info};

use super::actor::Workspace;
use super::messages::{
    ApplyEdit, DocumentShutdown, FormatDocument, GenerateAll, GetFileUri, GetSource, GetTargetInfo,
    SendDidChange,
};
use crate::config::Config;
use crate::editor::crdt::{CrdtEditor, Snapshot};
use crate::generation::EditEvent;
use crate::lsp::Client as LspClient;
use crate::parser::{target_map::TargetMap, GoParser};
use tree_sitter::{InputEdit, Tree};
use lsp_types::{Position, TextEdit};

pub struct Document {
    uri: String,
    workspace: Addr<Workspace>,
    lsp_client: LspClient,
    parser: GoParser,
    tree: Tree,
    editor: CrdtEditor,
}

impl Document {
    pub fn new(
        uri: String,
        workspace: Addr<Workspace>,
        lsp_client: LspClient,
        parser: GoParser,
        tree: Tree,
        editor: CrdtEditor,
    ) -> Self {
        Document {
            uri,
            workspace,
            lsp_client,
            parser,
            tree,
            editor,
        }
    }

    fn sync_tree(&mut self, edit: &TextEdit) -> Result<()> {
        let input_edit = InputEdit {
            range: edit.range,
            new_text: edit.new_text.clone(),
        };
        self.tree.edit(edit)?;
        let new_tree = self.parser.parse(source, &self.tree)?;
        self.tree = new_tree;
        Ok(())
    }

    fn build_target_map(&self, source: &str) -> Result<TargetMap> {
        let map = TargetMap::build(&self.tree, &source)?


    }
}
