use anyhow::{Context, Result};
use lsp_types::{
    DocumentFormattingParams, DocumentRangeFormattingParams, FormattingOptions, Position, Range,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextEdit,
    VersionedTextDocumentIdentifier,
};
use tracing::debug;

use super::client::Client as LspClient;
use crate::editor::TransactionalCrdtEditor;

/// LSP整形マネージャー
/// CRDTエディタの変更をLSPに通知し、整形結果を適用する
pub struct LspFormattingManager<'a> {
    lsp_client: &'a LspClient,
    editor: &'a mut TransactionalCrdtEditor,
    document_uri: String,
    document_version: i32,
}

impl<'a> LspFormattingManager<'a> {
    /// 新しい整形マネージャーを作成
    pub fn new(
        lsp_client: &'a LspClient,
        editor: &'a mut TransactionalCrdtEditor,
        document_uri: String,
        document_version: i32,
    ) -> Self {
        Self {
            lsp_client,
            editor,
            document_uri,
            document_version,
        }
    }

    /// 範囲を指定して整形を実行
    pub async fn format_range(&mut self, start_line: u32, end_line: u32) -> Result<String> {
        debug!(
            "Formatting range: lines {} to {} in {}",
            start_line, end_line, self.document_uri
        );

        // 1. まず現在のドキュメント状態をLSPに通知（didChange）
        self.send_did_change().await?;

        // 2. 範囲整形をリクエスト
        let range = Range {
            start: Position {
                line: start_line,
                character: 0,
            },
            end: Position {
                line: end_line,
                character: u32::MAX, // 行末まで
            },
        };

        let params = DocumentRangeFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: self.document_uri.parse()?,
            },
            range,
            options: self.default_formatting_options(),
            work_done_progress_params: Default::default(),
        };

        let edits = self
            .lsp_client
            .range_formatting(params.text_document, params.range, params.options)
            .await
            .context("Failed to request range formatting")?;

        // 3. 編集をCRDTエディタに適用
        match edits {
            Some(edits) if !edits.is_empty() => self.apply_text_edits(edits).await,
            _ => {
                debug!("No formatting changes from LSP range formatting");
                Ok(self.editor.get_text().to_string())
            }
        }
    }

    /// ドキュメント全体を整形
    pub async fn format_document(&mut self) -> Result<String> {
        debug!("Formatting entire document: {}", self.document_uri);

        // 1. 現在のドキュメント状態をLSPに通知
        self.send_did_change().await?;

        // 2. ドキュメント整形をリクエスト
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: self.document_uri.parse()?,
            },
            options: self.default_formatting_options(),
            work_done_progress_params: Default::default(),
        };

        let edits = self
            .lsp_client
            .format_document(params.text_document, params.options)
            .await?;

        // 3. 編集をCRDTエディタに適用
        match edits {
            Some(edits) if !edits.is_empty() => self.apply_text_edits(edits).await,
            _ => {
                debug!("No formatting changes from LSP");
                Ok(self.editor.get_text().to_string())
            }
        }
    }

    /// 生成されたコードを整形（関数本体のみ）
    pub async fn format_generated_function_body(
        &mut self,
        function_start_line: u32,
        function_end_line: u32,
        generated_body: &str,
    ) -> Result<String> {
        debug!(
            "Formatting generated function body: lines {} to {}",
            function_start_line, function_end_line
        );

        // トランザクションを開始
        let transaction_id = format!("format_{}_{}", function_start_line, function_end_line);
        self.editor.begin_transaction(
            transaction_id.clone(),
            format!(
                "Format generated function at lines {}-{}",
                function_start_line, function_end_line
            ),
        )?;

        // 1. 生成されたコードを一時的に適用
        let start_byte = self
            .editor
            .line_col_to_byte(function_start_line as usize, 0);
        let end_byte = self
            .editor
            .line_col_to_byte(function_end_line as usize + 1, 0);

        self.editor.replace(start_byte, end_byte, generated_body)?;

        // 2. LSPに変更を通知
        self.send_did_change().await?;

        // 3. 範囲整形を実行
        let formatted = self
            .format_range(function_start_line, function_end_line)
            .await?;

        // 4. トランザクションをコミット
        self.editor.commit_transaction(&transaction_id)?;

        Ok(formatted)
    }

    /// LSPにdidChange通知を送信
    async fn send_did_change(&mut self) -> Result<()> {
        self.document_version += 1;

        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: self.document_uri.parse()?,
                version: self.document_version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None, // 全体を送信（簡略化）
                range_length: None,
                text: self.editor.get_text().to_string(),
            }],
        };

        self.lsp_client.did_change(params).await?;

        debug!(
            "Sent didChange notification for version {}",
            self.document_version
        );
        Ok(())
    }

    /// LSPからのTextEditをCRDTエディタに適用
    async fn apply_text_edits(&mut self, edits: Vec<TextEdit>) -> Result<String> {
        if edits.is_empty() {
            return Ok(self.editor.get_text().to_string());
        }

        debug!("Applying {} text edits from LSP", edits.len());

        // TextEditは逆順（後ろから）に適用する必要がある
        let mut sorted_edits = edits;
        sorted_edits.sort_by(|a, b| {
            b.range
                .start
                .line
                .cmp(&a.range.start.line)
                .then_with(|| b.range.start.character.cmp(&a.range.start.character))
        });

        let transaction_id = "lsp_format_apply";
        self.editor.begin_transaction(
            transaction_id.to_string(),
            "Apply LSP formatting".to_string(),
        )?;

        for edit in sorted_edits {
            let start_byte = self.position_to_byte(&edit.range.start);
            let end_byte = self.position_to_byte(&edit.range.end);

            self.editor.replace(start_byte, end_byte, &edit.new_text)?;
        }

        self.editor.commit_transaction(transaction_id)?;

        Ok(self.editor.get_text().to_string())
    }

    /// LSP Positionをバイトオフセットに変換
    fn position_to_byte(&self, position: &Position) -> usize {
        self.editor
            .line_col_to_byte(position.line as usize, position.character as usize)
    }

    /// デフォルトの整形オプション
    fn default_formatting_options(&self) -> FormattingOptions {
        FormattingOptions {
            tab_size: 4,
            insert_spaces: false, // Goはタブを使用
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            ..Default::default()
        }
    }
}

/// 整形結果
#[derive(Debug)]
pub struct FormattingResult {
    /// 整形後のテキスト
    pub formatted_text: String,
    /// 適用された編集の数
    pub edit_count: usize,
    /// 変更があったかどうか
    pub has_changes: bool,
}

impl FormattingResult {
    /// 変更なしの結果を作成
    pub fn no_changes(text: String) -> Self {
        Self {
            formatted_text: text,
            edit_count: 0,
            has_changes: false,
        }
    }

    /// 変更ありの結果を作成
    pub fn with_changes(text: String, edit_count: usize) -> Self {
        Self {
            formatted_text: text,
            edit_count,
            has_changes: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatting_result() {
        let result = FormattingResult::no_changes("test".to_string());
        assert!(!result.has_changes);
        assert_eq!(result.edit_count, 0);

        let result = FormattingResult::with_changes("formatted".to_string(), 3);
        assert!(result.has_changes);
        assert_eq!(result.edit_count, 3);
    }
}
