//! Integration tests against a real Obsidian vault.
//!
//! These tests require:
//! - Obsidian to be running
//! - The `obsidian` CLI to be available
//! - Set `OBSIDIAN_TEST_VAULT` env var to a vault name (e.g. `OBSIDIAN_TEST_VAULT=my-vault cargo test --test integration`)
//!
//! Run with: OBSIDIAN_TEST_VAULT=<name> cargo test --test integration -- --test-threads=1
//! Skip with: cargo test --lib

use obsidian_mcp::obsidian;

const TEST_DIR: &str = "_test_obsidian_mcp";

fn vault() -> String {
    std::env::var("OBSIDIAN_TEST_VAULT")
        .expect("Set OBSIDIAN_TEST_VAULT env var to run integration tests")
}

// ============================================================
// Safe read-only tests (no vault modifications)
// ============================================================

#[test]
fn test_ls_vault_root() {
    let vault = vault();
    let result = obsidian::run(&vault, &["folders"]);
    assert!(result.is_ok(), "Failed to list folders: {:?}", result.err());
}

#[test]
fn test_files_list() {
    let vault = vault();
    let result = obsidian::run(&vault, &["files"]);
    assert!(result.is_ok());
    let output = result.unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert!(!lines.is_empty(), "Vault should have at least one file");
}

#[test]
fn test_glob_md_files() {
    let vault = vault();
    let files_output = obsidian::run(&vault, &["files"]).unwrap();
    let folders_output = obsidian::run(&vault, &["folders"]).unwrap();
    let matched = obsidian::glob_match_entries(&files_output, &folders_output, "**/*.md", None);
    assert!(!matched.is_empty(), "Vault should contain at least one .md file");
    assert!(
        matched.iter().all(|f| f.ends_with(".md")),
        "All matched files should end with .md"
    );
}

#[test]
fn test_glob_no_match() {
    let vault = vault();
    let files_output = obsidian::run(&vault, &["files"]).unwrap();
    let folders_output = obsidian::run(&vault, &["folders"]).unwrap();
    let matched = obsidian::glob_match_entries(&files_output, &folders_output, "nonexistent_folder_xyz/**/*.md", None);
    assert!(matched.is_empty(), "Should match nothing for nonexistent folder");
}

#[test]
fn test_glob_directories() {
    let vault = vault();
    let files_output = obsidian::run(&vault, &["files"]).unwrap();
    let folders_output = obsidian::run(&vault, &["folders"]).unwrap();
    let matched = obsidian::glob_match_entries(&files_output, &folders_output, "**/", None);
    assert!(!matched.is_empty(), "Vault should contain at least one directory");
    assert!(
        matched.iter().all(|d| d.ends_with('/')),
        "All matched directories should end with /"
    );
}

#[test]
fn test_glob_with_path() {
    let vault = vault();
    let test_path = format!("{TEST_DIR}/glob_path_test.md");
    obsidian::run(&vault, &["create", &format!("path={test_path}"), "content=glob path test", "overwrite"])
        .expect("Create should succeed");

    let files_output = obsidian::run(&vault, &["files"]).unwrap();
    let folders_output = obsidian::run(&vault, &["folders"]).unwrap();
    let matched = obsidian::glob_match_entries(&files_output, &folders_output, "**/*.md", Some(TEST_DIR));
    assert!(!matched.is_empty(), "Should find .md files under test dir");
    assert!(
        matched.iter().all(|f| f.starts_with(TEST_DIR)),
        "All results should be under {TEST_DIR}"
    );

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={test_path}"), "permanent"])
        .expect("Cleanup should succeed");
}

#[test]
fn test_read_existing_file() {
    let vault = vault();
    // Use a self-created file to avoid depending on vault contents
    let test_path = format!("{TEST_DIR}/read_test.md");
    obsidian::run(&vault, &["create", &format!("path={test_path}"), "content=read test content", "overwrite"])
        .expect("Create should succeed");

    let content = obsidian::run(&vault, &["read", &format!("path={test_path}")]);
    assert!(content.is_ok(), "Should be able to read file: {:?}", content.err());
    assert!(content.unwrap().contains("read test content"));

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={test_path}"), "permanent"])
        .expect("Cleanup should succeed");
}

