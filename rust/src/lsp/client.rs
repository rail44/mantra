use anyhow::Result;
use jsonrpsee::core::client::{Client as RpcClient, ClientT};
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::core::traits::ToRpcParams;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::lsp::transport::create_client;
use crate::lsp::types::*;

/// Newtype wrapper for serde_json::Value that implements ToRpcParams
/// This enables LSP-compatible parameter passing using ObjectParams internally
#[derive(Debug, Clone)]
pub struct LspParams(Value);

impl From<Value> for LspParams {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl ToRpcParams for LspParams {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        match self.0 {
            Value::Object(map) => {
                let mut params = ObjectParams::new();
                for (k, v) in map {
                    params.insert(&k, v)?;
                }
                params.to_rpc_params()
            }
            _ => {
                // For non-object values, serialize directly to RawValue
                let json_str = serde_json::to_string(&self.0)?;
                serde_json::value::RawValue::from_string(json_str).map(Some)
            }
        }
    }
}

/// LSP client that manages a language server process and provides RPC communication
pub struct Client {
    rpc: Arc<RpcClient>,
    _process: Child,
    _notification_rx: mpsc::UnboundedReceiver<NotificationMessage>,
}

impl Client {
    /// Start a new LSP server process and connect to it
    pub async fn start(
        command: &str,
        args: &[&str],
    ) -> Result<Self> {
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
        
        // Create jsonrpsee client with duplex transport
        let (rpc_client, notification_rx) = create_client(stdout, stdin).await?;
        
        let client = Self {
            rpc: Arc::new(rpc_client),
            _process: process,
            _notification_rx: notification_rx,
        };
        
        Ok(client)
    }
    
    /// Send a request and wait for response
    pub async fn request<R>(&self, method: &str, params: Value) -> Result<R> 
    where
        R: DeserializeOwned,
    {
        debug!("Sending request: {} with params: {:?}", method, params);
        
        let result = self.rpc
            .request(method, LspParams::from(params))
            .await
            .map_err(|e| anyhow::anyhow!("RPC error: {:?}", e))?;
        
        Ok(result)
    }
    
    /// Send a notification (no response expected)
    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        debug!("Sending notification: {} with params: {:?}", method, params);
        
        self.rpc
            .notification(method, LspParams::from(params))
            .await
            .map_err(|e| anyhow::anyhow!("RPC error: {:?}", e))?;
        
        Ok(())
    }
    
    /// Initialize the LSP connection
    pub async fn initialize(&self, root_uri: &str) -> Result<Value> {
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": format!("file://{}", root_uri),
            "capabilities": {
                "textDocument": {
                    "hover": {
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "synchronization": {
                        "didOpen": true
                    }
                }
            },
            "workspaceFolders": [{
                "uri": format!("file://{}", root_uri),
                "name": "workspace"
            }]
        });
        
        self.request("initialize", params).await
    }
}