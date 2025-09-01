# Type Investigation Architecture

## 目的
LLMがtool call経由で型の内部構造を段階的に調査できるようにする。

## アーキテクチャ概要
```
SymbolInspector ✅
├── workspace: &WorkspaceService
└── inspect_by_path() ✅

WorkspaceService ✅
├── workspace: Arc<RwLock<Workspace>>
└── open_document(uri) ✅

DocumentService ✅
├── get_definition_at_path() ✅
├── get_full_definition_at() ✅
└── LSP/tree-sitter統合

InspectTool ✅
├── execute() ✅
└── 階層的参照サポート
```

## 実装状況

### ✅ 実装済み
- LSP definition request/response
- WorkspaceService document管理
- SymbolInspector（inspect_by_path）
- DocumentService（get_definition_at_path, get_full_definition_at）
- InspectTool（execute, 階層的参照）
- tree-sitter definition extraction
- Document::collect_type_references
- スコープID自動生成（ropeから型名取得）
- type_scope_mapping生成

### ⏳ 未実装
- **inspectツールのsymbol機能**：現在symbolパラメータを受け取るが、実際には型定義全体を返すだけ
  - 期待動作：`inspect(scope="SimpleCache", symbol="items")`で特定フィールドの型情報のみ返す
  - 現状：symbolの有無に関わらず型定義全体を返す（プレースホルダー実装）

## 実装計画

### 1. SymbolInspectorに新メソッド追加
- `inspect_symbol()`メソッドを追加
- scopeで指定されたコード片内の特定symbolの定義を調査
- 定義先ドキュメント内でsymbolを検索し、LSP definitionを実行

### 2. ast_utilsにヘルパー関数追加
- コード片内でのsymbol検索機能
- 構造体/インターフェース内でフィールド/メソッドを検索

### 3. InspectToolの修正
- symbolパラメータを必須に変更
- symbolがある場合は新しい`inspect_symbol()`を呼ぶ
- 動的scope管理の追加（新しいscopeを登録）

### 4. レスポンス形式の改善
- 新しいscope_idとcontentを含む構造化された応答
- LLMが連鎖的に調査できる形式

### 5. 動的scope管理
- inspectの結果を新しいscopeとして登録
- 階層的な型調査を可能にする