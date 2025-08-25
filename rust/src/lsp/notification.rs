use anyhow::Result;
use lsp_types::PublishDiagnosticsParams;
use serde_json::Value;
use tokio::sync::broadcast;

/// LSP通知ハンドラー
pub struct NotificationHandler {
    diagnostics_tx: broadcast::Sender<PublishDiagnosticsParams>,
}

impl std::fmt::Debug for NotificationHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationHandler")
            .field("has_diagnostics_channel", &true)
            .finish()
    }
}

impl Default for NotificationHandler {
    fn default() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self { diagnostics_tx: tx }
    }
}

impl NotificationHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// 通知メッセージを処理する
    pub async fn handle_notification(&self, method: &str, params: Value) -> Result<()> {
        match method {
            "textDocument/publishDiagnostics" => {
                let diagnostics: PublishDiagnosticsParams = serde_json::from_value(params)?;
                tracing::debug!(
                    "Received diagnostics for {}: {} items",
                    diagnostics.uri.as_str(),
                    diagnostics.diagnostics.len()
                );
                self.diagnostics_tx.send(diagnostics)?;
            }
            _ => {
                tracing::trace!("Unhandled notification: {}", method);
            }
        }
        Ok(())
    }
}
