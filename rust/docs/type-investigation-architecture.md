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

- **項目5: 動的scope管理**
  - 実装完了：inspect結果を新しいscopeとして自動登録
  - type_scope_mappingをファイル情報付きに拡張（document_uri + PathSegment）
  - ScopedCodeに完全な位置情報を含めるよう拡張
  - DocumentService::get_full_definition_atでPathSegment自動生成
  - 階層的調査（例：SimpleCache → SimpleCache.items → 次の階層）が可能
  - 別ファイルの型定義にも対応
  - テスト完備（test_dynamic_scope_management）

### ⏳ 未実装
- **項目4: レスポンス形式の改善**
  - 現状：プレーンテキスト応答のみ
  - 期待動作：新しいscope_idとcontentを含む構造化された応答
  - 目的：LLMが連鎖的に調査できる形式の提供

## TODO: 今後の改善項目

### LLMインタラクションの改善
- **プロンプト内容の最適化**
  - inspectツールの使用方法をより明確に説明
  - 階層的調査のベストプラクティスを提示
  - 型調査の効果的な進め方をガイド

- **ツール結果の表示改善**
  - 現状：プレーンテキストで型定義を表示
  - 改善案：構造化された形式（JSON/YAML）での出力
  - フィールド/メソッドの一覧表示
  - 利用可能な次のscope候補の提示

### 機能拡張
- **dot記法でのsymbol指定サポート**
  - 現状：`inspect(scope="SimpleCache", symbol="items")`のみ
  - 改善案：`inspect(scope="SimpleCache", symbol="items.value")`を許可
  - ネストしたフィールドへの直接アクセス
  - 中間スコープの自動生成

- **型調査の効率化**
  - よく使われる型（map, slice, channel等）の専用ハンドリング
  - 外部パッケージ型の基本情報提供
  - 型階層の可視化機能