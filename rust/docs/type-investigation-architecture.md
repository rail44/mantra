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

### ✅ 完了済み
- **inspectツールのsymbol機能**：symbolパラメータで特定フィールド/メソッドの型情報を取得
  - 実装完了：`inspect(scope="SimpleCache", symbol="items")`で特定フィールドの定義のみ返す
  - SymbolInspector::inspect_symbol()メソッド追加済み
  - ast_utils::find_symbol_in_node()でGo構造体/インターフェースのシンボル検索対応
  - InspectToolでsymbolパラメータ必須化、inspect_symbol()呼び出し対応
  - テスト完備（test_symbol_inspector_inspect_symbol, test_inspect_tool_with_symbol）

### ⏳ 未実装
- **項目4: レスポンス形式の改善**
  - 現状：プレーンテキスト応答のみ
  - 期待動作：新しいscope_idとcontentを含む構造化された応答
  - 目的：LLMが連鎖的に調査できる形式の提供
  
- **項目5: 動的scope管理**
  - 現状：静的なtype_scope_mappingのみ
  - 期待動作：inspectの結果を新しいscopeとして登録
  - 目的：階層的な型調査（例：SimpleCache → items → cacheItem → value）を可能にする