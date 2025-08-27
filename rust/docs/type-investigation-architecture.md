# Type Investigation Architecture

## 目的
LLMがtool call経由で型の内部構造を段階的に調査できるようにする。

## 基本方針
1. **Workspace中心**: Workspaceが全ドキュメントを管理
2. **LSP + tree-sitter併用**: 型位置はLSP、構造解析はtree-sitter  
3. **スコープID**: コード片を参照可能にする識別子を付与

## アーキテクチャ概要
```
SymbolInspector (新規)
├── workspace: &mut Workspace を受け取る
├── inspect_symbol() - シンボル調査のメインロジック
│   ├── LSP definition/typeDefinition でシンボル位置取得
│   ├── tree-sitter でシンボル種別判定
│   └── 適切な範囲のコード抽出
└── extract_with_scope() - スコープID付きコード抽出

Workspace
├── documents: HashMap<URI, DocumentService>
├── open_document(uri) -> DocumentService
│   └── 既存なら返却、なければ新規作成（冪等）
└── ドキュメント管理に専念

DocumentService
└── 単一ドキュメントの操作に専念
```

## 必要な実装項目

### LSP拡張
- textDocument/definitionリクエストの実装（既存？要確認）
- GotoDefinitionResponseのパース処理
  - Location/LocationLink両形式の対応
  - targetRangeの活用（LocationLinkの場合）
- （将来）textDocument/typeDefinitionの追加検討

### Workspace拡張
- open_documentメソッド
  - 既存ドキュメントがあれば返却
  - なければ新規作成（ファイル読み込み、LSP通知、DocumentService作成）
  - 冪等性を保証
- ドキュメントのライフサイクル管理

### SymbolInspector実装（新規）
- シンボル調査のメインコンポーネント
- Workspaceを利用して複数ドキュメントを扱う
- inspect_symbolメソッド
  - definitionでシンボル定義位置を取得
  - tree-sitterでシンボル種別を判定（type/function/variable等）
  - 種別に応じた適切な範囲を抽出
- シンボル種別の判定ロジック
  - tree-sitterノードタイプから判定

### スコープ管理
- スコープIDの生成ロジック
- スコープ情報の保持構造
- 親子関係の管理

### コード抽出
- tree-sitterノードからのソース抽出
- スコープIDのアノテーション付与
- 構造（型、フィールド、メソッド）の識別

### LLMツール
- inspect_typeツールの定義
- リクエスト/レスポンス型
- DocumentService/Workspaceとの連携

### Generation Task
- tool callループの実装
- ツール実行とレスポンス処理
- コンテキスト管理

## 実装順序

### Phase 1: 基盤整備
1. LSP Client拡張
   - definitionメソッドの実装/確認
   - GotoDefinitionResponseのパース
2. Workspace拡張
   - open_documentメソッドの実装

### Phase 2: シンボル調査
3. SymbolInspector基本実装
   - definitionベースのシンボル調査
   - tree-sitterによる種別判定
4. スコープ管理とコード抽出
   - スコープID生成
   - 範囲抽出ロジック

### Phase 3: LLM統合
5. LLMツール実装
6. Generation Task拡張

## 未決定事項（実装時に判断）
- スコープIDの形式
- コード抽出の粒度
- 外部ファイルのURI解決方法
- キャッシュ戦略
- エラーハンドリング方針