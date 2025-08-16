use mantra::config::Config;
use mantra::workspace::{Workspace, WorkspaceCommand};
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;

#[tokio::test]
#[ignore] // Skip in CI since it requires gopls and actual LLM
async fn test_generate_output() {
    // Create config
    let config = Config {
        url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
        model: "test-model".to_string(),
        api_key: Some("test-key".to_string()),
        log_level: None,
        openrouter: None,
    };

    // Test with simple file
    let file_path = Path::new("examples/simple_test.go");

    // Create workspace and generate
    let workspace_tx = Workspace::spawn(PathBuf::from("examples"), config)
        .await
        .unwrap();

    let (tx, rx) = oneshot::channel();
    workspace_tx
        .send(WorkspaceCommand::GenerateFile {
            file_path: file_path.to_path_buf(),
            response: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap().unwrap();

    // Check that line count is reasonable (not too many extra lines)
    let line_count = result.lines().count();

    println!(
        "Generated output ({} lines):\n---\n{}\n---",
        line_count, result
    );

    // Check that mantra comments are preserved
    assert!(result.contains("// mantra:"), "Missing mantra comments");

    // Check that implementations were added
    assert!(
        result.contains("return a + b"),
        "Missing Add implementation"
    );
    assert!(
        result.contains("return n%2 == 0"),
        "Missing IsEven implementation"
    );
    assert!(
        result.contains("return strings.ToUpper(s)"),
        "Missing ToUpper implementation"
    );

    // Original file has 16 lines, allow some extra
    assert!(line_count <= 25, "Too many lines in output: {}", line_count);

    // Shutdown workspace
    workspace_tx.send(WorkspaceCommand::Shutdown).await.unwrap();
}
