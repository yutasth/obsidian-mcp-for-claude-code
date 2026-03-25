use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ServerInfo;
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
pub struct ReadParams {
    /// Vault name
    pub vault: String,
    /// File path relative to vault root (e.g. 'folder/note.md')
    pub path: String,
    /// Line number to start reading from (1-based)
    pub offset: Option<usize>,
    /// Number of lines to read
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteParams {
    /// Vault name
    pub vault: String,
    /// File path relative to vault root
    pub path: String,
    /// Content to write
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditParams {
    /// Vault name
    pub vault: String,
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
pub struct GlobParams {
    /// Vault name
    pub vault: String,
    /// Glob pattern to match (e.g. '**/*.md')
    pub pattern: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GrepParams {
    /// Vault name
    pub vault: String,
    /// Search query text
    pub query: String,
    /// Limit to a specific folder path
    pub path: Option<String>,
    /// Case sensitive search (default: false)
    pub case_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LsParams {
    /// Vault name
    pub vault: String,
    /// Folder path to list (omit for vault root)
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MoveParams {
    /// Vault name
    pub vault: String,
    /// Source file path relative to vault root
    pub path: String,
    /// Destination folder or path (e.g. 'folder/' to move, 'folder/new_name.md' to move and rename)
    pub to: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MkdirParams {
    /// Vault name
    pub vault: String,
    /// Folder path to create relative to vault root
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteParams {
    /// Vault name
    pub vault: String,
    /// File or folder path relative to vault root
    pub path: String,
    /// Skip trash and delete permanently (default: false)
    pub permanent: Option<bool>,
}

#[tool_router]
impl ObsidianTools {
    /// Read a file from the Obsidian vault. Returns the full content of the file.
    /// When OBSIDIAN_HIDE_SECRET is enabled, secrets are replaced with [SECRET:N] placeholders.
    #[tool(name = "obsidian_read")]
    fn read(&self, Parameters(params): Parameters<ReadParams>) -> Result<String, String> {
        let content = obsidian::run(&params.vault, &["read", &format!("path={}", params.path)])
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
    #[tool(name = "obsidian_write")]
    fn write(&self, Parameters(params): Parameters<WriteParams>) -> Result<String, String> {
        let mut content = params.content.clone();

        if secret::is_enabled() {
            if let Ok(existing) = obsidian::run(&params.vault, &["read", &format!("path={}", params.path)]) {
                if secret::has_secrets(&existing) {
                    content = secret::expand_write(&existing, &content)?;
                }
            }
        }

        obsidian::run(
            &params.vault,
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
    #[tool(name = "obsidian_edit")]
    fn edit(&self, Parameters(params): Parameters<EditParams>) -> Result<String, String> {
        let content = obsidian::run(&params.vault, &["read", &format!("path={}", params.path)])
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
            &params.vault,
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
    #[tool(name = "obsidian_glob")]
    fn glob(&self, Parameters(params): Parameters<GlobParams>) -> Result<String, String> {
        let files_output =
            obsidian::run(&params.vault, &["files"]).map_err(|e| e.to_string())?;

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
    #[tool(name = "obsidian_grep")]
    fn grep(&self, Parameters(params): Parameters<GrepParams>) -> Result<String, String> {
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

        let result = obsidian::run(&params.vault, &args_refs).map_err(|e| e.to_string())?;

        if secret::is_enabled() {
            let masked = secret::mask(&result).masked;
            // Filter out result lines that contain secrets to prevent leaking
            // the fact that a search term matched inside a secret region
            Ok(secret::filter_secret_lines(&masked))
        } else {
            Ok(result)
        }
    }

    /// List files and folders in a vault directory.
    #[tool(name = "obsidian_ls")]
    fn ls(&self, Parameters(params): Parameters<LsParams>) -> Result<String, String> {
        let folder_arg = params.path.as_ref().map(|p| format!("folder={p}"));

        let files_args: Vec<&str> = if let Some(ref fa) = folder_arg {
            vec!["files", fa]
        } else {
            vec!["files"]
        };

        let folders_args: Vec<&str> = if let Some(ref fa) = folder_arg {
            vec!["folders", fa]
        } else {
            vec!["folders"]
        };

        let files = obsidian::run(&params.vault, &files_args).map_err(|e| e.to_string())?;
        let folders = obsidian::run(&params.vault, &folders_args).map_err(|e| e.to_string())?;

        let mut result = String::new();
        if !folders.trim().is_empty() {
            result.push_str("## Folders\n");
            result.push_str(folders.trim());
            result.push('\n');
        }
        if !files.trim().is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("## Files\n");
            result.push_str(files.trim());
        }
        if result.is_empty() {
            result.push_str("Directory is empty.");
        }

        Ok(result)
    }

    /// Move or rename a file in the Obsidian vault. Obsidian will automatically update internal links.
    #[tool(name = "obsidian_move")]
    fn mv(&self, Parameters(params): Parameters<MoveParams>) -> Result<String, String> {
        obsidian::run(
            &params.vault,
            &["move", &format!("path={}", params.path), &format!("to={}", params.to)],
        )
        .map_err(|e| e.to_string())
    }

    /// Create a directory in the Obsidian vault.
    #[tool(name = "obsidian_mkdir")]
    fn mkdir(&self, Parameters(params): Parameters<MkdirParams>) -> Result<String, String> {
        obsidian::mkdir(&params.vault, &params.path).map_err(|e| e.to_string())
    }

    /// Delete a file or empty folder from the Obsidian vault.
    #[tool(name = "obsidian_delete")]
    fn delete(&self, Parameters(params): Parameters<DeleteParams>) -> Result<String, String> {
        let permanent = params.permanent.unwrap_or(false);
        obsidian::delete(&params.vault, &params.path, permanent).map_err(|e| e.to_string())
    }
}

#[tool_handler]
impl ServerHandler for ObsidianTools {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
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
            "Obsidian vault tools mirroring Claude Code's Read/Edit/Write/Glob/Grep/LS interface.{hide_secret}"
        ).into());
        info
    }
}
