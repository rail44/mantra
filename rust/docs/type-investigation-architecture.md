# Type Investigation Architecture

## 目的
LLMがtool call経由で型の内部構造を段階的に調査できるようにする。

## 基本方針
1. **Workspace中心**: Workspaceが全ドキュメントを管理
2. **LSP + tree-sitter併用**: 型位置はLSP、構造解析はtree-sitter  
3. **スコープID**: コード片を参照可能にする識別子を付与

## アーキテクチャ概要
```
SymbolInspector ✅
├── workspace: &WorkspaceService
├── inspect_by_path() ✅ - AST path経由でシンボル調査
│   ├── LSP definition でシンボル位置取得 ✅
│   └── tree-sitter で完全な定義抽出 ✅
└── extract_with_scope() - 未実装

WorkspaceService ✅
├── workspace: Arc<RwLock<Workspace>>
├── open_document(uri) ✅ - 冪等なドキュメント取得
└── ドキュメント管理

DocumentService ✅
├── get_definition_at_path() ✅
├── get_full_definition_at() ✅
└── LSP/tree-sitter統合
```

## 必要な実装項目

### LSP拡張 ✅
- textDocument/definitionリクエスト ✅
- GotoDefinitionResponseのパース ✅
  - Location/LocationLink両形式の対応 ✅
- textDocument/typeDefinition - 未実装

### Workspace拡張 ✅  
- open_documentメソッド ✅
- 冪等性を保証 ✅
- ドキュメントのライフサイクル管理 ✅

### SymbolInspector実装 ✅
- inspect_by_pathメソッド ✅
- 複数ドキュメント対応 ✅
- tree-sitter種別判定 ✅

### スコープ管理 - 未実装
- スコープID生成
- ScopedCode構造体活用

### コード抽出 ✅
- tree-sitterノードからのソース抽出 ✅
- 型/関数/定数の識別 ✅

### LLMツール - 未実装
- tool callインターフェース

### Generation Task ✅
- SymbolInspector統合 ✅

## 実装順序

### Phase 1: 基盤整備 ✅
1. LSP Client拡張 ✅
2. Workspace拡張 ✅

### Phase 2: シンボル調査 🔄
3. SymbolInspector基本実装 ✅
4. スコープ管理とコード抽出 🔄
   - 範囲抽出ロジック ✅
   - スコープID生成 ❌

### Phase 3: LLM統合 🔄
5. LLMツール実装 ❌
6. Generation Task拡張 ✅

## 実装済み項目
- LSP definition request/response
- WorkspaceService document管理
- SymbolInspector basic functionality
- tree-sitter definition extraction

## 未実装項目
- スコープIDシステム
- LLM tool callインターフェース
- textDocument/typeDefinition