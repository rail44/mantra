use anyhow::Result;
use jsonrpsee::core::client::{Client as RpcClient, ClientBuilder};
use std::ops::Deref;
use std::sync::Arc;
use tokio::io::BufReader as AsyncBufReader;
use tokio::process::{Child, Command};
use tracing::info;

use crate::lsp::transport::{StdioReceiver, StdioSender};
use crate::lsp::{NotificationHandler, PublishDiagnosticsParams};

/// Create a new LSP client by starting a language server process
/// Returns a RpcClient that implements LspRpcClient trait methods
/// NOTE: 後方互換性のため、通知ハンドラーなしのバージョンを維持
pub async fn create_lsp_client(command: &str, args: &[&str]) -> Result<(RpcClient, Child)> {
    info!("Starting LSP server: {} {:?}", command, args);

    let mut cmd = Command::new(command);
    for arg in args {
        cmd.arg(arg);
    }

    let mut process = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let stdin = process.stdin.take().expect("Failed to get stdin");
    let stdout = process.stdout.take().expect("Failed to get stdout");

    // Create transport components
    let sender = StdioSender::new(stdin);
    let receiver = StdioReceiver::new(AsyncBufReader::new(stdout));

    // Build the client
    let rpc_client = ClientBuilder::default().build_with_tokio(sender, receiver);

    Ok((rpc_client, process))
}

/// 通知ハンドラー付きのLSPクライアントを作成
pub async fn create_lsp_client_with_notifications(
    command: &str, 
    args: &[&str]
) -> Result<(RpcClient, Child, Arc<NotificationHandler>)> {
    info!("Starting LSP server with notification support: {} {:?}", command, args);

    let mut cmd = Command::new(command);
    for arg in args {
        cmd.arg(arg);
    }

    let mut process = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let stdin = process.stdin.take().expect("Failed to get stdin");
    let stdout = process.stdout.take().expect("Failed to get stdout");

    // 通知ハンドラーを作成
    let notification_handler = Arc::new(NotificationHandler::new());

    // Create transport components with notification handler
    let sender = StdioSender::new(stdin);
    let receiver = StdioReceiver::with_notification_handler(
        AsyncBufReader::new(stdout),
        notification_handler.clone(),
    );

    // Build the client
    let rpc_client = ClientBuilder::default().build_with_tokio(sender, receiver);

    Ok((rpc_client, process, notification_handler))
}

/// LSPクライアントラッパー（通知ハンドラー付き）
pub struct Client {
    client: RpcClient,
    process: Child,
    notification_handler: Arc<NotificationHandler>,
}

impl Client {
    /// 新しいLSPクライアントを作成
    pub async fn new(command: &str, args: &[&str]) -> Result<Self> {
        let (client, process, notification_handler) = 
            create_lsp_client_with_notifications(command, args).await?;
        
        Ok(Self {
            client,
            process,
            notification_handler,
        })
    }
    
    /// RpcClientへの参照を取得
    pub fn rpc_client(&self) -> &RpcClient {
        &self.client
    }
    
    /// 診断情報を待機
    pub async fn wait_for_diagnostics(&self, uri: &str) -> Result<PublishDiagnosticsParams> {
        self.notification_handler.wait_for_diagnostics(uri).await
    }
    
    /// タイムアウト付きで診断情報を待機
    pub async fn wait_for_diagnostics_timeout(
        &self, 
        uri: &str,
        timeout: std::time::Duration
    ) -> Result<PublishDiagnosticsParams> {
        self.notification_handler.wait_for_diagnostics_timeout(uri, timeout).await
    }
    
    /// プロセスを終了
    pub async fn shutdown(mut self) -> Result<()> {
        self.process.kill().await?;
        Ok(())
    }
}

// RpcClientへの透過的なアクセスを提供
impl Deref for Client {
    type Target = RpcClient;
    
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
