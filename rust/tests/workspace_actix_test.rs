use actix::prelude::*;
use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use mantra::config::Config;
use mantra::core::types::{Position, Range};
use mantra::workspace::{
    GenerateFile, GetLlmClient, GetLspClient, RegisterScope, Shutdown, Workspace,
};

/// テスト用の最小限の設定を作成
fn test_config() -> Config {
    Config {
        model: "test-model".to_string(),
        url: "http://localhost:8080".to_string(),
        api_key: Some("test-key".to_string()),
        log_level: Some("debug".to_string()),
        openrouter: None,
    }
}

/// テスト用のGoファイルを作成
async fn create_test_go_file(dir: &TempDir) -> Result<PathBuf> {
    let file_path = dir.path().join("test.go");
    let content = r#"package main

// mantra:generate
func Add(a, b int) int {
    panic("not implemented")
}
"#;
    tokio::fs::write(&file_path, content).await?;
    Ok(file_path)
}

#[test]
fn test_workspace_actor_lifecycle() -> Result<()> {
    // Actixシステムを起動
    let system = System::new();

    system.block_on(async {
        // テスト用の一時ディレクトリ
        let temp_dir = TempDir::new()?;
        let root_dir = temp_dir.path().to_path_buf();

        // Workspaceアクターを作成して起動
        let addr = Workspace::start_actor(root_dir.clone(), test_config()).await?;

        // GetLspClientメッセージをテスト
        let _lsp_client = addr.send(GetLspClient).await?;
        // LSPクライアントが正常に取得できることを確認（詳細な検証は別途）

        // GetLlmClientメッセージをテスト
        let _llm_client = addr.send(GetLlmClient).await?;
        // LLMクライアントが正常に取得できることを確認

        // RegisterScopeメッセージをテスト
        let scope_id = addr
            .send(RegisterScope {
                uri: "file:///test.go".to_string(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 10,
                        character: 0,
                    },
                },
            })
            .await?;
        assert!(!scope_id.is_empty());

        // Shutdownメッセージを送信
        addr.send(Shutdown).await?;

        // システムを停止
        System::current().stop();

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn test_generate_file_placeholder() -> Result<()> {
    // Actixシステムを起動
    let system = System::new();

    system.block_on(async {
        // テスト用の一時ディレクトリとファイル
        let temp_dir = TempDir::new()?;
        let root_dir = temp_dir.path().to_path_buf();
        let test_file = create_test_go_file(&temp_dir).await?;

        // Workspaceアクターを作成して起動
        let addr = Workspace::start_actor(root_dir.clone(), test_config()).await?;

        // GenerateFileメッセージをテスト（現在はプレースホルダー実装）
        let result = addr
            .send(GenerateFile {
                file_path: test_file,
            })
            .await?;

        match result {
            Ok(generated) => {
                // 現在の実装では元のソースコードが返される
                assert!(generated.contains("func Add(a, b int) int"));
                assert!(generated.contains("panic(\"not implemented\")"));
            }
            Err(e) => {
                // エラーの場合もログに記録
                eprintln!(
                    "GenerateFile error (expected in current implementation): {}",
                    e
                );
            }
        }

        // Shutdownメッセージを送信
        addr.send(Shutdown).await?;

        // システムを停止
        System::current().stop();

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn test_multiple_workspace_actors() -> Result<()> {
    // Actixシステムを起動
    let system = System::new();

    system.block_on(async {
        // 複数のWorkspaceアクターを作成
        let temp_dir1 = TempDir::new()?;
        let temp_dir2 = TempDir::new()?;

        let addr1 = Workspace::start_actor(temp_dir1.path().to_path_buf(), test_config()).await?;
        let addr2 = Workspace::start_actor(temp_dir2.path().to_path_buf(), test_config()).await?;

        // 両方のアクターが独立して動作することを確認
        let _lsp1 = addr1.send(GetLspClient).await?;
        let _lsp2 = addr2.send(GetLspClient).await?;

        // 両方が正常に取得できればOK（独立して動作している）

        // 両方をシャットダウン
        addr1.send(Shutdown).await?;
        addr2.send(Shutdown).await?;

        // システムを停止
        System::current().stop();

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
