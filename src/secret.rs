/// Secret masking for Obsidian files.
///
/// When `OBSIDIAN_HIDE_SECRET=true`, highlight syntax (`==text==`) and
/// `[!secret]` callout blocks are replaced with `[SECRET:N]` placeholders.
///
/// This module is stateless: IDs are assigned by occurrence order in the file,
/// and the original content is re-read from the vault on every write/edit.

/// A single masked secret with its original text and assigned ID.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskedSecret {
    /// 1-based index within the file
    pub id: usize,
    /// The full original text including syntax markers (e.g. `==text==` or callout block)
    pub original: String,
}

/// Result of masking a file's content.
#[derive(Debug)]
pub struct MaskResult {
    /// The masked text with `[SECRET:N]` placeholders
    pub masked: String,
    /// The secrets that were extracted, in order
    pub secrets: Vec<MaskedSecret>,
}

/// Extract and mask all secrets from content.
/// Returns the masked text and the list of extracted secrets.
pub fn mask(content: &str) -> MaskResult {
    let mut secrets = Vec::new();
    let mut id_counter = 0;

    // First pass: mask highlights
    let after_highlights = mask_highlights(content, &mut secrets, &mut id_counter);

    // Second pass: mask [!secret] callouts
    let masked = mask_secret_callouts(&after_highlights, &mut secrets, &mut id_counter);

    MaskResult { masked, secrets }
}

/// Expand `[SECRET:N]` placeholders in old_string/new_string using the original file content.
/// Returns an error if the SECRET count changes between old and new.
pub fn expand_edit(
    original_content: &str,
    old_string: &str,
    new_string: &str,
) -> Result<(String, String), String> {
    let file_mask = mask(original_content);

    // Collect SECRET IDs in old and new
    let old_secret_ids = extract_secret_ids(old_string);
    let new_secret_ids = extract_secret_ids(new_string);

    // Validate: same set of IDs in old and new (order may differ)
    let mut old_sorted = old_secret_ids.clone();
    old_sorted.sort();
    let mut new_sorted = new_secret_ids.clone();
    new_sorted.sort();

    if old_sorted != new_sorted {
        return Err(format!(
            "SECRET ID mismatch between old_string {:?} and new_string {:?}. \
             All secret IDs in old_string must appear in new_string and vice versa.",
            old_secret_ids, new_secret_ids
        ));
    }

    // Validate: all referenced IDs exist in the file
    for id in old_secret_ids.iter().chain(new_secret_ids.iter()) {
        if !file_mask.secrets.iter().any(|s| s.id == *id) {
            return Err(format!(
                "[SECRET:{id}] does not exist in this file (file has {} secrets).",
                file_mask.secrets.len()
            ));
        }
    }

    // Expand placeholders in old_string
    let expanded_old = expand_placeholders(old_string, &file_mask.secrets);
    // Expand placeholders in new_string
    let expanded_new = expand_placeholders(new_string, &file_mask.secrets);

    Ok((expanded_old, expanded_new))
}

/// Check if a file contains any secrets.
pub fn has_secrets(content: &str) -> bool {
    // Check for highlights
    if let Some(start) = content.find("==") {
        let after = &content[start + 2..];
        if let Some(end) = after.find("==") {
            if end > 0 {
                return true;
            }
        }
    }
    // Check for [!secret] callouts
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("> [!secret]") || trimmed.starts_with(">[!secret]") {
            return true;
        }
    }
    false
}

/// Validate that content for write contains exactly the same SECRET IDs as the original file.
/// Returns the content with secrets expanded, or an error if IDs don't match.
pub fn expand_write(original_content: &str, write_content: &str) -> Result<String, String> {
    let file_mask = mask(original_content);

    let file_ids: Vec<usize> = file_mask.secrets.iter().map(|s| s.id).collect();
    let content_ids = extract_secret_ids(write_content);

    // Check exact set match (same IDs, possibly different order)
    let mut file_sorted = file_ids.clone();
    file_sorted.sort();
    let mut content_sorted = content_ids.clone();
    content_sorted.sort();

    if file_sorted != content_sorted {
        let missing: Vec<usize> = file_sorted.iter().filter(|id| !content_sorted.contains(id)).copied().collect();
        let extra: Vec<usize> = content_sorted.iter().filter(|id| !file_sorted.contains(id)).copied().collect();
        let mut msg = "SECRET ID mismatch.".to_string();
        if !missing.is_empty() {
            msg.push_str(&format!(" Missing: {:?}.", missing));
        }
        if !extra.is_empty() {
            msg.push_str(&format!(" Unknown: {:?}.", extra));
        }
        return Err(msg);
    }

    Ok(expand_placeholders(write_content, &file_mask.secrets))
}

