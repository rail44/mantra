# Mantra Development Plan

## 概要
MantraプロジェクトをGo専用から多言語対応のAIコード生成ツールへと進化させるための開発計画。

## 実装計画と優先順位

### フェーズ1: 基盤整備 (優先度: 高)

#### 1-1. 言語抽象レイヤーの導入
**目的**: Go以外の言語をサポートするための基盤を作る

**実装内容**:
- `trait Language`の定義
  - Parser trait (tree-sitterベース)
  - Target抽出のインターフェース
  - 言語固有の設定（コメント形式、関数定義パターン等）
- `GoLanguage`の実装（既存コードのリファクタリング）
- `LanguageDetector`の実装（ファイル拡張子ベース）

**見積もり工数**: 1週間

#### 1-2. TypeResolver抽象化
**目的**: 型情報取得を言語非依存にし、LLMツールとして提供可能にする

**実装内容**:
- `trait TypeResolver`の定義
  - `resolve_function_types()`: 関数の引数・戻り値の型情報取得
  - `resolve_symbol()`: シンボルの型情報取得
- `LspTypeResolver`の実装（既存コードのリファクタリング）
- `MockTypeResolver`の実装（テスト用）

**見積もり工数**: 3-4日

### フェーズ2: LLMツール化 (優先度: 高)

#### 2-1. TypeResolver as Tool
**目的**: LLMが必要に応じて型情報を取得できるようにする

**実装内容**:
- LLM Tool定義の追加
  - `get_type_info`: 指定されたシンボルの型情報を取得
  - `list_available_types`: 利用可能な型のリスト取得
- プロンプト生成の変更
  - 事前に全型情報を渡すのではなく、ツール呼び出しを許可
- Tool実行フレームワークの実装

**見積もり工数**: 1週間

### フェーズ3: 言語拡張 (優先度: 中)

#### 3-1. TypeScript/JavaScript サポート
**実装内容**:
- `TypeScriptLanguage`の実装
- tree-sitter-typescriptの統合
- TypeScript Language Serverクライアントの実装

**見積もり工数**: 1週間

#### 3-2. Python サポート
**実装内容**:
- `PythonLanguage`の実装
- tree-sitter-pythonの統合
- Pylspクライアントの実装

**見積もり工数**: 1週間

#### 3-3. Rust サポート
**実装内容**:
- `RustLanguage`の実装
- tree-sitter-rustの統合
- rust-analyzerクライアントの実装

**見積もり工数**: 2週間

### フェーズ4: 高度な機能 (優先度: 低)

#### 4-1. Context-aware型推論
- プロジェクト全体の型情報を活用
- import/use文の解析と型の自動解決

#### 4-2. LSPサーバーモード
- mantra自体をLSPサーバーとして動作
- リアルタイムコード生成

## 優先順位の判断理由

### 高優先度の理由

**言語抽象レイヤー**:
- 依存関係のボトルネック：他のすべての機能がこれに依存
- 既存コードへの影響が大：早期に実装しないと、後の変更コストが増大
- アーキテクチャの根幹：設計を間違えると全体に影響

**TypeResolver抽象化**:
- 現在のTODOコメント：すでに技術的負債として認識されている
- テスタビリティ向上：モック実装により、LLM/LSPなしでテスト可能
- Tool化の前提条件：これがないとTypeResolver as Toolが実装できない

**TypeResolver as Tool**:
- 即座の価値提供：トークン使用量の大幅削減（全型情報 → 必要な型のみ）
- LLMコスト削減：API呼び出しコストの直接的な削減
- 精度向上の可能性：LLMが必要な情報を動的に取得できる

### 中優先度の理由

**多言語サポート**:
- 市場需要はあるが、まず基盤を固めることが重要
- 各言語の実装は独立しているため、順次追加可能
- 言語選定は市場調査後に決定

### 低優先度の理由

