# obsidian-mcp

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

## 設計思想

- **Claude Code にとって直感的**: 組み込みツールと同じパラメータ体系に `vault` を追加しただけ
- **Obsidian と整合的**: Vault アクセスは原則として公式 CLI 経由。リンク更新やインデックスが壊れない
  - 例外: フォルダの作成(`mkdir`)と削除(`delete` のフォルダ対応)は公式 CLI が未対応のため、vault パスを解決した上でファイルシステムを直接操作する
- **安全**: フォルダ削除は空の場合のみ許可。vault 外へのパス操作はガードされている

## テスト

```sh
# ユニットテストのみ
make test-unit

# 全テスト（統合テストには Obsidian 起動 + vault が必要）
OBSIDIAN_TEST_VAULT=<vault名> make test
```

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
  obsidian-mcp    # リリースバイナリ（make build で生成、git 管理外）
  .mcp.json       # 配布用 MCP 設定テンプレート
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
