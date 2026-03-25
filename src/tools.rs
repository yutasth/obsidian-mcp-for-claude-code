use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerInfo, ToolsCapability};
use rmcp::schemars;
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;

use crate::obsidian;
use crate::secret;

pub struct ObsidianTools {
    tool_router: ToolRouter<Self>,
}

impl ObsidianTools {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// File path relative to vault root (e.g. 'folder/note.md')
    pub path: String,
    /// Line number to start reading from (1-based)
    pub offset: Option<usize>,
    /// Number of lines to read
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// File path relative to vault root
    pub path: String,
    /// Content to write
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// File path relative to vault root
    pub path: String,
    /// The exact text to find and replace
    pub old_string: String,
    /// The replacement text
    pub new_string: String,
    /// Replace all occurrences (default: false)
    pub replace_all: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlobParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// Glob pattern to match (e.g. '**/*.md')
    pub pattern: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrepParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// Search query text
    pub query: String,
    /// Limit to a specific folder path
    pub path: Option<String>,
    /// Case sensitive search (default: false)
    pub case_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// Source file path relative to vault root
    pub path: String,
    /// Destination folder or path (e.g. 'folder/' to move, 'folder/new_name.md' to move and rename)
    pub to: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MkdirParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// Folder path to create relative to vault root
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RmParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// File path relative to vault root
    pub path: String,
    /// Skip trash and delete permanently (default: false)
    pub permanent: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RmdirParams {
    /// Vault name (optional if OBSIDIAN_VAULT env var is set)
    pub vault: Option<String>,
    /// Empty folder path relative to vault root
    pub path: String,
}

#[tool_router]
impl ObsidianTools {
    /// Read a file from the Obsidian vault. Returns the full content of the file.
    /// When OBSIDIAN_HIDE_SECRET is enabled, secrets are replaced with [SECRET:N] placeholders.
    #[tool(name = "Read")]
    fn read(&self, Parameters(params): Parameters<ReadParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        let content = obsidian::run(&vault, &["read", &format!("path={}", params.path)])
            .map_err(|e| e.to_string())?;

        let content = if secret::is_enabled() {
            secret::mask(&content).masked
        } else {
            content
        };

        Ok(obsidian::apply_line_range(&content, params.offset, params.limit))
    }

