use jsonrpsee::core::async_trait;
use jsonrpsee::core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use std::fmt;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tracing::debug;

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
}

#[async_trait]
impl TransportSenderT for StdioSender {
    type Error = TransportError;

    async fn send(&mut self, msg: String) -> Result<(), Self::Error> {
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

/// Handles receiving LSP messages from stdout
pub struct StdioReceiver {
    stdout: AsyncBufReader<ChildStdout>,
}

impl StdioReceiver {
    pub fn new(stdout: AsyncBufReader<ChildStdout>) -> Self {
        Self { stdout }
    }
}

#[async_trait]
impl TransportReceiverT for StdioReceiver {
    type Error = TransportError;

    async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
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
        Ok(ReceivedMessage::Bytes(buffer))
    }
}
