use std::path::PathBuf;
use std::process::Command;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ObsidianError {
    #[error("obsidian CLI error: {0}")]
    Cli(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Execute an obsidian CLI command and return stdout.
pub fn run(vault: &str, args: &[&str]) -> Result<String, ObsidianError> {
    let mut cmd = Command::new("obsidian");
    cmd.arg(format!("vault={vault}"));
    for arg in args {
        cmd.arg(*arg);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ObsidianError::Cli(stderr.into_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Apply line-number formatting with optional offset/limit, matching Claude Code's Read output.
pub fn apply_line_range(content: &str, offset: Option<usize>, limit: Option<usize>) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = offset.map(|o| o.saturating_sub(1)).unwrap_or(0);
    let end = limit
        .map(|l| (start + l).min(lines.len()))
        .unwrap_or(lines.len());

    if start >= lines.len() {
        return String::new();
    }

    lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{}\t{}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get the filesystem path of a vault.
pub fn vault_path(vault: &str) -> Result<PathBuf, ObsidianError> {
    let output = run(vault, &["vault", "info=path"])?;
    Ok(PathBuf::from(output.trim()))
}

/// Create a directory in the vault.
/// The obsidian CLI has no mkdir command, so this operates on the filesystem directly.
pub fn mkdir(vault: &str, path: &str) -> Result<String, ObsidianError> {
    let vault_root = vault_path(vault)?;
    let dir_path = vault_root.join(path);

    // Safety: ensure the path is inside the vault
    // We can't canonicalize yet because it doesn't exist, so check the parent
    let parent = dir_path
        .parent()
        .ok_or_else(|| ObsidianError::Cli("invalid path".to_string()))?;

    // Ensure parent exists and is within vault
    if parent.exists() {
        let canonical_vault = vault_root
            .canonicalize()
            .map_err(|e| ObsidianError::Cli(format!("cannot resolve vault path: {e}")))?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| ObsidianError::Cli(format!("cannot resolve parent path: {e}")))?;
        if !canonical_parent.starts_with(&canonical_vault) {
            return Err(ObsidianError::Cli(
                "path is outside the vault".to_string(),
            ));
        }
    }

    std::fs::create_dir_all(&dir_path)
        .map_err(|e| ObsidianError::Cli(format!("cannot create directory: {e}")))?;

    Ok(format!("Created directory: {path}"))
}

/// Delete a file or folder from the vault.
/// Files are deleted via `obsidian delete`. Folders are deleted via filesystem
/// because the obsidian CLI does not support folder deletion.
pub fn delete(vault: &str, path: &str, permanent: bool) -> Result<String, ObsidianError> {
    // First try as a file
    let path_arg = format!("path={path}");
    let mut args = vec!["delete", &path_arg];
    if permanent {
        args.push("permanent");
    }
    match run(vault, &args) {
        Ok(output) if output.contains("is a folder") => {
            // obsidian CLI returns exit 0 but prints error to stdout for folders
            // Fall through to folder deletion
        }
        Ok(output) => return Ok(output),
        Err(ObsidianError::Cli(ref msg)) if msg.contains("is a folder") => {
            // Fall through to folder deletion
        }
        Err(e) => return Err(e),
    }

    // It's a folder — resolve vault path and remove via filesystem
    let vault_root = vault_path(vault)?;
    let folder_path = vault_root.join(path);

    // Safety: ensure the folder is inside the vault
    let canonical_vault = vault_root
        .canonicalize()
        .map_err(|e| ObsidianError::Cli(format!("cannot resolve vault path: {e}")))?;
    let canonical_folder = folder_path
        .canonicalize()
        .map_err(|e| ObsidianError::Cli(format!("cannot resolve folder path: {e}")))?;
    if !canonical_folder.starts_with(&canonical_vault) {
        return Err(ObsidianError::Cli(
            "folder path is outside the vault".to_string(),
        ));
    }

    // Only delete empty folders for safety (ignore system files like .DS_Store)
    let significant_entries: Vec<_> = std::fs::read_dir(&canonical_folder)
        .map_err(|e| ObsidianError::Cli(format!("cannot read folder: {e}")))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            // Ignore macOS/system metadata files
            name != ".DS_Store" && !name.starts_with("._")
        })
        .collect();
    if !significant_entries.is_empty() {
        return Err(ObsidianError::Cli(format!(
            "folder is not empty ({} entries). Delete contents first.",
            significant_entries.len()
        )));
    }

    // Remove system files before removing the folder
    if let Ok(entries) = std::fs::read_dir(&canonical_folder) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    std::fs::remove_dir(&canonical_folder)
        .map_err(|e| ObsidianError::Cli(format!("cannot remove folder: {e}")))?;

    Ok(format!("Deleted folder: {path}"))
}

/// Perform string replacement in content. Returns error if old_string is not found or not unique (when replace_all is false).
pub fn replace_content(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Result<String, String> {
    let count = content.matches(old_string).count();
    if count == 0 {
        return Err("old_string not found in file".to_string());
    }
    if !replace_all && count > 1 {
        return Err(format!(
            "old_string found {count} times; must be unique. Provide more context or use replace_all."
        ));
    }

    if replace_all {
        Ok(content.replace(old_string, new_string))
    } else {
        Ok(content.replacen(old_string, new_string, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === apply_line_range tests ===

    #[test]
    fn test_apply_line_range_full_content() {
        let content = "line1\nline2\nline3";
        let result = apply_line_range(content, None, None);
        assert_eq!(result, "1\tline1\n2\tline2\n3\tline3");
    }

    #[test]
    fn test_apply_line_range_with_offset() {
        let content = "line1\nline2\nline3";
        let result = apply_line_range(content, Some(2), None);
        assert_eq!(result, "2\tline2\n3\tline3");
    }

    #[test]
    fn test_apply_line_range_with_limit() {
        let content = "line1\nline2\nline3";
        let result = apply_line_range(content, None, Some(2));
        assert_eq!(result, "1\tline1\n2\tline2");
    }

    #[test]
    fn test_apply_line_range_with_offset_and_limit() {
        let content = "line1\nline2\nline3\nline4";
        let result = apply_line_range(content, Some(2), Some(2));
        assert_eq!(result, "2\tline2\n3\tline3");
    }

    #[test]
    fn test_apply_line_range_offset_beyond_content() {
        let content = "line1\nline2";
        let result = apply_line_range(content, Some(10), None);
        assert_eq!(result, "");
    }

    #[test]
    fn test_apply_line_range_empty_content() {
        let result = apply_line_range("", None, None);
        assert_eq!(result, "");
    }

    // === replace_content tests ===

    #[test]
    fn test_replace_content_single_match() {
        let result = replace_content("hello world", "world", "rust", false).unwrap();
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_replace_content_not_found() {
        let result = replace_content("hello world", "xyz", "rust", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_replace_content_multiple_matches_no_replace_all() {
        let result = replace_content("aaa", "a", "b", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("3 times"));
    }

    #[test]
    fn test_replace_content_multiple_matches_with_replace_all() {
        let result = replace_content("aaa", "a", "b", true).unwrap();
        assert_eq!(result, "bbb");
    }

    #[test]
    fn test_replace_content_multiline() {
        let content = "line1\nold text\nline3";
        #[allow(unused_variables)]
        let result = replace_content(content, "old text", "new text", false).unwrap();
        assert_eq!(result, "line1\nnew text\nline3");
    }
}
