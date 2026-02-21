use std::path::PathBuf;

use crate::git::model::WorktreeEntry;

/// Parse `git worktree list --porcelain` output
pub fn parse_worktree_list(output: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;
    let mut is_bare = false;
    let mut is_detached = false;
    let mut is_first = true;

    for line in output.lines() {
        if line.is_empty() {
            // End of entry
            if let Some(path) = current_path.take() {
                entries.push(WorktreeEntry {
                    path,
                    head_ref: current_head.take(),
                    branch: current_branch.take(),
                    is_bare,
                    is_detached,
                    is_main: is_first,
                });
                is_first = false;
                is_bare = false;
                is_detached = false;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            current_head = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            // refs/heads/main -> main
            let branch_name = rest.strip_prefix("refs/heads/").unwrap_or(rest);
            current_branch = Some(branch_name.to_string());
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            is_detached = true;
        }
    }

    // Handle last entry (no trailing newline)
    if let Some(path) = current_path.take() {
        entries.push(WorktreeEntry {
            path,
            head_ref: current_head.take(),
            branch: current_branch.take(),
            is_bare,
            is_detached,
            is_main: is_first,
        });
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list() {
        let output = "\
worktree /home/user/project
HEAD abc1234567890
branch refs/heads/main

worktree /home/user/project-feature
HEAD def5678901234
branch refs/heads/feature/cool

";
        let entries = parse_worktree_list(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, PathBuf::from("/home/user/project"));
        assert_eq!(entries[0].branch, Some("main".to_string()));
        assert!(entries[0].is_main);
        assert!(!entries[0].is_detached);

        assert_eq!(entries[1].path, PathBuf::from("/home/user/project-feature"));
        assert_eq!(entries[1].branch, Some("feature/cool".to_string()));
        assert!(!entries[1].is_main);
    }

    #[test]
    fn test_parse_detached() {
        let output = "\
worktree /home/user/project
HEAD abc1234567890
detached

";
        let entries = parse_worktree_list(output);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_detached);
        assert!(entries[0].branch.is_none());
    }

    #[test]
    fn test_parse_bare() {
        let output = "\
worktree /home/user/project.git
bare

";
        let entries = parse_worktree_list(output);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_bare);
    }

    #[test]
    fn test_parse_empty() {
        let entries = parse_worktree_list("");
        assert!(entries.is_empty());
    }
}
