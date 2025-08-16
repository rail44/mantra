# SymbolInspector設計ドキュメント

## 概要

SymbolInspectorは、LLMがコード生成時にシンボル情報を段階的に取得するための仕組みです。
アクターモデルベースのアーキテクチャにより、並行性と拡張性を確保しています。

## 背景

### 現状
- `TargetGenerator`でLSP hoverを直接使用（TODOコメント有り）
- 全型情報を事前にLLMに渡している（トークン消費大）

### 目標
- 必要な情報のみを段階的に取得
- シンボル情報取得ロジックの抽象化
- 将来の多言語対応への準備

## アーキテクチャ

### アクターモデル設計

システムは独立したアクターの協調により動作します：

```
┌─────────────────┐
│  LLM Generator  │
└────────┬────────┘
         │ inspect(scope, symbol)
         ▼
┌─────────────────┐
│SymbolInspector  │
└────────┬────────┘
         │ 
         ▼
┌─────────────────┐     ┌──────────────┐
│   Workspace     │────▶│ Document     │ (per file)
│   (Registry)    │     │   Actor      │
└────────┬────────┘     └──────────────┘
         │
         ├──▶ LSP Client
         └──▶ LLM Client
```

### 各コンポーネントの責務

#### **Workspace**
- アクターのレジストリ/ルーター
- Documentアクターのライフサイクル管理
- LSP/LLMクライアントへのアクセス提供

#### **Document Actor**
- 単一ファイルの管理
- tree-sitter解析状態の保持
- シンボル位置特定
- テキスト範囲の抽出

#### **SymbolInspector**
- スコープID管理
- 検査ロジックのオーケストレーション
- 複数アクターを協調させて結果生成

### 処理フロー

```
1. 初期化
   - tree-sitterでmantraターゲット抽出
   - スコープID採番

2. LLMツール呼び出し: inspect(scope, symbol)
   
3. SymbolInspector処理
   a. Workspaceから該当Documentアクター取得
   b. Documentにシンボル位置問い合わせ
   c. LSP経由でdefinition取得
   d. 定義ファイルのDocumentアクター取得
   e. 定義コード抽出
   
4. 結果返却
   - 新スコープID + コード
```

## 主要インターフェース

### Workspace

```rust
impl Workspace {
    /// Documentアクターを取得（なければ作成）
    pub async fn get_document(&mut self, uri: &str) -> Result<Sender<DocumentCommand>>
    
    /// LSPクライアントを取得
    pub fn get_lsp_client(&self) -> &LspClient
    
    /// LLMクライアントを取得
    pub fn get_llm_client(&self) -> &LLMClient
}
```

### Document Actor

```rust
enum DocumentCommand {
    FindSymbol {
        range: Range,
        symbol: String,
        response: oneshot::Sender<Result<Position>>,
    },
    GetText {
        range: Range,
        response: oneshot::Sender<Result<String>>,
    },
}
```

### SymbolInspector

```rust
pub struct SymbolInspector {
    scopes: HashMap<String, Location>,
}

impl SymbolInspector {
    pub async fn inspect(
        self,
        scope_id: String,
        symbol: String,
        workspace: &mut Workspace,
    ) -> Result<InspectionResult>
}
```

### InspectionResult

```rust
pub struct InspectionResult {
    pub scope_id: String,  // 新しいスコープID
    pub code: String,      // 定義のコード
}
```

## 実装方針

### Phase 1: 基盤実装
- SymbolInspectorトレイト定義
- 既存コードとの統合方法検討
- スコープID管理の基本実装

### Phase 2: LSP統合
- definitionベースの実装
- 定義位置のソース読み込み
- エラーハンドリング

### Phase 3: LLMツール化
- inspectツールの実装
- プロンプト生成との統合

## 検討事項

### スコープID形式
- 位置ベースが最も確実
- 例: `"file.go:6:5"`

**→ 実装時に詳細決定**

### tree-sitterとLSPの役割分担
- tree-sitter: 初期解析、シンボル位置特定
- LSP definition: 定義位置取得
- ファイルI/O: 定義コード読み込み

**→ 詳細は実装時に調整**

### エラー処理
- LSP未起動時の動作
- シンボルが見つからない場合
- 外部パッケージの扱い

**→ 実装しながら方針決定**

## 今後の拡張可能性

- 多言語対応（TypeScript、Python、Rust）
- 必要に応じてhover等の追加情報
- リファクタリング支援

## 注意事項

- 実装の詳細は開発中に適宜調整
- 既存コードへの影響を最小限に
- パフォーマンスを意識した設計