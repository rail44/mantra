use actix::prelude::*;
use anyhow::Result;
use lsp_types::Range;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use super::{InitializeTool, ShutdownTool, ToolActor};

// ============================================================================
// InspectTool Actor
// ============================================================================

/// Tool for inspecting symbols in code using LSP
#[derive(Debug, Default)]
pub struct InspectTool {
    /// Map of scope IDs to their locations
    scopes: HashMap<String, ScopeInfo>,
    /// Counter for generating unique scope IDs
    next_scope_id: usize,
}

/// Information about a scope
#[derive(Debug, Clone)]
pub struct ScopeInfo {
    /// File URI
    pub uri: String,
    /// Range in the file
    pub range: Range,
}

impl InspectTool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a scope for inspection
    pub fn register_scope(&mut self, uri: String, range: Range) -> String {
        let scope_id = format!("scope_{}", self.next_scope_id);
        self.next_scope_id += 1;

        self.scopes
            .insert(scope_id.clone(), ScopeInfo { uri, range });

        debug!("Registered scope: {}", scope_id);
        scope_id
    }
}

impl Actor for InspectTool {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("InspectTool actor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("InspectTool actor stopped");
    }
}

impl ToolActor for InspectTool {
    fn name(&self) -> &str {
        "InspectTool"
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Request to inspect a symbol
#[derive(Debug, Clone, Deserialize)]
pub struct InspectRequest {
    /// Scope ID to inspect within
    pub scope_id: String,
    /// Symbol name to inspect
    pub symbol: String,
}

/// Response from inspection
#[derive(Debug, Clone, Serialize)]
pub struct InspectResponse {
    /// New scope ID for the definition
    pub scope_id: String,
    /// Code snippet of the definition
    pub code: String,
}

/// Message to register a scope
#[derive(Message, Debug)]
#[rtype(result = "String")]
pub struct RegisterScope {
    pub uri: String,
    pub range: Range,
}

/// Message to inspect a symbol
#[derive(Message, Debug)]
#[rtype(result = "Result<InspectResponse>")]
pub struct Inspect {
    pub request: InspectRequest,
    pub lsp_client: crate::lsp::Client,
}

// ============================================================================
// Message Handlers
// ============================================================================

impl Handler<InitializeTool> for InspectTool {
    type Result = Result<()>;

    fn handle(&mut self, _msg: InitializeTool, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("Initializing InspectTool");
        self.initialize()
    }
}

impl Handler<ShutdownTool> for InspectTool {
    type Result = ();

    fn handle(&mut self, _msg: ShutdownTool, ctx: &mut Context<Self>) -> Self::Result {
        debug!("Shutting down InspectTool");
        ctx.stop();
    }
}

impl Handler<RegisterScope> for InspectTool {
    type Result = String;

    fn handle(&mut self, msg: RegisterScope, _ctx: &mut Context<Self>) -> Self::Result {
        self.register_scope(msg.uri, msg.range)
    }
}

impl Handler<Inspect> for InspectTool {
    type Result = ResponseFuture<Result<InspectResponse>>;

    fn handle(&mut self, msg: Inspect, _ctx: &mut Context<Self>) -> Self::Result {
        let scope_info = match self.scopes.get(&msg.request.scope_id) {
            Some(info) => info.clone(),
            None => {
                return Box::pin(async move {
                    Err(anyhow::anyhow!("Scope {} not found", msg.request.scope_id))
                });
            }
        };

        let request = msg.request;
        let lsp_client = msg.lsp_client;

        Box::pin(async move {
            // Find symbol position in the scope
            let symbol_position = find_symbol_in_scope(
                &lsp_client,
                &scope_info.uri,
                &scope_info.range,
                &request.symbol,
            )
            .await?;

            // Get definition location
            let definition_location = lsp_client
                .goto_definition(
                    lsp_types::TextDocumentIdentifier {
                        uri: lsp_types::Url::parse(&scope_info.uri).unwrap(),
                    },
                    symbol_position,
                )
                .await?;

            // Get code at definition
            let code = if let Some(location) = definition_location.first() {
                get_definition_code(&lsp_client, location).await?
            } else {
                return Err(anyhow::anyhow!(
                    "No definition found for symbol: {}",
                    request.symbol
                ));
            };

            Ok(InspectResponse {
                scope_id: format!("def_{}", request.symbol),
                code,
            })
        })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn find_symbol_in_scope(
    lsp_client: &crate::lsp::Client,
    uri: &str,
    range: &Range,
    symbol: &str,
) -> Result<lsp_types::Position> {
    // Get document symbols
    let symbols = lsp_client
        .document_symbols(lsp_types::TextDocumentIdentifier {
            uri: lsp_types::Url::parse(uri).unwrap(),
        })
        .await?;

    // Find the symbol within the range
    for sym in symbols {
        if sym.name == symbol && is_position_in_range(&sym.range.start, range) {
            return Ok(sym.range.start);
        }
    }

    // If not found in symbols, search in text (simplified approach)
    // In a real implementation, this would be more sophisticated
    Ok(range.start)
}

async fn get_definition_code(
    lsp_client: &crate::lsp::Client,
    location: &crate::lsp::Location,
) -> Result<String> {
    // Get hover information at the definition location
    let hover = lsp_client
        .hover(
            lsp_types::TextDocumentIdentifier {
                uri: location.uri.clone(),
            },
            location.range.start,
        )
        .await?;

    if let Some(hover) = hover {
        Ok(format_hover_content(hover))
    } else {
        Ok("// Definition found but no hover information available".to_string())
    }
}

fn is_position_in_range(pos: &lsp_types::Position, range: &Range) -> bool {
    pos.line >= range.start.line && pos.line <= range.end.line
}

fn format_hover_content(hover: crate::lsp::Hover) -> String {
    use lsp_types::{HoverContents, MarkedString, MarkupContent};

    match hover.contents {
        HoverContents::Scalar(MarkedString::String(text)) => text,
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => ls.value,
        HoverContents::Markup(MarkupContent { value, .. }) => value,
        HoverContents::Array(items) => items
            .into_iter()
            .map(|item| match item {
                MarkedString::String(s) => s,
                MarkedString::LanguageString(ls) => ls.value,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