**高度な機能**:
- 実装の複雑さに対して、即座の価値提供が限定的
- 現状の機能でも基本的なニーズは満たせる
- 基盤が整ってから実装する方が効率的

## 実装の詳細設計

### Language trait設計
```rust
trait Language {
    fn name(&self) -> &str;
    fn extensions(&self) -> &[&str];
    fn parser(&self) -> Box<dyn LanguageParser>;
    fn type_resolver(&self, lsp_client: Option<LspClient>) -> Box<dyn TypeResolver>;
    fn extract_targets(&self, source: &str, tree: &Tree) -> Result<Vec<Target>>;
}

trait LanguageParser {
    fn parse(&mut self, source: &str) -> Result<Tree>;
    fn parse_incremental(&mut self, source: &str, old_tree: Option<&Tree>) -> Result<Tree>;
}
```

### TypeResolver trait設計
```rust
trait TypeResolver {
    async fn resolve_function_types(&self, target: &Target, source: &str) -> Result<TypeInfo>;
    async fn resolve_symbol(&self, symbol: &str, context: &Context) -> Result<SymbolType>;
}

struct TypeInfo {
    parameters: Vec<TypeDefinition>,
    returns: Vec<TypeDefinition>,
    generics: Option<Vec<GenericConstraint>>,
}
```

### TypeResolver Tool設計
```rust
#[derive(Serialize, Deserialize)]
struct TypeInfoTool {
    name: String,
    description: String,
}

impl Tool for TypeInfoTool {
    async fn execute(&self, params: Value) -> Result<Value> {
        // シンボル名から型情報を取得
        // LLMが必要な型情報だけを動的に要求
    }
}
```

## コスト対効果の分析

| 機能 | 実装コスト | 期待効果 | ROI | 優先度 |
|------|-----------|----------|-----|---------|
| 言語抽象レイヤー | 高（1週間） | 基盤 | 必須 | 最高 |
| TypeResolver抽象化 | 低（3-4日） | 高（テスト性） | 高 | 高 |
| TypeResolver as Tool | 中（1週間） | 高（コスト削減） | 高 | 高 |
| TypeScript対応 | 中（1週間） | 高（利用者増） | 高 | 中-高 |
| Python対応 | 中（1週間） | 中（特定分野） | 中 | 中 |
| Rust対応 | 高（2週間） | 低（ニッチ） | 低 | 低 |

## リスク評価と対策

### リスク1: 言語抽象化の設計ミス
- **影響**: 全体のアーキテクチャに影響
- **対策**: 段階的なリファクタリング、十分な設計レビュー

### リスク2: Tool化によるレスポンス時間の増加
- **影響**: ユーザー体験の低下
- **対策**: キャッシュ機構の実装、並列処理の最適化

### リスク3: 多言語対応による複雑性の増大
- **影響**: メンテナンスコストの増加
- **対策**: 言語ごとのモジュール分離、統合テストの充実

## 成功指標

### フェーズ1完了時
- [ ] 既存のGo実装が新しい抽象レイヤー上で動作
- [ ] TypeResolverのモックを使ったテストが可能
- [ ] コードカバレッジ80%以上

### フェーズ2完了時
- [ ] LLMのトークン使用量が30%以上削減
- [ ] Tool呼び出しのレスポンス時間が1秒以内
- [ ] エラー率が5%以下

### フェーズ3完了時
- [ ] 3言語以上のサポート
- [ ] 各言語で基本的なコード生成が可能
- [ ] 言語ごとのサンプルプロジェクト完備

## まとめ

この計画により：
1. **早期に価値を提供**：TypeResolver as Toolによるコスト削減
2. **技術的負債を最小化**：適切な抽象化による保守性向上
3. **段階的な機能拡張**：各フェーズが独立して価値を提供
4. **リスクの最小化**：基盤から順次構築することで大規模な手戻りを防止

実装は基盤整備から始め、各フェーズの完了ごとに評価を行い、必要に応じて計画を調整する。