# TypeResolver設計ドキュメント

## 概要
TypeResolverは、LLMがコード生成時に型情報を動的に取得するための仕組みです。
Phase 1-2では基盤実装、Phase 2でLLMツール化を目指します。

## 設計の中心概念：スコープID

### スコープIDとは
- コードスニペットを一意に識別するID
- LLMとの対話で使用される識別子
- 内部的にはファイルURI + 位置範囲で管理

### スコープIDの要件
1. **一意性**: 同じコード片は常に同じID
2. **可読性**: LLMが内容を推測できる
3. **一貫性**: 言語間で統一された形式
4. **ツール互換**: ツール呼び出しの引数として使える

## アーキテクチャ

### 基本的な流れ

```
1. 初期コンテキスト生成
   ↓
   [scope: UserService.SaveUser]
   func (s *UserService) SaveUser(user *User) error { ... }
   
2. LLMツール呼び出し
   ↓
   get_type(scope: "UserService.SaveUser", symbol: "User")
   
3. 型情報取得（内部処理）
   ↓
   - スコープID → URI + Range解決
   - tree-sitterでシンボル位置特定
   - LSP definitionで定義位置取得
   
4. 結果返却
   ↓
   [scope: User@model]
   type User struct { ... }
```

### コンポーネント構成

```rust
// スコープ管理
struct ScopeManager {
    scopes: HashMap<String, ScopeInfo>,
}

struct ScopeInfo {
    uri: String,
    range: Range,
    source: String,
}

// 型情報解決
trait TypeResolver {
    async fn get_type_in_scope(
        &self,
        scope_manager: &ScopeManager,
        scope_id: &str,
        symbol: &str,
    ) -> Result<Option<TypeInfo>>;
}

// 言語サポート（スコープID生成）
trait LanguageSupport {
    fn generate_scope_id(
        &self,
        uri: &str,
        range: &Range,
        source: &str,
    ) -> String;
    
    fn resolve_symbol_in_scope(
        &self,
        scope_id: &str,
        symbol_name: &str,
        source: &str,
    ) -> Option<Position>;
}
```

## スコープID生成規則

### 基本規則
各言語の`LanguageSupport`実装が、言語固有の規則でスコープIDを生成します。

### Go
```go
func SaveUser()                 → "SaveUser"
func (s *Service) SaveUser()    → "Service.SaveUser"
type User struct                → "User"
```

### TypeScript/JavaScript
```typescript
function saveUser()              → "saveUser"
class Service { saveUser() }    → "Service.saveUser"
const saveUser = () => {}       → "saveUser"  // 変数名を使用
export default function()       → "default"   // 特殊ケース
```

### Python
```python
def save_user():                 → "save_user"
class Service:
    def save_user(self):        → "Service.save_user"
```

### Rust
```rust
fn save_user()                   → "save_user"
impl Service {
    fn save_user(&self)         → "Service::save_user"
}
```

## 特殊ケースの対処

### 1. 無名関数/ラムダ
- **方針**: 変数名優先、なければ位置ベース
- **実装**: tree-sitterで親ノードから変数名を取得

```typescript
const handler = () => {}        → "handler"
export default function() {}    → "default"
(() => {})()                    → "anonymous_L42"
```

### 2. 同一ファイル内の同名関数
- **方針**: オーバーロード番号を付与
- **実装**: tree-sitterで同名関数をカウント

```go
func SaveUser(id string)        → "SaveUser#1"
func SaveUser(user *User)       → "SaveUser#2"
```

### 3. ネスト構造
- **方針**: `.`で統一した完全修飾名
- **実装**: tree-sitterで親を辿って階層を構築

```java
class Outer {
    class Inner {
        void method()           → "Outer.Inner.method"
    }
}
```

## LSPとの連携

### 使用するLSP RPC
- `textDocument/hover`: 型情報の取得
- `textDocument/definition`: 定義位置の取得
- `textDocument/references`: 参照箇所の取得（将来）
- `textDocument/documentSymbol`: シンボル一覧（将来）

### LSPから確実に取得できる情報
- ファイルURI
- 位置情報（line, character）
- 範囲（Range）

### LSP実装依存の情報
- hoverの内容フォーマット
- シンボル名
- パッケージ/モジュール名

## 実装フェーズ

### Phase 1-2: 基盤実装
1. `ScopeManager`の実装
2. `TypeResolver` traitの定義
3. `LspTypeResolver`の実装
4. 各言語の`generate_scope_id`実装

### Phase 2: LLMツール化
1. ツールインターフェースの定義
2. スコープID解決の最適化
3. キャッシュ機構の実装

## 利点

1. **段階的な型情報取得**
   - 必要な型情報のみを取得
   - トークン使用量の削減

2. **言語非依存な設計**
   - 各言語がLanguageSupportを実装
   - 統一されたインターフェース

3. **LLMフレンドリー**
   - 直感的なスコープID
   - ツール引数との一致

## 考慮事項

1. **パフォーマンス**
   - tree-sitter解析のキャッシュ
   - LSP呼び出しの最適化

2. **エラーハンドリング**
   - スコープIDが見つからない場合
   - LSPが型情報を返さない場合

3. **将来の拡張**
   - 新言語の追加
   - より高度な型推論