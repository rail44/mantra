# スコープID実装計画書

## 概要
LLMがinspectツール経由で型の内部構造を段階的に調査できるようにするため、型参照にスコープIDを付与する。

## 実装状況

### ✅ 実装済み
- `TypeReference`構造体（src/parser/target.rs）
- `InspectTool`（src/llm/tools.rs）
- `SymbolInspector`（src/inspector/mod.rs）
- `Document::collect_type_references`（src/document/mod.rs）
- ropeを使った型名テキスト取得
- 階層的参照サポート（`SimpleCache.Name`形式）

### ⏳ 未実装
- inspectツールのsymbol機能（型定義全体ではなく特定フィールド/メソッドの抽出）