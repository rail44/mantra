#[cfg(test)]
mod workspace_tests {
    use super::super::*;
    use crate::config::Config;
    use crate::document::DocumentCommand;
    use std::path::PathBuf;
    use tokio::sync::oneshot;

    async fn create_test_config() -> Config {
        Config {
            url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            model: "anthropic/claude-3.5-sonnet".to_string(),
            api_key: Some("test-key".to_string()),
            log_level: Some("error".to_string()),
            openrouter: None,
        }
    }

    #[tokio::test]
    async fn test_workspace_creation() {
        let root_dir = PathBuf::from(".");
        let config = create_test_config().await;

        let workspace_tx = Workspace::spawn(root_dir, config).await;
        assert!(
            workspace_tx.is_ok(),
            "Workspace should be created successfully"
        );

        // Shutdown
        if let Ok(tx) = workspace_tx {
            let _ = tx.send(WorkspaceCommand::Shutdown).await;
        }
    }

    #[tokio::test]
    async fn test_document_actor_lifecycle() {
        let root_dir = PathBuf::from(".");
        let config = create_test_config().await;
        let workspace_tx = Workspace::spawn(root_dir, config).await.unwrap();

        // Create a test file
        let test_content = "package main\n\nfunc main() {}\n";
        let test_file = "target/test_workspace_lifecycle.go";
        std::fs::write(test_file, test_content).unwrap();

        // Get document actor
        let uri = format!(
            "file://{}",
            std::env::current_dir().unwrap().join(test_file).display()
        );

        let (tx, rx) = oneshot::channel();
        workspace_tx
            .send(WorkspaceCommand::GetDocument {
                uri: uri.clone(),
                response: tx,
            })
            .await
            .unwrap();
        let document_sender = rx.await.unwrap().unwrap();

        // Test getting source
        let (tx, rx) = oneshot::channel();
        document_sender
            .send(DocumentCommand::GetSource { response: tx })
            .await
            .unwrap();

        let result = rx.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_content);

        // Clean up
        workspace_tx.send(WorkspaceCommand::Shutdown).await.ok();
        std::fs::remove_file(test_file).ok();
    }

    #[tokio::test]
    async fn test_document_actor_reuse() {
        let root_dir = PathBuf::from(".");
        let config = create_test_config().await;
        let workspace_tx = Workspace::spawn(root_dir, config).await.unwrap();

        // Create a test file
        let test_file = "target/test_workspace_reuse.go";
        std::fs::write(test_file, "package main").unwrap();

        let uri = format!(
            "file://{}",
            std::env::current_dir().unwrap().join(test_file).display()
        );

        // Get document actor twice - should reuse the same actor
        let (tx, rx) = oneshot::channel();
        workspace_tx
            .send(WorkspaceCommand::GetDocument {
                uri: uri.clone(),
                response: tx,
            })
            .await
            .unwrap();
        let _sender1 = rx.await.unwrap().unwrap();

        let (tx, rx) = oneshot::channel();
        workspace_tx
            .send(WorkspaceCommand::GetDocument {
                uri: uri.clone(),
                response: tx,
            })
            .await
            .unwrap();
        let sender2 = rx.await.unwrap().unwrap();

        // Both senders should work
        let (tx, rx) = oneshot::channel();
        sender2
            .send(DocumentCommand::GetSource { response: tx })
            .await
            .unwrap();
        assert!(rx.await.unwrap().is_ok());

        // Clean up
        workspace_tx.send(WorkspaceCommand::Shutdown).await.ok();
        std::fs::remove_file(test_file).ok();
    }
}
