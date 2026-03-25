# obsidian-mcp

[日本語](README.md)

An MCP server that lets Claude Code interact with Obsidian Vaults as if they were a local filesystem.

It provides the same interface as Claude Code's built-in tools (Read, Edit, Write, Glob, Grep, LS) for Obsidian Vaults. All vault access goes through the [official Obsidian CLI](https://obsidian.md/help/cli), keeping Obsidian's index and link management intact.

## Usage

### As a dedicated Obsidian agent

Deny Claude Code's built-in tools (Read, Edit, Write, Bash, etc.) and allow only this MCP to create a sandboxed agent that can only operate on Obsidian Vaults:

```jsonc
// .claude/settings.json
{
  "permissions": {
    "deny": ["Read", "Edit", "Write", "Bash", "Glob", "Grep"]
    // obsidian_* tools are automatically available via MCP
  }
}
```

Add the following to CLAUDE.md so Claude Code uses obsidian_* tools instead of built-in ones:

```markdown
# CLAUDE.md

When working with Obsidian Vaults, use MCP obsidian_* tools
instead of built-in Read/Edit/Write/Glob/Grep:

- Read → obsidian_read
- Edit → obsidian_edit
- Write → obsidian_write
- Glob → obsidian_glob
- Grep → obsidian_grep
- LS → obsidian_ls
- Move/rename (mv) → obsidian_move
- Create directory (mkdir) → obsidian_mkdir
- Delete (rm) → obsidian_delete
```

This ensures the agent operates only on the Obsidian Vault without touching the local filesystem.

### Adding to your everyday Claude Code

Use alongside built-in tools to reference and update vault notes while coding. Tool names are prefixed with `obsidian_` so they don't conflict with built-in `Read`, `Edit`, etc.

## Tools

| MCP Tool | Description | Underlying obsidian CLI command |
|---|---|---|
| `obsidian_read` | Read a file (with offset/limit) | `obsidian read` |
| `obsidian_write` | Create or overwrite a file | `obsidian create ... overwrite` |
| `obsidian_edit` | Edit via string replacement | `obsidian read` → replace → `obsidian create ... overwrite` |
| `obsidian_glob` | Find files by glob pattern | `obsidian files` + glob-match |
| `obsidian_grep` | Full-text search | `obsidian search:context` |
| `obsidian_ls` | List files and folders | `obsidian files` + `obsidian folders` |
| `obsidian_move` | Move/rename a file (auto-updates links) | `obsidian move` |
| `obsidian_mkdir` | Create a directory | Direct filesystem operation |
| `obsidian_delete` | Delete a file or empty folder | `obsidian delete` + filesystem (folders) |

## Secret Mode

Set `OBSIDIAN_HIDE_SECRET=true` to hide confidential content marked with specific Obsidian syntax from Claude Code. Useful for vaults containing internal documents or personal secrets.

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

### Enabling

Set `OBSIDIAN_HIDE_SECRET` in the `env` section of `.mcp.json`. The easiest way is to copy the sample config:

```sh
cp dist/mcp.json.secret.example .mcp.json
# Edit the command path in .mcp.json to point to your binary
```

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

# 3. Register the MCP server with Claude Code
claude mcp add obsidian-mcp --scope project -- "$(pwd)/dist/obsidian-mcp"
```

To use from another project, specify the absolute path to the binary:

```sh
claude mcp add obsidian-mcp --scope project -- /path/to/dist/obsidian-mcp
```

Alternatively, copy a sample config from `dist/` and use it as `.mcp.json`:

```sh
# Standard
cp dist/mcp.json.example .mcp.json

# With Secret mode enabled
cp dist/mcp.json.secret.example .mcp.json
```

`.mcp.json` is the config file where Claude Code reads the MCP server's startup command and environment variables. Use the `env` field to set variables like `OBSIDIAN_HIDE_SECRET`.

## Design

- **Intuitive for Claude Code**: Same parameter scheme as built-in tools, with an added `vault` parameter
- **Consistent with Obsidian**: Vault access goes through the official CLI by default, preserving link updates and indexing
  - Exception: Folder creation (`mkdir`) and deletion (`delete` for folders) are not supported by the CLI, so they operate on the filesystem directly after resolving the vault path
- **Safe**: Folder deletion is only allowed for empty folders. Path operations outside the vault are guarded

## Testing

```sh
# Unit tests only
make test-unit

# All tests (integration tests require Obsidian + a vault)
OBSIDIAN_TEST_VAULT=<vault-name> make test
```

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
  mcp.json.example          # MCP config template (standard)
  mcp.json.secret.example   # MCP config template (with Secret mode)
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