#[test]
fn test_read_with_line_range() {
    let vault = vault();
    let test_path = format!("{TEST_DIR}/line_range_test.md");
    obsidian::run(&vault, &["create", &format!("path={test_path}"), "content=line1\nline2\nline3\nline4\nline5", "overwrite"])
        .expect("Create should succeed");

    let content = obsidian::run(&vault, &["read", &format!("path={test_path}")]).unwrap();
    let ranged = obsidian::apply_line_range(&content, Some(2), Some(3));

    let lines: Vec<&str> = ranged.lines().collect();
    assert_eq!(lines.len(), 3, "Should return 3 lines");
    assert!(lines[0].starts_with("2\t"), "First line should start with '2\\t'");

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={test_path}"), "permanent"])
        .expect("Cleanup should succeed");
}

#[test]
fn test_search() {
    let vault = vault();
    // Create a file with known content to search for
    let test_path = format!("{TEST_DIR}/search_test.md");
    obsidian::run(&vault, &["create", &format!("path={test_path}"), "content=unique_search_term_xyz123", "overwrite"])
        .expect("Create should succeed");

    let result = obsidian::run(&vault, &["search", "query=unique_search_term_xyz123", "limit=5"]);
    assert!(result.is_ok(), "Search should succeed: {:?}", result.err());

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={test_path}"), "permanent"])
        .expect("Cleanup should succeed");
}

// ============================================================
// Write tests (create temp files, verify, clean up)
// ============================================================

#[test]
fn test_write_read_delete() {
    let vault = vault();
    let test_path = format!("{TEST_DIR}/write_test.md");
    let test_content = "# Write Test\n\nThis is a test file created by obsidian-mcp integration tests.";

    // Write
    let write_result = obsidian::run(
        &vault,
        &["create", &format!("path={test_path}"), &format!("content={test_content}"), "overwrite"],
    );
    assert!(write_result.is_ok(), "Write failed: {:?}", write_result.err());

    // Read back
    let read_result = obsidian::run(&vault, &["read", &format!("path={test_path}")]);
    assert!(read_result.is_ok(), "Read failed: {:?}", read_result.err());
    let read_content = read_result.unwrap();
    assert!(
        read_content.contains("Write Test"),
        "Read content should contain 'Write Test', got: {read_content}"
    );

    // Clean up
    let delete_result = obsidian::run(&vault, &["delete", &format!("path={test_path}"), "permanent"]);
    assert!(delete_result.is_ok(), "Delete failed: {:?}", delete_result.err());
}

#[test]
fn test_edit_via_replace_content() {
    let vault = vault();
    let test_path = format!("{TEST_DIR}/edit_test.md");
    let original = "# Edit Test\n\nOriginal content here.\n\nKeep this line.";

    // Create file
    obsidian::run(
        &vault,
        &["create", &format!("path={test_path}"), &format!("content={original}"), "overwrite"],
    )
    .expect("Create should succeed");

    // Read
    let content = obsidian::run(&vault, &["read", &format!("path={test_path}")])
        .expect("Read should succeed");

    // Replace
    let new_content = obsidian::replace_content(&content, "Original content here.", "Modified content here.", false)
        .expect("Replace should succeed");

    // Write back
    obsidian::run(
        &vault,
        &["create", &format!("path={test_path}"), &format!("content={new_content}"), "overwrite"],
    )
    .expect("Overwrite should succeed");

    // Verify
    let verified = obsidian::run(&vault, &["read", &format!("path={test_path}")])
        .expect("Read should succeed");
    assert!(verified.contains("Modified content here."), "Should contain modified text");
    assert!(verified.contains("Keep this line."), "Should preserve other content");
    assert!(!verified.contains("Original content here."), "Should not contain original text");

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={test_path}"), "permanent"])
        .expect("Delete should succeed");
}

// ============================================================
// Delete tests (file + folder)
// ============================================================

#[test]
fn test_delete_file() {
    let vault = vault();
    let test_path = format!("{TEST_DIR}/delete_file_test.md");
    obsidian::run(&vault, &["create", &format!("path={test_path}"), "content=temp", "overwrite"])
        .expect("Create should succeed");

    let result = obsidian::delete_file(&vault, &test_path, true);
    assert!(result.is_ok(), "Delete file failed: {:?}", result.err());
}

#[test]
fn test_delete_empty_folder() {
    let vault = vault();
    let folder = format!("{TEST_DIR}/empty_folder_test");
    let file_path = format!("{folder}/temp.md");

    // Create a file to ensure the folder exists
    obsidian::run(&vault, &["create", &format!("path={file_path}"), "content=temp", "overwrite"])
        .expect("Create should succeed");

    // Delete the file, leaving an empty folder
    obsidian::run(&vault, &["delete", &format!("path={file_path}"), "permanent"])
        .expect("File delete should succeed");

    // Now delete the empty folder
    let result = obsidian::delete_folder(&vault, &folder);
    assert!(result.is_ok(), "Delete empty folder failed: {:?}", result.err());
}