    /// Create or overwrite a file in the Obsidian vault.
    /// When OBSIDIAN_HIDE_SECRET is enabled, all [SECRET:N] IDs from the original file
    /// must be present in the content. They are expanded back to original text before writing.
    #[tool(name = "Write")]
    fn write(&self, Parameters(params): Parameters<WriteParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        let mut content = params.content.clone();

        if secret::is_enabled() {
            if let Ok(existing) = obsidian::run(&vault, &["read", &format!("path={}", params.path)]) {
                if secret::has_secrets(&existing) {
                    content = secret::expand_write(&existing, &content)?;
                }
            }
        }

        obsidian::run(
            &vault,
            &[
                "create",
                &format!("path={}", params.path),
                &format!("content={}", content),
                "overwrite",
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(format!("Written: {}", params.path))
    }

    /// Edit a file in the Obsidian vault by replacing an exact string match. The old_string must be unique in the file.
    /// When OBSIDIAN_HIDE_SECRET is enabled, [SECRET:N] placeholders in old_string/new_string
    /// are expanded to the original content. The number of secrets must not change.
    #[tool(name = "Edit")]
    fn edit(&self, Parameters(params): Parameters<EditParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        let content = obsidian::run(&vault, &["read", &format!("path={}", params.path)])
            .map_err(|e| e.to_string())?;

        let (old_string, new_string) = if secret::is_enabled() {
            secret::expand_edit(&content, &params.old_string, &params.new_string)?
        } else {
            (params.old_string.clone(), params.new_string.clone())
        };

        let replace_all = params.replace_all.unwrap_or(false);
        let new_content =
            obsidian::replace_content(&content, &old_string, &new_string, replace_all)?;

        obsidian::run(
            &vault,
            &[
                "create",
                &format!("path={}", params.path),
                &format!("content={}", new_content),
                "overwrite",
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(format!("Edited: {}", params.path))
    }

    /// Find files in the Obsidian vault matching a glob pattern (e.g. '**/*.md', 'daily/*.md').
    #[tool(name = "Glob")]
    fn glob(&self, Parameters(params): Parameters<GlobParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        let files_output =
            obsidian::run(&vault, &["files"]).map_err(|e| e.to_string())?;

        let matched: Vec<&str> = files_output
            .lines()
            .filter(|line| glob_match::glob_match(&params.pattern, line))
            .collect();

        if matched.is_empty() {
            Ok("No files matched.".to_string())
        } else {
            Ok(matched.join("\n"))
        }
    }

    /// Search for text across files in the Obsidian vault. Returns matching files and context.
    #[tool(name = "Grep")]
    fn grep(&self, Parameters(params): Parameters<GrepParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        let mut args = vec![
            "search:context".to_string(),
            format!("query={}", params.query),
            "format=json".to_string(),
        ];
        if let Some(ref p) = params.path {
            args.push(format!("path={p}"));
        }
        if params.case_sensitive.unwrap_or(false) {
            args.push("case".to_string());
        }
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let result = obsidian::run(&vault, &args_refs).map_err(|e| e.to_string())?;

        if secret::is_enabled() {
            let masked = secret::mask(&result).masked;
            // Filter out result lines that contain secrets to prevent leaking
            // the fact that a search term matched inside a secret region
            Ok(secret::filter_secret_lines(&masked))
        } else {
            Ok(result)
        }
    }

    /// Move or rename a file in the Obsidian vault. Obsidian will automatically update internal links.
    #[tool(name = "mv")]
    fn mv(&self, Parameters(params): Parameters<MoveParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        obsidian::run(
            &vault,
            &["move", &format!("path={}", params.path), &format!("to={}", params.to)],
        )
        .map_err(|e| e.to_string())
    }

    /// Create a directory in the Obsidian vault.
    #[tool(name = "mkdir")]
    fn mkdir(&self, Parameters(params): Parameters<MkdirParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        obsidian::mkdir(&vault, &params.path).map_err(|e| e.to_string())
    }

    /// Delete a file from the Obsidian vault.
    #[tool(name = "rm")]
    fn rm(&self, Parameters(params): Parameters<RmParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        let permanent = params.permanent.unwrap_or(false);
        obsidian::delete_file(&vault, &params.path, permanent).map_err(|e| e.to_string())
    }

    /// Delete an empty folder from the Obsidian vault.
    #[tool(name = "rmdir")]
    fn rmdir(&self, Parameters(params): Parameters<RmdirParams>) -> Result<String, String> {
        let vault = obsidian::resolve_vault(params.vault).map_err(|e| e.to_string())?;
        obsidian::delete_folder(&vault, &params.path).map_err(|e| e.to_string())
    }
}

#[tool_handler]
impl ServerHandler for ObsidianTools {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities.tools = Some(ToolsCapability::default());
        let vault_note = if std::env::var("OBSIDIAN_VAULT").is_err() {
            "\n\nThe `vault` parameter is required in every tool call."
        } else {
            ""
        };
        let hide_secret = if secret::is_enabled() {
            concat!(
                "\n\n## Secret hiding (currently active)\n",
                "Files may contain [SECRET:N] placeholders replacing confidential content ",
                "(Obsidian ==highlights== and [!secret] callouts).\n",
                "Rules:\n",
                "- Do NOT attempt to guess, decode, or reconstruct secret content.\n",
                "- When editing, every [SECRET:N] from old_string must appear in new_string with the same ID. ",
                "You may reorder them but must not add, remove, or change any ID.\n",
                "- When writing, all [SECRET:N] IDs from the original file must be present.\n",
                "- Search results matching inside secret regions are automatically filtered out.\n",
            )
        } else {
            ""
        };
        info.instructions = Some(format!(
            "Obsidian vault tools mirroring Claude Code's Read/Edit/Write/Glob/Grep interface.{vault_note}{hide_secret}"
        ).into());
        info
    }
}
