use jsonrpsee::core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use serde_json::Value;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tracing::debug;

use crate::lsp::NotificationHandler;

/// Error type for LSP transport operations
#[derive(Debug)]
pub struct TransportError(pub String);

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transport error: {}", self.0)
    }
}

impl std::error::Error for TransportError {}

/// Handles sending LSP messages to stdin
pub struct StdioSender {
    stdin: ChildStdin,
}

impl StdioSender {
    pub fn new(stdin: ChildStdin) -> Self {
        Self { stdin }
    }

    async fn send_impl(&mut self, msg: String) -> Result<(), TransportError> {
        let content_length = msg.len();
        let header = format!("Content-Length: {}\r\n\r\n", content_length);

        self.stdin
            .write_all(header.as_bytes())
            .await
            .map_err(|e| TransportError(format!("Failed to write header: {}", e)))?;
        self.stdin
            .write_all(msg.as_bytes())
            .await
            .map_err(|e| TransportError(format!("Failed to write message: {}", e)))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| TransportError(format!("Failed to flush: {}", e)))?;

        debug!("Sent LSP message: {}", msg);
        Ok(())
    }
}

impl TransportSenderT for StdioSender {
    type Error = TransportError;

    #[allow(refining_impl_trait)]
    fn send(
        &mut self,
        msg: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>> {
        Box::pin(self.send_impl(msg))
    }
}

/// Handles receiving LSP messages from stdout
pub struct StdioReceiver {
    stdout: AsyncBufReader<ChildStdout>,
    notification_handler: Option<Arc<NotificationHandler>>,
}

impl StdioReceiver {
    pub fn with_notification_handler(
        stdout: AsyncBufReader<ChildStdout>,
        handler: Arc<NotificationHandler>,
    ) -> Self {
        Self {
            stdout,
            notification_handler: Some(handler),
        }
    }

    async fn receive_impl(&mut self) -> Result<ReceivedMessage, TransportError> {
        // Read LSP headers
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            self.stdout
                .read_line(&mut line)
                .await
                .map_err(|e| TransportError(format!("Failed to read line: {}", e)))?;

            if line == "\r\n" || line == "\n" {
                break;
            }
            headers.push(line);
        }

        // Parse Content-Length
        let mut content_length = None;
        for header in &headers {
            if header.starts_with("Content-Length: ") {
                let len_str = header.trim_start_matches("Content-Length: ").trim();
                content_length = Some(len_str.parse::<usize>().map_err(|e| {
                    TransportError(format!("Failed to parse content length: {}", e))
                })?);
                break;
            }
        }

        let content_length = content_length
            .ok_or_else(|| TransportError("Missing Content-Length header".to_string()))?;

        // Read the message body
        let mut buffer = vec![0u8; content_length];
        self.stdout
            .read_exact(&mut buffer)
            .await
            .map_err(|e| TransportError(format!("Failed to read message body: {}", e)))?;

        debug!("Received LSP message: {}", String::from_utf8_lossy(&buffer));

        // 通知ハンドラーがある場合、notificationをチェック
        if let Some(handler) = &self.notification_handler {
            if let Ok(msg) = serde_json::from_slice::<Value>(&buffer) {
                // notificationかどうかチェック（idがなくmethodがある）
                if msg.get("id").is_none() && msg.get("method").is_some() {
                    if let (Some(method), params) = (
                        msg.get("method").and_then(|m| m.as_str()),
                        msg.get("params"),
                    ) {
                        let params = params.cloned().unwrap_or(Value::Null);
                        let handler = handler.clone();
                        let method = method.to_string();

                        // 非同期でハンドラーに渡す（ブロッキングを避ける）
                        tokio::spawn(async move {
                            if let Err(e) = handler.handle_notification(&method, params).await {
                                debug!("Failed to handle notification: {}", e);
                            }
                        });
                    }
                }
            }
        }

        Ok(ReceivedMessage::Bytes(buffer))
    }
}

impl TransportReceiverT for StdioReceiver {
    type Error = TransportError;

    #[allow(refining_impl_trait)]
    fn receive(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<ReceivedMessage, Self::Error>> + Send + '_>> {
        Box::pin(self.receive_impl())
    }
}
