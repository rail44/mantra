# `Glyph` Architecture Document (Local-First Edition)

## 1\. 概要 (Overview)

`Glyph`は、開発者のローカルマシン上で完結する、インタラクティブな開発ツールである。宣言的な仕様記述から、Spannerに最適化されたGoのデータアクセス層コードをAIによってリアルタイムに生成する。

このツールの核心は、開発者が\*\*「何を(What)」したいか**という宣言に集中し、**「どうやるか(How)」\*\*という実装作業をAIに委任する点にある。これにより、**ローカル環境での高速なフィードバックループ**を実現し、開発の生産性と創造性を最大化する。

### 哲学

  * **宣言駆動 (Declaration-Driven):** 開発者の「意図」を唯一の信頼できる情報源（Single Source of Truth）とする。
  * **AIによる実装 (AI-Powered Implementation):** AIを、文脈を理解する高度な「実装パートナー」として活用する。
  * **インタラクティブな開発体験 (Interactive DX):** 宣言と実装のフィードバックループを限りなく短くし、思考を中断させない。

## 2\. アーキテクチャ図 (Architecture Diagram)

開発者のローカル環境で、すべてのプロセスが完結する。

```mermaid
graph TD
    subgraph "Developer's Local Environment"
        Dev[👨‍💻 Developer]
        Editor[📝 Editor (e.g., Neovim)]
        CLI[⚙️ Glyph CLI (Interactive Mode)]
        LLM[🧠 Local LLM (Ollama + Devstral)]
        Emulator[📦 Spanner Emulator (Optional)]

        Dev -- "1. Edits Declaration File" --> Editor
        Editor -- "2. Saves File" --> CLI
        CLI -- "3. Watches File Change" --> CLI
        CLI -- "4. Generates Context-Rich Prompt" --> LLM
        LLM -- "5. Generates Implementation" --> CLI
        CLI -- "6. Displays Result in Real-time" --> Dev
        CLI -- "7. Executes Implementation (Optional)" --> Emulator
        Emulator -- "8. Returns Result" --> CLI
    end
```

## 3\. コンポーネント詳細 (Component Breakdown)

`Glyph` CLIツールは、協調して動作する複数のモジュールで構成される。

### 3.1. CLIフレームワーク (CLI Framework)

  * **技術スタック**: `spf13/cobra`, `spf13/viper`
  * **役割**:
      * `glyph watch <file>`のようなサブコマンドとフラグの管理。
      * 設定ファイル（`~/.glyph.yaml`など）や環境変数からの設定値（モデル名など）の読み込み。
      * アプリケーション全体の入り口として、各モジュールを制御する。

### 3.2. インタラクティブ・モード (Interactive Mode)

  * **技術スタック**: `charmbracelet/bubbletea`, `fsnotify`
  * **役割**:
      * 開発者のためのメインインターフェース。`glyph watch`コマンドで起動する。
      * **ファイル監視**: `fsnotify`を利用し、宣言ファイルの保存イベントをリアルタイムに検知する。
      * **リアルタイム再生成**: ファイル変更をトリガーに、Core Logicを呼び出し、実装の再生成を実行する。
      * **リッチなUI**: 生成中のスピナー表示、生成結果（特に差分）のシンタックスハイライト、エラーメッセージの整形など、洗練されたTUIを提供する。

### 3.3. コアロジック (Core Logic)

#### 3.3.1. 宣言パーサー (Declaration Parser)

  * **技術スタック**: `go/parser`, `go/ast`
  * **役割**: Goで書かれた宣言ファイルを静的解析し、構造化された情報を抽出する。
      * 対象の`Request`/`Response`構造体の名前とフィールド定義。
      * `@description`などのカスタムディレクティブ。

#### 3.3.2. コンテキスト・リッチ・プロンプトビルダー (Context-Rich Prompt Builder)

  * **役割**: AIに渡すための最適なプロンプトを動的に組み立てる。これは`Glyph`の最もインテリジェントな部分である。
  * **収集するコンテキスト**:
    1.  **新しい仕様**: 宣言パーサーが抽出した最新の宣言情報。
    2.  **人間による修正**: 以前のAI生成物と現在の実装ファイルを比較し、人間による手動修正の差分（diff）を特定する。
    3.  **内蔵ナレッジ**: Spannerのベストプラクティスなど、ツール自体が持つ静的な知識。
  * **生成するプロンプト**: これらの情報を組み合わせ、「新しい仕様を満たしつつ、人間による価値ある変更を尊重するように」という高度な指示を生成する。

#### 3.3.3. AIクライアント (AI Client)

  * **技術スタック**: 標準の`net/http`。
  * **役割**:
      * プロンプトビルダーが生成したプロンプトを、ローカルで稼働するAI（Ollama/Devstral）のエンドポイントに送信する。
      * AIからの応答を実装ジェネレーターに渡す。

#### 3.3.4. 実装ジェネレーター (Implementation Generator)

  * **役割**:
      * AIからの応答（SQL、Goコード）を受け取り、`_impl.go`ファイルとして整形・保存する。

## 4\. 開発・利用フロー (Workflow)

1.  **起動**: 開発者はターミナルで`glyph watch user_queries.go`を実行し、ツールをインタラクティブ・モードで起動する。
2.  **宣言**: 開発者は、使い慣れたエディタ（`neovim`など）で`user_queries.go`を開き、取得したいデータの構造とAIへのヒントを記述する。
3.  **対話的生成**: ファイルを保存するたびに、隣のターミナルで`Glyph`が変更を検知し、実装ファイル (`_impl.go`) がリアルタイムに更新されるのを確認する。
4.  **手動修正**: AIの生成物が期待通りでない場合、開発者は`_impl.go`をためらわずに直接編集・修正する。この手動修正は、次回のAIによる再生成時に「尊重すべきコンテキスト」として考慮される。
5.  **コミット**: 開発者は、最終的に完成した宣言ファイルと実装ファイルの両方をGitにコミットする。

このアーキテクチャにより、CI/CDなどの外部環境に依存せず、開発者個人のマシン上で完結する、高速で創造的な開発サイクルが実現される。
