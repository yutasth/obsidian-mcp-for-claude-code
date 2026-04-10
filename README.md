# obsidian-mcp-for-claude-code

[English](README.en.md)

Claude Code から Obsidian Vault をファイルシステムのように扱える MCP サーバー。

Claude Code の組み込みツール（Read, Edit, Write, Glob, Grep）と同じ感覚で Obsidian Vault を操作できる。Vault へのアクセスは [Obsidian 公式 CLI](https://obsidian.md/help/cli) を経由するため、Obsidian のインデックスやリンク管理と整合性が保たれる。

## 使用例

### Obsidian 専用エージェントとして使う

Claude Code の組み込みツール（Read, Edit, Write, Bash 等）を deny にし、この MCP だけを許可することで、Obsidian Vault のみを操作するサンドボックス化されたエージェントを作れる:

```jsonc
// .claude/settings.json
{
  "permissions": {
    "deny": ["Read", "Edit", "Write", "Bash", "Glob", "Grep"]
    // MCP の read/write/edit 等のツールは MCP 経由で自動的に利用可能
  }
}
```

CLAUDE.md に以下のように記載すれば、Claude Code が組み込みツールの代わりに MCP ツールを使うようになる:

```markdown
# CLAUDE.md

Obsidian Vault の操作には組み込みツールではなく、同名の MCP ツールを使うこと。
これらは組み込みツールやシェルコマンドに対応する操作を Vault 上で提供する:

| 組み込みツール / コマンド | MCP ツール |
|---|---|
| Read | mcp__obsidian__Read |
| Write | mcp__obsidian__Write |
| Edit | mcp__obsidian__Edit |
| Glob | mcp__obsidian__Glob |
| Grep | mcp__obsidian__Grep |
| mv | mcp__obsidian__mv |
| mkdir | mcp__obsidian__mkdir |
| rm | mcp__obsidian__rm |
| rmdir | mcp__obsidian__rmdir |
```

これにより、操作対象を Obsidian Vault に限定したエージェントが実現できる。

### 普段使いの Claude Code に追加する

組み込みツールと併用すれば、コーディング中に Vault のメモを参照・更新できる。

## 提供ツール

| MCP ツール | 説明 | 対応する obsidian CLI コマンド |
|---|---|---|
| `Read` | ファイル読み取り（offset/limit 対応） | `obsidian read` |
| `Write` | ファイル作成・上書き | `obsidian create ... overwrite` |
| `Edit` | 文字列置換による編集 | `obsidian read` → 置換 → `obsidian create ... overwrite` |
| `Glob` | glob パターンでファイル・ディレクトリ検索（`path` で範囲限定可、`_directory_descriptions.md` によるディレクトリ説明付与） | `obsidian files` + `obsidian folders` + glob-match |
| `Grep` | テキスト全文検索（`output_mode`: content/files_with_matches/count、`head_limit`、`glob` フィルタ対応） | `obsidian search:context` |
| `mv` | ファイルの移動・リネーム（リンク自動更新） | `obsidian move` |
| `mkdir` | ディレクトリ作成（`description` で説明も同時設定可） | ファイルシステム直接操作 |
| `rm` | ファイルの削除 | `obsidian delete` |
| `rmdir` | 空ディレクトリの削除（説明も自動削除） | ファイルシステム直接操作 |
| `update_directory_description` | ディレクトリ説明の更新・削除（`_directory_descriptions.md` を管理） | `obsidian read` → 更新 → `obsidian create ... overwrite` |

## 前提条件

- [Obsidian](https://obsidian.md/) が起動していること
- [Obsidian CLI](https://obsidian.md/help/cli) (`obsidian` コマンド) が利用可能であること — Obsidian の設定 → 一般 → CLI ツールからインストールできる
- [Rust toolchain](https://rustup.rs/) (1.94.0+)

## セットアップ

```sh
# 1. リポジトリをクローン
git clone https://github.com/yutasth/obsidian-mcp-for-claude-code.git
cd obsidian-mcp-for-claude-code

# 2. ビルド
make build
```

## 設定

### 環境変数

| 環境変数 | 説明 | 必須 |
|---|---|---|
| `OBSIDIAN_VAULT` | デフォルトの vault 名。設定すると各ツールの `vault` パラメータを省略できる | 推奨 |
| `OBSIDIAN_HIDE_SECRET` | `true` にすると [Secret モード](#secret-モード) が有効になる | 任意 |

### MCP サーバーの登録

`claude mcp add` で登録する場合、`-e` で環境変数を渡せる:

```sh
claude mcp add obsidian --scope project \
  -e OBSIDIAN_VAULT=my-vault \
  -- /path/to/dist/obsidian-mcp
```

または、`.mcp.json` を手動で作成する:

`.mcp.json` の例（全環境変数を設定する場合）:

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "/path/to/dist/obsidian-mcp",
      "env": {
        "OBSIDIAN_VAULT": "my-vault",
        "OBSIDIAN_HIDE_SECRET": "true"
      }
    }
  }
}
```

### ツールの自動許可

毎回の確認を省くには、`.claude/settings.json`（または `.claude/settings.local.json`）でワイルドカード許可を設定する:

```jsonc
// .claude/settings.json
{
  "permissions": {
    "allow": [
      "mcp__obsidian__*"
    ]
  }
}
```

### vault パラメータの解決順序

1. ツール呼び出し時に `vault` を明示的に指定 → それを使う
2. 省略した場合 → 環境変数 `OBSIDIAN_VAULT` にフォールバック
3. どちらもない → エラー

複数 vault を使い分けたい場合は、デフォルトを環境変数で設定し、必要なときだけ `vault` を指定すればよい。

## Secret モード

環境変数 `OBSIDIAN_HIDE_SECRET=true` を設定すると、Obsidian の特定の記法で書かれた機密情報が Claude Code から隠される。社内文書や個人の秘密情報を含む vault を扱うときに有効。設定方法は[設定](#設定)セクションを参照。

### 対象となる記法

**ハイライト構文** (`==` で囲む):

```markdown
プロジェクトのコードネームは ==Project Aurora== です。
担当者の連絡先: ==090-xxxx-xxxx==
```

**`[!secret]` コールアウト**:

```markdown
> [!secret]
> 契約金額: 5,000万円
> 契約期間: 2026年4月〜2027年3月
```

### Claude Code から見える内容

上記の記法は `[SECRET:N]` プレースホルダーに置換される:

```markdown
プロジェクトのコードネームは [SECRET:1] です。
担当者の連絡先: [SECRET:2]
[SECRET:3]
```

### ルール

- **read/grep**: 秘密は `[SECRET:N]` に置換される。grep で秘密の内部にマッチした結果は自動的に除外される
- **edit**: `old_string` と `new_string` に含まれる `[SECRET:N]` の ID 集合が一致していなければ拒否。順序の入れ替えはOK
- **write**: 元ファイルの全 `[SECRET:N]` ID が含まれていなければ拒否。ID が揃っていれば書き込み可能
- 秘密の追加・削除は Obsidian 上で直接行う

## テスト

```sh
# ユニットテストのみ（Obsidian 不要）
make test-unit

# 全テスト（統合テストには Obsidian 起動 + vault が必要）
OBSIDIAN_TEST_VAULT=<vault名> make test
```

### 統合テストの準備

統合テストは実際の Obsidian Vault に対してファイルの作成・読み取り・編集・削除を行う。**普段使いの vault ではなく、テスト専用の vault を作成することを強く推奨する。**

1. Obsidian で新しい vault を作成する（例: `obsidian-mcp-test`）
2. Obsidian が起動している状態で統合テストを実行する:

```sh
OBSIDIAN_TEST_VAULT=obsidian-mcp-test make test
```

テストは vault 内の `_test_obsidian_mcp/` ディレクトリに一時ファイルを作成し、テスト完了後にディレクトリごと削除する。

## 設計思想

- **Claude Code にとって直感的**: 組み込みツールと同じパラメータ体系。`vault` は省略可能で、環境変数によるデフォルト指定に対応
- **Obsidian と整合的**: Vault アクセスは原則として公式 CLI 経由。リンク更新やインデックスが壊れない
  - 例外: フォルダの作成(`mkdir`)と削除(`delete` のフォルダ対応)は公式 CLI が未対応のため、vault パスを解決した上でファイルシステムを直接操作する
- **安全**: フォルダ削除は空の場合のみ許可。vault 外へのパス操作はガードされている

## プロジェクト構成

```
src/
  lib.rs        # ライブラリルート
  main.rs       # エントリポイント（stdio transport で MCP サーバー起動）
  obsidian.rs   # obsidian CLI ラッパー + 純粋ロジック
  tools.rs      # MCP ツール定義（rmcp の #[tool_router] / #[tool_handler]）
tests/
  integration.rs  # 実 vault に対する統合テスト
dist/
  obsidian-mcp              # リリースバイナリ（make build で生成、git 管理外）
Makefile          # build / test / clean
```

## 技術スタック

- **Rust** — 実行速度重視
- **[rmcp](https://github.com/modelcontextprotocol/rust-sdk)** (v1.2) — 公式 Rust MCP SDK
- **tokio** — async runtime（MCP transport 用）
- **glob-match** — Vault 内ファイルの glob マッチング
- **thiserror** — エラー型定義

## ライセンス

[MIT](LICENSE)
