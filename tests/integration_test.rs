use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn init_repo(dir: &Path) {
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "test@test.com"]);
    run_git(dir, &["config", "user.name", "Test"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);
    run_git(dir, &["config", "tag.gpgsign", "false"]);
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("Failed to run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn write_file(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

#[test]
fn test_repo_detection() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    let output = run_git(dir.path(), &["rev-parse", "--show-toplevel"]);
    assert!(!output.trim().is_empty());
}

#[test]
fn test_unborn_head() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    // No commits => HEAD is unborn
    let result = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(!result.status.success()); // Should fail on unborn HEAD
}

#[test]
fn test_changed_files_unstaged() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    // Initial commit
    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    // Modify file (unstaged)
    write_file(dir.path(), "file1.txt", "hello world\n");

    let output = run_git(dir.path(), &["diff", "--name-status"]);
    assert!(output.contains("file1.txt"));
    assert!(output.starts_with("M"));
}

#[test]
fn test_changed_files_staged() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    // Modify and stage
    write_file(dir.path(), "file1.txt", "hello world\n");
    run_git(dir.path(), &["add", "file1.txt"]);

    let output = run_git(dir.path(), &["diff", "--cached", "--name-status"]);
    assert!(output.contains("file1.txt"));
}

#[test]
fn test_compare_mode() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    // Get the default branch name
    let default_branch = run_git(dir.path(), &["branch", "--show-current"]);
    let default_branch = default_branch.trim();

    // Create branch and make changes
    run_git(dir.path(), &["checkout", "-b", "feature"]);
    write_file(dir.path(), "file2.txt", "new feature\n");
    run_git(dir.path(), &["add", "file2.txt"]);
    run_git(dir.path(), &["commit", "-m", "feature"]);

    // Compare default_branch...feature
    let range = format!("{}...feature", default_branch);
    let output = run_git(dir.path(), &["diff", &range, "--name-status"]);
    assert!(output.contains("file2.txt"));
}

#[test]
fn test_diff_output() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "line1\nline2\nline3\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    write_file(dir.path(), "file1.txt", "line1\nmodified\nline3\n");

    let output = run_git(
        dir.path(),
        &["diff", "--patch", "--unified=3", "--", "file1.txt"],
    );
    assert!(output.contains("-line2"));
    assert!(output.contains("+modified"));
    assert!(output.contains("@@"));
}

#[test]
fn test_binary_file_diff() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    // Create a binary file
    std::fs::write(dir.path().join("binary.bin"), [0u8, 1, 2, 3, 255, 254]).unwrap();
    run_git(dir.path(), &["add", "binary.bin"]);
    run_git(dir.path(), &["commit", "-m", "add binary"]);

    // Modify binary
    std::fs::write(dir.path().join("binary.bin"), [4u8, 5, 6, 7, 255]).unwrap();

    let output = run_git(dir.path(), &["diff", "--patch", "--", "binary.bin"]);
    assert!(output.contains("Binary files"));
}

#[test]
fn test_rename_detection() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "old_name.txt", "content here\n");
    run_git(dir.path(), &["add", "old_name.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    run_git(dir.path(), &["mv", "old_name.txt", "new_name.txt"]);

    let output = run_git(dir.path(), &["diff", "--cached", "--name-status", "-M"]);
    assert!(output.contains("R"));
    assert!(output.contains("new_name.txt"));
}

#[test]
fn test_worktree_list() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    let output = run_git(dir.path(), &["worktree", "list", "--porcelain"]);
    assert!(output.contains("worktree "));
    assert!(output.contains("branch refs/heads/"));
}

#[test]
fn test_refs_listing() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    // Create a branch
    run_git(dir.path(), &["branch", "test-branch"]);

    let output = run_git(
        dir.path(),
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    );
    assert!(output.contains("test-branch"));
}

#[test]
fn test_new_file_diff() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    // Add new file
    write_file(dir.path(), "new_file.txt", "new content\n");
    run_git(dir.path(), &["add", "new_file.txt"]);

    let output = run_git(dir.path(), &["diff", "--cached", "--name-status"]);
    assert!(output.contains("A"));
    assert!(output.contains("new_file.txt"));
}

#[test]
fn test_deleted_file_diff() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "hello\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    // Delete file
    run_git(dir.path(), &["rm", "file1.txt"]);

    let output = run_git(dir.path(), &["diff", "--cached", "--name-status"]);
    assert!(output.contains("D"));
    assert!(output.contains("file1.txt"));
}

#[test]
fn test_numstat() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    write_file(dir.path(), "file1.txt", "line1\nline2\nline3\n");
    run_git(dir.path(), &["add", "file1.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);

    write_file(
        dir.path(),
        "file1.txt",
        "line1\nmodified\nline3\nnew_line\n",
    );

    let output = run_git(dir.path(), &["diff", "--numstat"]);
    assert!(!output.trim().is_empty());
    // Should show additions and deletions
    let parts: Vec<&str> = output.trim().split('\t').collect();
    assert!(parts.len() >= 3);
}

// CLI flag tests
#[test]
fn test_print_default_config_flag() {
    let result = Command::new(env!("CARGO_BIN_EXE_git-review-tui"))
        .arg("--print-default-config")
        .output()
        .unwrap();

    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("default_base"));
    assert!(stdout.contains("unified_context"));
    assert!(stdout.contains("watch"));
}

#[test]
fn test_print_keys_flag() {
    let result = Command::new(env!("CARGO_BIN_EXE_git-review-tui"))
        .arg("--print-keys")
        .output()
        .unwrap();

    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("Navigation:"));
    assert!(stdout.contains("j/Down"));
    assert!(stdout.contains("Selectors:"));
    assert!(stdout.contains("q/Esc"));
}

#[test]
fn test_version_flag() {
    let result = Command::new(env!("CARGO_BIN_EXE_git-review-tui"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("git-review-tui"));
}
