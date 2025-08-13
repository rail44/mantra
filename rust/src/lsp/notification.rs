use crate::lsp::rpc::PublishDiagnosticsParams;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// LSP通知ハンドラー
pub struct NotificationHandler {
    diagnostics_tx: broadcast::Sender<PublishDiagnosticsParams>,
    diagnostics_rx: Arc<Mutex<broadcast::Receiver<PublishDiagnosticsParams>>>,
}

impl NotificationHandler {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(100);
        Self {
            diagnostics_tx: tx,
            diagnostics_rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// 通知メッセージを処理する
    pub async fn handle_notification(&self, method: &str, params: Value) -> Result<()> {
        match method {
            "textDocument/publishDiagnostics" => {
                let diagnostics: PublishDiagnosticsParams = serde_json::from_value(params)?;
                tracing::debug!(
                    "Received diagnostics for {}: {} items",
                    diagnostics.uri,
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

    /// 診断情報の受信を待機
    pub async fn wait_for_diagnostics(&self, uri: &str) -> Result<PublishDiagnosticsParams> {
        let mut rx = self.diagnostics_rx.lock().await;
        loop {
            match rx.recv().await {
                Ok(diagnostics) => {
                    if diagnostics.uri == uri {
                        return Ok(diagnostics);
                    }
                    // 他のURIの診断は無視して待機を続ける
                    tracing::trace!(
                        "Skipping diagnostics for different URI: {} (waiting for: {})",
                        diagnostics.uri,
                        uri
                    );
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Missed {} diagnostics notifications", n);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to receive diagnostics: {}", e));
                }
            }
        }
    }

    /// タイムアウト付きで診断情報を待機
    pub async fn wait_for_diagnostics_timeout(
        &self,
        uri: &str,
        timeout: std::time::Duration,
    ) -> Result<PublishDiagnosticsParams> {
        tokio::time::timeout(timeout, self.wait_for_diagnostics(uri))
            .await
            .map_err(|_| anyhow::anyhow!("Timeout waiting for diagnostics"))?
    }
}
