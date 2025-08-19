# CrdtEditor TextEdit Integration Plan

## 概要

LSP formatting corruption問題を解決するため、CrdtEditorをTextEdit中心の設計にリファクタリングし、TransactionalCrdtEditorを削除してアーキテクチャを簡素化する。

## 現在の問題

### 1. フォーマット結果の破損
- 複数編集の並行実行時、古いバージョンに対するフォーマット結果が適用される
- document_versionとcolaのReplicaが分離している

### 2. 位置変換の重複
```rust
LSP位置 → バイト位置 → TextEdit(LSP位置) → didChange
```
無駄な変換チェーンが発生

### 3. 責務の混在
- TransactionalCrdtEditor: 大部分が委譲レイヤー
- DocumentActor: 複雑な位置計算を実行
- CrdtEditor: TextEdit生成の責務を持つ

## 目標アーキテクチャ

### TextEdit中心のデータフロー
```rust
LSP → TextEdit → CrdtEditor → didChange(同じTextEdit)
```

### 責務の再分離
```rust
CrdtEditor {
    // 統合された責務
    replica: Replica,           // cola CRDT
    rope: Rope,                 // crop text storage  
    document_version: i32,      // LSP version
    
    // 新しいインターフェース
    apply_text_edit(edit: &TextEdit) -> Result<()>
    increment_version() -> i32
}

DocumentActor {
    // 簡素化された責務
    editor: CrdtEditor,         // 直接使用
    
    // LSP調整のみ
}
```

## 実装手順

### Phase 1: CrdtEditorの拡張
```rust
impl CrdtEditor {
    document_version: i32,
    
    pub fn apply_text_edit(&mut self, edit: &TextEdit) -> Result<()> {
        let start_byte = self.lsp_position_to_byte(edit.range.start);
        let end_byte = self.lsp_position_to_byte(edit.range.end);
        
        if start_byte < end_byte {
            self.delete(start_byte, end_byte)?;
        }
        self.insert(start_byte, &edit.new_text)?;
        Ok(())
    }
    
    pub fn increment_version(&mut self) -> i32 {
        self.document_version += 1;
        self.document_version
    }
}
```

### Phase 2: TransactionalCrdtEditor削除
- 全ての利用箇所をCrdtEditorに置き換え
- transaction.rs ファイルを削除
- DocumentActorのeditorフィールドを変更

### Phase 3: DocumentActor簡素化
```rust
// Before
fn apply_edit_internal(&mut self, edit: EditEvent) -> Result<lsp_types::TextEdit>

// After  
fn apply_edit_internal(&mut self, edit: EditEvent) -> Result<()> {
    let text_edit = TextEdit {
        range: Range { start, end },
        new_text: content,
    };
    self.editor.apply_text_edit(&text_edit)?;
    
    let version = self.editor.increment_version();
    self.send_did_change(text_edit, version);
}
```

### Phase 4: フォーマット処理の修正
```rust
fn apply_formatting_edits(&mut self, edits: Vec<TextEdit>) -> Result<()> {
    for edit in edits {
        self.editor.apply_text_edit(&edit)?;
        
        let version = self.editor.increment_version();
        self.send_did_change_for_format(edit, version).await?;
    }
}
```

## 期待される効果

1. **フォーマット破損の解決**: バージョン整合性の確保
2. **パフォーマンス向上**: 位置変換の最適化
3. **コード簡素化**: 中間レイヤーの削除
4. **保守性向上**: 明確な責務分離

## リスク評価

- **低リスク**: 既存のCRDT機能は保持される
- **テスト必須**: 各段階での動作確認
- **後方互換性**: 外部APIに影響なし

## 成功指標

1. フォーマット出力の破損解消
2. コードの行数削減（TransactionalCrdtEditor削除）
3. パフォーマンステストの通過
4. 既存テストの全通過