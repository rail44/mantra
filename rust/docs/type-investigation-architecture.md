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