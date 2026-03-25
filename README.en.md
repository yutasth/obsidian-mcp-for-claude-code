# obsidian-mcp

[日本語](README.md)

An MCP server that lets Claude Code interact with Obsidian Vaults as if they were a local filesystem.

It provides the same interface as Claude Code's built-in tools (Read, Edit, Write, Glob, Grep) for Obsidian Vaults. All vault access goes through the [official Obsidian CLI](https://obsidian.md/help/cli), keeping Obsidian's index and link management intact.

## Usage

### As a dedicated Obsidian agent

Deny Claude Code's built-in tools (Read, Edit, Write, Bash, etc.) and allow only this MCP to create a sandboxed agent that can only operate on Obsidian Vaults:

```jsonc
// .claude/settings.json
{
  "permissions": {
    "deny": ["Read", "Edit", "Write", "Bash", "Glob", "Grep"]
    // MCP read/write/edit tools are automatically available via MCP
  }
}
```

Add the following to CLAUDE.md so Claude Code uses MCP tools instead of built-in ones:

```markdown
# CLAUDE.md

Use the MCP equivalents instead of built-in tools for Obsidian Vault operations.
Each MCP tool mirrors its built-in counterpart on the vault:

| Built-in tool / command | MCP tool |
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

This limits the agent's scope to the Obsidian Vault.

### Adding to your everyday Claude Code

Use alongside built-in tools to reference and update vault notes while coding. The MCP server name `obsidian` acts as a prefix (`mcp__obsidian__read`, etc.), so they don't conflict with built-in `Read`, `Edit`, etc.

## Tools

| MCP Tool | Description | Underlying obsidian CLI command |
|---|---|---|
| `Read` | Read a file (with offset/limit) | `obsidian read` |
| `Write` | Create or overwrite a file | `obsidian create ... overwrite` |
| `Edit` | Edit via string replacement | `obsidian read` → replace → `obsidian create ... overwrite` |
| `Glob` | Find files by glob pattern | `obsidian files` + glob-match |
| `Grep` | Full-text search | `obsidian search:context` |
| `mv` | Move/rename a file (auto-updates links) | `obsidian move` |
| `mkdir` | Create a directory | Direct filesystem operation |
| `rm` | Delete a file | `obsidian delete` |
| `rmdir` | Delete an empty folder | Direct filesystem operation |

## Prerequisites

- [Obsidian](https://obsidian.md/) must be running
- [Obsidian CLI](https://obsidian.md/help/cli) (`obsidian` command) must be available — install from Obsidian Settings → General → CLI tools
- [Rust toolchain](https://rustup.rs/) (1.94.0+)

## Setup

```sh
# 1. Clone the repository
git clone https://github.com/yutasth/obsidian-cli-for-claude-code.git
cd obsidian-cli-for-claude-code

# 2. Build
make build
```

## Configuration

### Environment variables

| Variable | Description | Required |
|---|---|---|
| `OBSIDIAN_VAULT` | Default vault name. When set, the `vault` parameter can be omitted from tool calls | Recommended |
| `OBSIDIAN_HIDE_SECRET` | Set to `true` to enable [Secret mode](#secret-mode) | Optional |

### Registering the MCP server

With `claude mcp add`, pass environment variables via `-e`:

```sh
claude mcp add obsidian --scope project \
  -e OBSIDIAN_VAULT=my-vault \
  -- /path/to/dist/obsidian-mcp
```

Alternatively, create `.mcp.json` manually:

Example `.mcp.json` with all environment variables:

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

### Auto-allowing tools

To skip confirmation prompts, add a wildcard allow rule to `.claude/settings.json` (or `.claude/settings.local.json`):

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

### Vault resolution order

1. Explicit `vault` parameter in the tool call → used as-is
2. Omitted → falls back to the `OBSIDIAN_VAULT` environment variable
3. Neither set → error

If you work with multiple vaults, set the default via the env var and override per-call when needed.

## Secret Mode

Set `OBSIDIAN_HIDE_SECRET=true` to hide confidential content marked with specific Obsidian syntax from Claude Code. Useful for vaults containing internal documents or personal secrets. See [Configuration](#configuration) for how to set this up.

### Supported syntax

**Highlight syntax** (wrapped with `==`):

```markdown
The project codename is ==Project Aurora==.
Contact: ==090-xxxx-xxxx==
```

**`[!secret]` callout**:

```markdown
> [!secret]
> Contract value: $500,000
> Term: April 2026 – March 2027
```

### What Claude Code sees

The above is replaced with `[SECRET:N]` placeholders:

```markdown
The project codename is [SECRET:1].
Contact: [SECRET:2]
[SECRET:3]
```

### Rules

- **read/grep**: Secrets are replaced with `[SECRET:N]`. Search results matching inside secret regions are automatically filtered out.
- **edit**: The set of `[SECRET:N]` IDs in `old_string` and `new_string` must be identical. Reordering is OK, but adding, removing, or changing IDs is rejected.
- **write**: All `[SECRET:N]` IDs from the original file must be present. The write is allowed if all IDs are accounted for.
- Adding or removing secrets should be done directly in Obsidian.

## Testing

```sh
# Unit tests only (no Obsidian required)
make test-unit

# All tests (integration tests require Obsidian + a vault)
OBSIDIAN_TEST_VAULT=<vault-name> make test
```

### Setting up for integration tests

Integration tests perform real file operations (create, read, edit, delete) against an Obsidian Vault. **It is strongly recommended to create a dedicated vault for testing rather than using your everyday vault.**

1. Create a new vault in Obsidian (e.g. `obsidian-mcp-test`)
2. With Obsidian running, execute the integration tests:

```sh
OBSIDIAN_TEST_VAULT=obsidian-mcp-test make test
```

Tests create temporary files under `_test_obsidian_mcp/` in the vault and remove the entire directory after completion.

## Design

- **Intuitive for Claude Code**: Same parameter scheme as built-in tools. `vault` is optional and defaults to the `OBSIDIAN_VAULT` environment variable
- **Consistent with Obsidian**: Vault access goes through the official CLI by default, preserving link updates and indexing
  - Exception: Folder creation (`mkdir`) and deletion (`delete` for folders) are not supported by the CLI, so they operate on the filesystem directly after resolving the vault path
- **Safe**: Folder deletion is only allowed for empty folders. Path operations outside the vault are guarded

## Project Structure

```
src/
  lib.rs        # Library root
  main.rs       # Entry point (starts MCP server via stdio transport)
  obsidian.rs   # Obsidian CLI wrapper + pure logic
  secret.rs     # Secret masking/unmasking logic
  tools.rs      # MCP tool definitions (rmcp #[tool_router] / #[tool_handler])
tests/
  integration.rs  # Integration tests against a real vault
dist/
  obsidian-mcp              # Release binary (generated by make build, not tracked by git)
Makefile          # build / test / clean
```

## Tech Stack

- **Rust** — Performance-focused
- **[rmcp](https://github.com/modelcontextprotocol/rust-sdk)** (v1.2) — Official Rust MCP SDK
- **tokio** — Async runtime (for MCP transport)
- **glob-match** — Glob matching for vault files
- **thiserror** — Error type definitions

## License

[MIT](LICENSE)
