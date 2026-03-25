# CLAUDE.md

## プロジェクト概要

Obsidian Vault を操作する MCP サーバー。詳細は [README.md](README.md) を参照。

## 開発方針

### RED-GREEN-REFACTOR

すべての機能追加・バグ修正は RED-GREEN-REFACTOR サイクルで進める。

1. **RED** — 失敗するテストを先に書く
2. **GREEN** — テストを通す最小限のコードを書く
3. **REFACTOR** — 重複除去・構造改善（テストが通ることを維持）

### テスト

- `make test-unit` — ユニットテスト（obsidian CLI 不要、純粋ロジックのみ）
- `OBSIDIAN_TEST_VAULT=<name> make test` — 全テスト（統合テストには Obsidian 起動 + vault が必要）
- 統合テストは vault 内の `_test_obsidian_mcp/` ディレクトリに一時ファイルを作成し、テスト後に削除する
- 新しい統合テストを追加する際は、読み取り専用 → 書き込み系の順で安全側から書く
- テスト用の vault 名は `OBSIDIAN_TEST_VAULT` 環境変数で指定する（ハードコードしない）

### アーキテクチャ

- `src/obsidian.rs` — obsidian CLI 呼び出し + 純粋ロジック。テスト可能な関数はここに置く
- `src/tools.rs` — MCP ツール定義。rmcp の `#[tool_router]` / `#[tool_handler]` マクロを使用
- `src/main.rs` — エントリポイント。ロジックを持たない
- ツール関数の戻り値は `Result<String, String>`（rmcp の `IntoCallToolResult` 経由で自動変換される）

### obsidian CLI の呼び出し規約

- Vault アクセスは原則 `obsidian::run(vault, args)` を経由する
- 引数は `key=value` 形式の文字列スライス（例: `&["read", "path=note.md"]`）
- `obsidian` コマンドの `--help` で利用可能なサブコマンドを確認できる
- obsidian CLI が対応しない操作（mkdir、フォルダ削除）は `obsidian::vault_path()` で vault ルートを解決し、ファイルシステム直接操作で補完する。その際は vault 外パスへのアクセスを必ずガードすること

### ビルド

```sh
make build   # cargo build --release + dist/ へコピー
make test    # 全テスト実行
make clean   # target/ と dist/ を削除
```

変更後は `make test` で全テスト通過を確認すること。
