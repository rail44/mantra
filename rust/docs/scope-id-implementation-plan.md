# スコープID実装計画書

## 概要
LLMがinspectツール経由で型の内部構造を段階的に調査できるようにするため、型参照にスコープIDを付与する。

## 現在の問題点
- `Target.type_references`は`Vec<Vec<PathSegment>>`で、AST pathのみを保持
- 型名の情報が失われており、LLMには`type_0`, `type_1`のような意味のないキーで渡されている
- inspectツールは未実装（TODOのまま）

## 合意済みの設計

### 1. TypeReference構造体
```rust
// src/parser/target.rs
pub struct TypeReference {
    pub path: Vec<PathSegment>,  // AST path
    pub scope_id: String,         // 型名
}
```

### 2. スコープIDの方針
- スコープIDは型名そのもの（`"SimpleCache"`, `"string"`, `"Duration"`など）
- 同じ型が複数出現しても区別しない（型の定義は同じため）
- AST pathから対応する型名テキストをropeを使って取得

### 3. inspectツールでの使用
- 初期コンテキスト：型名をスコープIDとして提供
- LLM：`inspect(scope="SimpleCache", symbol="Name")`のように呼び出し
- システム：後続のinspectで`"SimpleCache.Name"`のような階層的な参照が可能

### 4. 検証済みの知見

tree-sitter CLIで以下を確認済み：
- `pointer_type`ノード全体から`*User`が取得できる
- `slice_type`ノード全体から`[]string`が取得できる  
- `type_identifier`ノードだけでは`User`や`string`のみで、修飾（`*`や`[]`）が欠落する

実装時の注意：
- 型ノード全体（`pointer_type`, `slice_type`等）のテキストをropeから取得する
- `type_identifier`ノードだけを見ると不完全な型情報になる

### 5. 実装箇所
- `Document::collect_type_references`：型参照を収集
- `Target::type_references`：`Vec<TypeReference>`に変更
- `generation/task.rs`：scope_idをキーとして使用
- `execute_inspect_tool`：実際のinspect処理を実装