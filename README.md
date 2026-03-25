# obsidian-mcp

[English](README.en.md)

Claude Code から Obsidian Vault をファイルシステムのように扱える MCP サーバー。

Claude Code の組み込みツール（Read, Edit, Write, Glob, Grep, LS）と同じ感覚で Obsidian Vault を操作できる。Vault へのアクセスは [Obsidian 公式 CLI](https://obsidian.md/help/cli) を経由するため、Obsidian のインデックスやリンク管理と整合性が保たれる。

## 使用例

### Obsidian 専用エージェントとして使う

Claude Code の組み込みツール（Read, Edit, Write, Bash 等）を deny にし、この MCP だけを許可することで、Obsidian Vault のみを操作するサンドボックス化されたエージェントを作れる:

```jsonc
// .claude/settings.json
{
  "permissions": {
    "deny": ["Read", "Edit", "Write", "Bash", "Glob", "Grep"]
    // obsidian_* ツールは MCP 経由で自動的に利用可能
  }
}
```

CLAUDE.md に以下のように記載すれば、Claude Code が組み込みツールの代わりに obsidian_* ツールを使うようになる:

```markdown
# CLAUDE.md

Obsidian Vault を操作するときは、組み込みの Read/Edit/Write/Glob/Grep ではなく、
MCP の obsidian_* ツールを使うこと。対応は以下の通り:

- Read → obsidian_read
- Edit → obsidian_edit
- Write → obsidian_write
- Glob → obsidian_glob
- Grep → obsidian_grep
- LS → obsidian_ls
- ファイル移動 (mv) → obsidian_move
- ディレクトリ作成 (mkdir) → obsidian_mkdir
- 削除 (rm) → obsidian_delete
```

これにより、ローカルファイルシステムには一切触れず Obsidian Vault だけを安全に操作するエージェントが実現できる。

### 普段使いの Claude Code に追加する

組み込みツールと併用すれば、コーディング中に Vault のメモを参照・更新できる。ツール名が `obsidian_read`, `obsidian_edit` のように prefix 付きなので、組み込みの `Read`, `Edit` と衝突しない。

## 提供ツール

| MCP ツール | 説明 | 対応する obsidian CLI コマンド |
|---|---|---|
| `obsidian_read` | ファイル読み取り（offset/limit 対応） | `obsidian read` |
| `obsidian_write` | ファイル作成・上書き | `obsidian create ... overwrite` |
| `obsidian_edit` | 文字列置換による編集 | `obsidian read` → 置換 → `obsidian create ... overwrite` |
| `obsidian_glob` | glob パターンでファイル検索 | `obsidian files` + glob-match |
| `obsidian_grep` | テキスト全文検索 | `obsidian search:context` |
| `obsidian_ls` | ディレクトリ内のファイル・フォルダ一覧 | `obsidian files` + `obsidian folders` |
| `obsidian_move` | ファイルの移動・リネーム（リンク自動更新） | `obsidian move` |
| `obsidian_mkdir` | ディレクトリ作成 | ファイルシステム直接操作 |
| `obsidian_delete` | ファイル・空フォルダの削除 | `obsidian delete` + ファイルシステム（フォルダ） |

## Secret モード

環境変数 `OBSIDIAN_HIDE_SECRET=true` を設定すると、Obsidian の特定の記法で書かれた機密情報が Claude Code から隠される。社内文書や個人の秘密情報を含む vault を扱うときに有効。

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

### 有効化

`.mcp.json` の `env` に `OBSIDIAN_HIDE_SECRET` を設定する。`dist/mcp.json.secret.example` をコピーして使うのが簡単:

```sh
cp dist/mcp.json.secret.example .mcp.json
# .mcp.json 内の command パスを実際のバイナリパスに書き換える
```

## 前提条件

- [Obsidian](https://obsidian.md/) が起動していること
- [Obsidian CLI](https://obsidian.md/help/cli) (`obsidian` コマンド) が利用可能であること — Obsidian の設定 → 一般 → CLI ツールからインストールできる
- [Rust toolchain](https://rustup.rs/) (1.94.0+)

## セットアップ

```sh
# 1. リポジトリをクローン
git clone https://github.com/yutasth/obsidian-cli-for-claude-code.git
cd obsidian-cli-for-claude-code

# 2. ビルド
make build

# 3. Claude Code に MCP サーバーを登録
claude mcp add obsidian-mcp --scope project -- "$(pwd)/dist/obsidian-mcp"
```

他のプロジェクトから使う場合は、バイナリの絶対パスを指定して登録する:

```sh
claude mcp add obsidian-mcp --scope project -- /path/to/dist/obsidian-mcp
```

または、`dist/` 内のサンプル設定をコピーして `.mcp.json` として使う:

```sh
# 通常版
cp dist/mcp.json.example .mcp.json

# Secret モード有効版
cp dist/mcp.json.secret.example .mcp.json
```

`.mcp.json` は Claude Code が MCP サーバーの起動コマンドと環境変数を読み取る設定ファイル。`env` フィールドで `OBSIDIAN_HIDE_SECRET` などの環境変数を設定できる。

## Vault の指定

すべてのツールに `vault` パラメータがあるが、環境変数 `OBSIDIAN_VAULT` を設定しておけば省略できる。Claude Code の組み込みツール（Read, Edit 等）にはない vault 指定を毎回行う必要がなくなる。

```sh
# claude mcp add のときに環境変数を渡す
claude mcp add obsidian-mcp --scope project -e OBSIDIAN_VAULT=my-vault -- /path/to/dist/obsidian-mcp
```

または `.mcp.json` の `env` に設定する:

```json
{
  "mcpServers": {
    "obsidian-mcp": {
      "command": "/path/to/dist/obsidian-mcp",
      "env": {
        "OBSIDIAN_VAULT": "my-vault"
      }
    }
  }
}
```

`vault` パラメータを明示的に指定した場合は環境変数より優先される。複数 vault を使い分けたい場合は、デフォルトを環境変数で設定し、必要なときだけ `vault` を指定すればよい。

## 設計思想

- **Claude Code にとって直感的**: 組み込みツールと同じパラメータ体系。`vault` は省略可能で、環境変数によるデフォルト指定に対応
- **Obsidian と整合的**: Vault アクセスは原則として公式 CLI 経由。リンク更新やインデックスが壊れない
  - 例外: フォルダの作成(`mkdir`)と削除(`delete` のフォルダ対応)は公式 CLI が未対応のため、vault パスを解決した上でファイルシステムを直接操作する
- **安全**: フォルダ削除は空の場合のみ許可。vault 外へのパス操作はガードされている

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
  mcp.json.example          # MCP 設定テンプレート（通常版）
  mcp.json.secret.example   # MCP 設定テンプレート（Secret モード有効版）
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
