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

/// Resolve vault name from explicit parameter or `OBSIDIAN_VAULT` environment variable.
pub fn resolve_vault(vault: Option<String>) -> Result<String, ObsidianError> {
    if let Some(v) = vault {
        return Ok(v);
    }
    std::env::var("OBSIDIAN_VAULT").map_err(|_| {
        ObsidianError::Cli(
            "vault not specified and OBSIDIAN_VAULT environment variable is not set".to_string(),
        )
    })
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

/// Delete a file from the vault via `obsidian delete`.
pub fn delete_file(vault: &str, path: &str, permanent: bool) -> Result<String, ObsidianError> {
    let path_arg = format!("path={path}");
    let mut args = vec!["delete", &path_arg];
    if permanent {
        args.push("permanent");
    }
    run(vault, &args)
}

/// Delete an empty folder from the vault via filesystem.
/// The obsidian CLI does not support folder deletion.
pub fn delete_folder(vault: &str, path: &str) -> Result<String, ObsidianError> {
    let vault_root = vault_path(vault)?;
    let folder_path = vault_root.join(path);

    // If folder doesn't exist, treat as success (idempotent)
    if !folder_path.exists() {
        return Ok(format!("Deleted folder: {path}"));
    }

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

/// Match files and folders against a glob pattern.
/// Folders are matched with a trailing `/` appended (e.g. `notes/`),
/// mirroring how Claude Code's Glob treats directories.
pub fn glob_match_entries(files_output: &str, folders_output: &str, pattern: &str, path: Option<&str>) -> Vec<String> {
    let prefix = path.map(|p| p.trim_end_matches('/'));

    let in_scope = |entry: &str| -> bool {
        match prefix {
            Some(p) => entry.starts_with(p) && entry[p.len()..].starts_with('/'),
            None => true,
        }
    };

    let mut matched: Vec<String> = Vec::new();

    for line in files_output.lines() {
        if !line.is_empty() && in_scope(line) && glob_match::glob_match(pattern, line) {
            matched.push(line.to_string());
        }
    }

    for line in folders_output.lines() {
        if !line.is_empty() {
            let with_slash = format!("{line}/");
            let matches_pattern = glob_match::glob_match(pattern, &with_slash);
            if matches_pattern && (in_scope(&with_slash) || prefix == Some(line)) {
                matched.push(with_slash);
            }
        }
    }

    matched.sort();
    matched
}

/// Parse a Markdown table of folder descriptions.
/// Expected format: `| path/ | description |` rows in a Markdown table.
/// Header rows and separator rows (`| --- | --- |`) are skipped.
pub fn parse_folder_descriptions(md: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in md.lines() {
        let trimmed = line.trim();
        // Only process lines that look like table rows
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .splitn(2, '|')
            .map(|s| s.trim())
            .collect();
        if cells.len() != 2 {
            continue;
        }
        let path = cells[0];
        let desc = cells[1];
        // Skip separator rows (e.g. "---", "------") and header rows
        if path.is_empty() || desc.is_empty() || path.chars().all(|c| c == '-' || c == ' ') {
            continue;
        }
        // Only accept paths ending with '/' (directories)
        if path.ends_with('/') {
            map.insert(path.to_string(), desc.to_string());
        }
    }
    map
}

/// Annotate glob entries with folder descriptions where available.
/// Directories with a matching description get `path/\tdescription`, others remain unchanged.
pub fn annotate_entries(entries: &[String], descriptions: &std::collections::HashMap<String, String>) -> Vec<String> {
    entries
        .iter()
        .map(|entry| {
            if let Some(desc) = descriptions.get(entry.as_str()) {
                format!("{entry}\t{desc}")
            } else {
                entry.clone()
            }
        })
        .collect()
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

    // === glob_match_entries tests ===

    #[test]
    fn test_glob_match_entries_files_only() {
        let files = "notes/hello.md\nnotes/world.md\nREADME.md";
        let folders = "notes\narchive";
        let result = glob_match_entries(files, folders, "**/*.md", None);
        assert_eq!(result, vec!["README.md", "notes/hello.md", "notes/world.md"]);
    }

    #[test]
    fn test_glob_match_entries_directories_only() {
        let files = "notes/hello.md\nREADME.md";
        let folders = "notes\narchive\narchive/2024";
        let result = glob_match_entries(files, folders, "**/", None);
        assert_eq!(result, vec!["archive/", "archive/2024/", "notes/"]);
    }

    #[test]
    fn test_glob_match_entries_mixed() {
        let files = "notes/hello.md\nREADME.md";
        let folders = "notes";
        let result = glob_match_entries(files, folders, "notes/**", None);
        assert!(result.contains(&"notes/hello.md".to_string()));
    }

    #[test]
    fn test_glob_match_entries_no_match() {
        let files = "notes/hello.md";
        let folders = "notes";
        let result = glob_match_entries(files, folders, "nonexistent/**", None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_glob_match_entries_with_path() {
        let files = "src/main.rs\nsrc/lib.rs\ntests/integration.rs\nREADME.md";
        let folders = "src\ntests";
        let result = glob_match_entries(files, folders, "**/*.rs", Some("src"));
        assert_eq!(result, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_glob_match_entries_with_path_trailing_slash() {
        let files = "src/main.rs\nsrc/lib.rs\ntests/integration.rs";
        let folders = "src\ntests";
        // path with trailing slash should work the same
        let result = glob_match_entries(files, folders, "**/*.rs", Some("src/"));
        assert_eq!(result, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_glob_match_entries_with_path_folders() {
        let files = "src/a.rs\nsrc/sub/b.rs";
        let folders = "src\nsrc/sub\ntests";
        let result = glob_match_entries(files, folders, "**/", Some("src"));
        assert_eq!(result, vec!["src/", "src/sub/"]);
    }

    // === parse_folder_descriptions / annotate_entries tests ===

    #[test]
    fn test_parse_folder_descriptions() {
        let md = "| パス | 説明 |\n|------|------|\n| _diary/ | 日々のメモ |\n| ふりかえり/ | 定期的な反省 |\n";
        let map = parse_folder_descriptions(md);
        assert_eq!(map.get("_diary/").map(|s| s.as_str()), Some("日々のメモ"));
        assert_eq!(map.get("ふりかえり/").map(|s| s.as_str()), Some("定期的な反省"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_parse_folder_descriptions_ignores_non_table_lines() {
        let md = "# フォルダ説明\n\n| パス | 説明 |\n|------|------|\n| _diary/ | 日々のメモ |\n\nこれは普通のテキスト\n";
        let map = parse_folder_descriptions(md);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("_diary/").map(|s| s.as_str()), Some("日々のメモ"));
    }

    #[test]
    fn test_parse_folder_descriptions_ignores_header_and_separator() {
        let md = "| パス | 説明 |\n| --- | --- |\n| 指針/ | 価値観 |\n";
        let map = parse_folder_descriptions(md);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("指針/").map(|s| s.as_str()), Some("価値観"));
    }

    #[test]
    fn test_annotate_entries_with_descriptions() {
        let entries = vec![
            "_diary/".to_string(),
            "ふりかえり/".to_string(),
            "ふりかえり/四半期/".to_string(),
            "TODO.md".to_string(),
        ];
        let mut descs = std::collections::HashMap::new();
        descs.insert("_diary/".to_string(), "日々のメモ".to_string());
        descs.insert("ふりかえり/".to_string(), "定期的な反省".to_string());

        let result = annotate_entries(&entries, &descs);
        assert_eq!(result[0], "_diary/\t日々のメモ");
        assert_eq!(result[1], "ふりかえり/\t定期的な反省");
        assert_eq!(result[2], "ふりかえり/四半期/");
        assert_eq!(result[3], "TODO.md");
    }

    #[test]
    fn test_annotate_entries_empty_descriptions() {
        let entries = vec!["_diary/".to_string(), "TODO.md".to_string()];
        let descs = std::collections::HashMap::new();
        let result = annotate_entries(&entries, &descs);
        assert_eq!(result[0], "_diary/");
        assert_eq!(result[1], "TODO.md");
    }

    // === resolve_vault tests ===

    #[test]
    fn test_resolve_vault_explicit_value() {
        let result = resolve_vault(Some("my-vault".to_string())).unwrap();
        assert_eq!(result, "my-vault");
    }

    #[test]
    fn test_resolve_vault_from_env() {
        std::env::set_var("OBSIDIAN_VAULT", "env-vault");
        let result = resolve_vault(None).unwrap();
        assert_eq!(result, "env-vault");
        std::env::remove_var("OBSIDIAN_VAULT");
    }

    #[test]
    fn test_resolve_vault_explicit_overrides_env() {
        std::env::set_var("OBSIDIAN_VAULT", "env-vault");
        let result = resolve_vault(Some("explicit-vault".to_string())).unwrap();
        assert_eq!(result, "explicit-vault");
        std::env::remove_var("OBSIDIAN_VAULT");
    }

    #[test]
    fn test_resolve_vault_none_without_env_is_error() {
        std::env::remove_var("OBSIDIAN_VAULT");
        let result = resolve_vault(None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("OBSIDIAN_VAULT"), "Error should mention env var: {err}");
    }
}