#[test]
fn test_delete_nonempty_folder_fails() {
    let vault = vault();
    let folder = format!("{TEST_DIR}/nonempty_folder_test");
    let file_path = format!("{folder}/keep.md");

    obsidian::run(&vault, &["create", &format!("path={file_path}"), "content=keep", "overwrite"])
        .expect("Create should succeed");

    // Deleting non-empty folder should fail
    let result = obsidian::delete_folder(&vault, &folder);
    assert!(result.is_err(), "Should fail to delete non-empty folder");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not empty"), "Error should mention 'not empty': {err}");

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={file_path}"), "permanent"])
        .expect("Cleanup file delete should succeed");
    obsidian::delete_folder(&vault, &folder).expect("Cleanup folder delete should succeed");
}

// ============================================================
// Move tests
// ============================================================

#[test]
fn test_move_file() {
    let vault = vault();
    let src = format!("{TEST_DIR}/move_src.md");
    let dst = format!("{TEST_DIR}/move_dst.md");

    obsidian::run(&vault, &["create", &format!("path={src}"), "content=move me", "overwrite"])
        .expect("Create should succeed");

    let result = obsidian::run(&vault, &["move", &format!("path={src}"), &format!("to={dst}")]);
    assert!(result.is_ok(), "Move failed: {:?}", result.err());

    // Verify destination exists
    let content = obsidian::run(&vault, &["read", &format!("path={dst}")]);
    assert!(content.is_ok(), "Should read moved file");
    assert!(content.unwrap().contains("move me"));

    // Clean up
    obsidian::run(&vault, &["delete", &format!("path={dst}"), "permanent"])
        .expect("Cleanup should succeed");
}

// ============================================================
// Mkdir tests
// ============================================================

#[test]
fn test_mkdir_simple() {
    let vault = vault();
    let dir = format!("{TEST_DIR}/mkdir_test");

    let result = obsidian::mkdir(&vault, &dir);
    assert!(result.is_ok(), "Mkdir failed: {:?}", result.err());

    // Verify folder exists via obsidian CLI
    let folder_info = obsidian::run(&vault, &["folder", &format!("path={dir}")]);
    assert!(folder_info.is_ok(), "Folder should exist: {:?}", folder_info.err());

    // Clean up
    obsidian::delete_folder(&vault, &dir).expect("Cleanup should succeed");
}

#[test]
fn test_mkdir_nested() {
    let vault = vault();
    let dir = format!("{TEST_DIR}/mkdir_nested/a/b");

    let result = obsidian::mkdir(&vault, &dir);
    assert!(result.is_ok(), "Nested mkdir failed: {:?}", result.err());

    let folder_info = obsidian::run(&vault, &["folder", &format!("path={dir}")]);
    assert!(folder_info.is_ok(), "Nested folder should exist");

    // Clean up (deepest first)
    obsidian::delete_folder(&vault, &format!("{TEST_DIR}/mkdir_nested/a/b")).unwrap();
    obsidian::delete_folder(&vault, &format!("{TEST_DIR}/mkdir_nested/a")).unwrap();
    obsidian::delete_folder(&vault, &format!("{TEST_DIR}/mkdir_nested")).unwrap();
}

#[test]
fn test_mkdir_already_exists() {
    let vault = vault();
    let dir = format!("{TEST_DIR}/mkdir_exists");

    obsidian::mkdir(&vault, &dir).expect("First mkdir should succeed");
    // Second call should also succeed (create_dir_all is idempotent)
    let result = obsidian::mkdir(&vault, &dir);
    assert!(result.is_ok(), "Mkdir on existing dir should succeed");

    // Clean up
    obsidian::delete_folder(&vault, &dir).expect("Cleanup should succeed");
}

// Clean up test directory recursively
#[test]
fn test_zz_cleanup_test_dir() {
    let vault = vault();
    // This runs last due to alphabetical ordering.
    // obsidian::delete_folder only removes empty folders, so use filesystem directly
    // to clean up nested empty folders left by tests.
    let vault_root = obsidian::vault_path(&vault).expect("vault_path should succeed");
    let test_dir = vault_root.join(TEST_DIR);
    if test_dir.exists() {
        assert!(
            test_dir.starts_with(&vault_root),
            "BUG: test_dir is outside the vault"
        );
        std::fs::remove_dir_all(&test_dir).expect("cleanup test dir should succeed");
    }
}
