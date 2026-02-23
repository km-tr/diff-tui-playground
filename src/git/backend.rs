use std::path::Path;

use anyhow::Result;

use super::model::*;

/// Trait for Git operations - allows mocking in tests
pub trait GitBackend: Send {
    /// Resolve the repository root from a path (via `--show-toplevel`)
    fn repo_root(&self, path: &Path) -> Result<std::path::PathBuf>;

    /// Resolve the git common directory (via `--git-common-dir`).
    /// For linked worktrees this returns the main repo's `.git` directory;
    /// its parent is the true repository root.
    fn git_common_dir(&self, repo: &Path) -> Result<std::path::PathBuf>;

    /// Get the current branch name (None if detached or unborn)
    fn current_branch(&self, repo: &Path) -> Result<Option<String>>;

    /// Get HEAD ref (short hash if detached)
    fn head_ref(&self, repo: &Path) -> Result<Option<String>>;

    /// Check if HEAD is unborn (no commits yet)
    fn is_unborn(&self, repo: &Path) -> Result<bool>;

    /// Check if HEAD is detached
    fn is_detached(&self, repo: &Path) -> Result<bool>;

    /// Get the list of changed files for a given diff spec
    fn changed_files(&self, repo: &Path, spec: &DiffSpec) -> Result<Vec<FileEntry>>;

    /// Get the full diff for a specific file
    fn file_diff(
        &self,
        repo: &Path,
        spec: &DiffSpec,
        file_path: &str,
        context_lines: u32,
    ) -> Result<FileDiff>;

    /// Get the list of refs (branches + tags) for the selector
    fn list_refs(&self, repo: &Path) -> Result<Vec<RefEntry>>;

    /// Get the list of worktrees
    fn list_worktrees(&self, repo: &Path) -> Result<Vec<WorktreeEntry>>;

    /// Find the merge-base between two refs
    fn merge_base(&self, repo: &Path, ref1: &str, ref2: &str) -> Result<String>;

    /// Try to find a suitable default base ref from a list of candidates
    fn find_default_base(&self, repo: &Path, candidates: &[String]) -> Option<String>;
}