/// Remove lines containing `[SECRET:N]` from search results.
/// This prevents leaking the fact that a match occurred inside a secret region.
pub fn filter_secret_lines(text: &str) -> String {
    text.lines()
        .filter(|line| !line.contains("[SECRET:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if secret hiding is enabled via environment variable.
pub fn is_enabled() -> bool {
    std::env::var("OBSIDIAN_HIDE_SECRET")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

// --- Internal helpers ---

/// Replace `==text==` with `[SECRET:N]`.
fn mask_highlights(
    text: &str,
    secrets: &mut Vec<MaskedSecret>,
    id_counter: &mut usize,
) -> String {
    let mut result = String::new();
    let mut remaining = text;

    loop {
        match remaining.find("==") {
            None => {
                result.push_str(remaining);
                break;
            }
            Some(start) => {
                result.push_str(&remaining[..start]);
                let after_open = &remaining[start + 2..];
                match after_open.find("==") {
                    None => {
                        // No closing ==, not a highlight
                        result.push_str("==");
                        remaining = after_open;
                    }
                    Some(end) => {
                        let inner = &after_open[..end];
                        if inner.is_empty() {
                            // ==== is not a highlight
                            result.push_str("====");
                            remaining = &after_open[end + 2..];
                        } else {
                            *id_counter += 1;
                            let original = format!("=={inner}==");
                            secrets.push(MaskedSecret {
                                id: *id_counter,
                                original,
                            });
                            result.push_str(&format!("[SECRET:{}]", id_counter));
                            remaining = &after_open[end + 2..];
                        }
                    }
                }
            }
        }
    }

    result
}

/// Replace `[!secret]` callout blocks with `[SECRET:N]`.
fn mask_secret_callouts(
    text: &str,
    secrets: &mut Vec<MaskedSecret>,
    id_counter: &mut usize,
) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("> [!secret]") || trimmed.starts_with(">[!secret]") {
            let mut callout_lines = vec![lines[i]];
            i += 1;
            while i < lines.len() {
                let line = lines[i];
                if line.starts_with('>') {
                    let inner = line.trim_start_matches('>').trim();
                    if inner.starts_with("[!") {
                        break;
                    }
                    callout_lines.push(line);
                    i += 1;
                } else {
                    break;
                }
            }
            *id_counter += 1;
            let original = callout_lines.join("\n");
            secrets.push(MaskedSecret {
                id: *id_counter,
                original,
            });
            result.push(format!("[SECRET:{}]", id_counter));
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    let mut output = result.join("\n");
    if text.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Extract `[SECRET:N]` IDs from text.
fn extract_secret_ids(text: &str) -> Vec<usize> {
    let mut ids = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("[SECRET:") {
        let after = &remaining[start + 8..];
        if let Some(end) = after.find(']') {
            if let Ok(id) = after[..end].parse::<usize>() {
                ids.push(id);
            }
            remaining = &after[end + 1..];
        } else {
            break;
        }
    }
    ids
}

/// Replace `[SECRET:N]` placeholders with original content.
fn expand_placeholders(text: &str, secrets: &[MaskedSecret]) -> String {
    let mut result = text.to_string();
    for secret in secrets {
        let placeholder = format!("[SECRET:{}]", secret.id);
        result = result.replace(&placeholder, &secret.original);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // === mask ===

    #[test]
    fn test_mask_highlight_simple() {
        let result = mask("before ==secret text== after");
        assert_eq!(result.masked, "before [SECRET:1] after");
        assert_eq!(result.secrets.len(), 1);
        assert_eq!(result.secrets[0].original, "==secret text==");
        assert_eq!(result.secrets[0].id, 1);
    }

    #[test]
    fn test_mask_highlight_multiple() {
        let result = mask("==first== and ==second==");
        assert_eq!(result.masked, "[SECRET:1] and [SECRET:2]");
        assert_eq!(result.secrets.len(), 2);
        assert_eq!(result.secrets[0].original, "==first==");
        assert_eq!(result.secrets[1].original, "==second==");
    }

    #[test]
    fn test_mask_no_secrets() {
        let result = mask("normal text");
        assert_eq!(result.masked, "normal text");
        assert!(result.secrets.is_empty());
    }

    #[test]
    fn test_mask_unclosed_highlight() {
        let result = mask("before ==unclosed");
        assert_eq!(result.masked, "before ==unclosed");
        assert!(result.secrets.is_empty());
    }

    #[test]
    fn test_mask_empty_highlight() {
        let result = mask("before ==== after");
        assert_eq!(result.masked, "before ==== after");
        assert!(result.secrets.is_empty());
    }

    #[test]
    fn test_mask_secret_callout() {
        let input = "before\n> [!secret]\n> confidential\n> more\nafter";
        let result = mask(input);
        assert_eq!(result.masked, "before\n[SECRET:1]\nafter");
        assert_eq!(result.secrets.len(), 1);
        assert_eq!(
            result.secrets[0].original,
            "> [!secret]\n> confidential\n> more"
        );
    }

    #[test]
    fn test_mask_callout_no_space() {
        let result = mask(">[!secret]\n> hidden");
        assert_eq!(result.masked, "[SECRET:1]");
        assert_eq!(result.secrets[0].original, ">[!secret]\n> hidden");
    }

    #[test]
    fn test_mask_non_secret_callout_untouched() {
        let input = "> [!info]\n> public info";
        let result = mask(input);
        assert_eq!(result.masked, input);
        assert!(result.secrets.is_empty());
    }

    #[test]
    fn test_mask_mixed() {
        let input = "public ==private== text\n> [!secret]\n> hidden\nvisible";
        let result = mask(input);
        assert_eq!(
            result.masked,
            "public [SECRET:1] text\n[SECRET:2]\nvisible"
        );
        assert_eq!(result.secrets.len(), 2);
    }

    // === expand_edit ===

    #[test]
    fn test_expand_edit_preserves_secrets() {
        let original = "line1\n==secret==\nline3";
        let (old, new) = expand_edit(
            original,
            "line1\n[SECRET:1]\nline3",
            "modified\n[SECRET:1]\nline3",
        )
        .unwrap();
        assert_eq!(old, "line1\n==secret==\nline3");
        assert_eq!(new, "modified\n==secret==\nline3");
    }

    #[test]
    fn test_expand_edit_reorder_secrets() {
        let original = "==aaa== and ==bbb==";
        let (_, new) = expand_edit(
            original,
            "[SECRET:1] and [SECRET:2]",
            "[SECRET:2] and [SECRET:1]",
        )
        .unwrap();
        assert_eq!(new, "==bbb== and ==aaa==");
    }

    #[test]
    fn test_expand_edit_reject_secret_removal() {
        let original = "==secret==";
        let result = expand_edit(original, "[SECRET:1]", "replaced");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID mismatch"));
    }

    #[test]
    fn test_expand_edit_reject_secret_addition() {
        let original = "no secrets here";
        let result = expand_edit(original, "no secrets here", "no [SECRET:1] here");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID mismatch"));
    }

    #[test]
    fn test_expand_edit_reject_wrong_id() {
        let original = "==aaa== and ==bbb==";
        // old has [1,2] but new has [1,3] — ID 3 doesn't exist
        let result = expand_edit(
            original,
            "[SECRET:1] and [SECRET:2]",
            "[SECRET:1] and [SECRET:3]",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID mismatch"));
    }

    #[test]
    fn test_expand_edit_no_secrets() {
        let original = "normal text";
        let (old, new) =
            expand_edit(original, "normal text", "modified text").unwrap();
        assert_eq!(old, "normal text");
        assert_eq!(new, "modified text");
    }

    #[test]
    fn test_expand_edit_invalid_id() {
        let original = "==only one==";
        // old=[1], new=[99] → ID mismatch (different sets)
        let result = expand_edit(original, "[SECRET:1]", "[SECRET:99]");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID mismatch"));
    }

    // === expand_write ===

    #[test]
    fn test_expand_write_all_ids_present() {
        let original = "==aaa== and ==bbb==";
        let result = expand_write(original, "[SECRET:1] and [SECRET:2]").unwrap();
        assert_eq!(result, "==aaa== and ==bbb==");
    }

    #[test]
    fn test_expand_write_reordered() {
        let original = "==aaa== and ==bbb==";
        let result = expand_write(original, "[SECRET:2] then [SECRET:1]").unwrap();
        assert_eq!(result, "==bbb== then ==aaa==");
    }

    #[test]
    fn test_expand_write_missing_id() {
        let original = "==aaa== and ==bbb==";
        let result = expand_write(original, "[SECRET:1] only");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing"));
    }

    #[test]
    fn test_expand_write_extra_id() {
        let original = "==aaa==";
        let result = expand_write(original, "[SECRET:1] and [SECRET:2]");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown"));
    }

    #[test]
    fn test_expand_write_no_secrets_in_file() {
        let original = "normal text";
        let result = expand_write(original, "new content").unwrap();
        assert_eq!(result, "new content");
    }

    // === filter_secret_lines ===

    #[test]
    fn test_filter_secret_lines() {
        let input = "normal line\n[SECRET:1] matched here\nanother normal";
        let result = filter_secret_lines(input);
        assert_eq!(result, "normal line\nanother normal");
    }

    #[test]
    fn test_filter_secret_lines_no_secrets() {
        let input = "line1\nline2";
        let result = filter_secret_lines(input);
        assert_eq!(result, input);
    }

    // === grep result filtering (mask + filter pipeline) ===

    #[test]
    fn test_grep_match_inside_highlight_is_hidden() {
        // Simulate search results where a match fell inside a highlight
        let search_output = "file.md:3: public text\nfile.md:5: contains ==keyword match== here\nfile.md:7: another public";
        let masked = mask(search_output).masked;
        let filtered = filter_secret_lines(&masked);
        // The line containing the secret should be gone entirely
        assert!(!filtered.contains("keyword"), "Secret content should not appear");
        assert!(!filtered.contains("[SECRET:"), "SECRET placeholder line should be filtered");
        assert!(filtered.contains("public text"), "Non-secret lines preserved");
        assert!(filtered.contains("another public"), "Non-secret lines preserved");
    }

    #[test]
    fn test_grep_match_inside_callout_is_hidden() {
        let search_output = "file.md:1: public\n> [!secret]\n> matches keyword here\nfile.md:10: visible";
        let masked = mask(search_output).masked;
        let filtered = filter_secret_lines(&masked);
        assert!(!filtered.contains("keyword"));
        assert!(!filtered.contains("[SECRET:"));
        assert!(filtered.contains("public"));
        assert!(filtered.contains("visible"));
    }

    #[test]
    fn test_grep_match_outside_secret_is_preserved() {
        let search_output = "file.md:1: keyword here\nfile.md:3: has ==secret== too";
        let masked = mask(search_output).masked;
        let filtered = filter_secret_lines(&masked);
        // Line with keyword (no secret) should be preserved
        assert!(filtered.contains("keyword here"));
        // Line with secret placeholder should be filtered
        assert!(!filtered.contains("[SECRET:"));
    }

    #[test]
    fn test_grep_all_results_in_secrets() {
        let search_output = "file.md:1: ==all secret==\nfile.md:2: ==also secret==";
        let masked = mask(search_output).masked;
        let filtered = filter_secret_lines(&masked);
        assert!(filtered.is_empty() || filtered.trim().is_empty());
    }

    // === expand_edit ID validation ===

    #[test]
    fn test_expand_edit_duplicate_id_in_new() {
        let original = "==aaa== and ==bbb==";
        // old has [1,2], new has [1,1] — different sets
        let result = expand_edit(
            original,
            "[SECRET:1] and [SECRET:2]",
            "[SECRET:1] and [SECRET:1]",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID mismatch"));
    }

    #[test]
    fn test_expand_edit_partial_edit_with_secret_preserved() {
        // Edit only touches non-secret parts
        let original = "header\n==secret==\nfooter";
        let (old, new) = expand_edit(
            original,
            "header\n[SECRET:1]\nfooter",
            "new header\n[SECRET:1]\nnew footer",
        )
        .unwrap();
        assert_eq!(old, "header\n==secret==\nfooter");
        assert_eq!(new, "new header\n==secret==\nnew footer");
    }

    // === expand_write ID validation ===

    #[test]
    fn test_expand_write_duplicate_id() {
        let original = "==aaa== and ==bbb==";
        // has [1,1] instead of [1,2]
        let result = expand_write(original, "[SECRET:1] and [SECRET:1]");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing"));
    }

    #[test]
    fn test_expand_write_swapped_ids() {
        // Same IDs, different order — should work
        let original = "first ==aaa== then ==bbb== end";
        let result = expand_write(original, "first [SECRET:2] then [SECRET:1] end").unwrap();
        assert_eq!(result, "first ==bbb== then ==aaa== end");
    }

    // === has_secrets ===

    #[test]
    fn test_has_secrets_highlight() {
        assert!(has_secrets("text ==secret== here"));
    }

    #[test]
    fn test_has_secrets_callout() {
        assert!(has_secrets("text\n> [!secret]\n> hidden"));
    }

    #[test]
    fn test_has_secrets_none() {
        assert!(!has_secrets("normal text"));
    }

    #[test]
    fn test_has_secrets_empty_highlight() {
        assert!(!has_secrets("===="));
    }

    // === is_enabled ===

    #[test]
    fn test_is_enabled_default_false() {
        std::env::remove_var("OBSIDIAN_HIDE_SECRET");
        assert!(!is_enabled());
    }
}
