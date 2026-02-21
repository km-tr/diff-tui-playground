use crate::git::model::{FileEntry, FileStatus};

/// Parse `git diff --name-status` output
pub fn parse_name_status(output: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry = parse_name_status_line(line);
        if let Some(entry) = entry {
            entries.push(entry);
        }
    }
    entries
}

fn parse_name_status_line(line: &str) -> Option<FileEntry> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.is_empty() {
        return None;
    }

    let status_str = parts[0].trim();
    let status_char = status_str.chars().next()?;
    let status = FileStatus::from_char(status_char);

    match status {
        FileStatus::Renamed | FileStatus::Copied => {
            if parts.len() >= 3 {
                Some(FileEntry {
                    path: parts[2].to_string(),
                    old_path: Some(parts[1].to_string()),
                    status,
                    additions: 0,
                    deletions: 0,
                })
            } else {
                None
            }
        }
        _ => {
            if parts.len() >= 2 {
                Some(FileEntry {
                    path: parts[1].to_string(),
                    old_path: None,
                    status,
                    additions: 0,
                    deletions: 0,
                })
            } else {
                None
            }
        }
    }
}

/// Parse `git diff --numstat` output and merge into existing entries
pub fn merge_numstat(entries: &mut [FileEntry], numstat_output: &str) {
    for line in numstat_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let additions = parts[0].parse::<u32>().unwrap_or(0);
            let deletions = parts[1].parse::<u32>().unwrap_or(0);
            let path = parts[2];

            // For renames, numstat uses the format "old => new" or just the new path
            if let Some(entry) = entries.iter_mut().find(|e| {
                e.path == path
                    || path.contains(" => ")
                        && (path.contains(&e.path)
                            || e.old_path.as_ref().is_some_and(|op| path.contains(op)))
            }) {
                entry.additions = additions;
                entry.deletions = deletions;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_status_basic() {
        let output = "M\tsrc/main.rs\nA\tsrc/new.rs\nD\told.rs\n";
        let entries = parse_name_status(output);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].status, FileStatus::Modified);
        assert_eq!(entries[0].path, "src/main.rs");
        assert_eq!(entries[1].status, FileStatus::Added);
        assert_eq!(entries[1].path, "src/new.rs");
        assert_eq!(entries[2].status, FileStatus::Deleted);
        assert_eq!(entries[2].path, "old.rs");
    }

    #[test]
    fn test_parse_name_status_rename() {
        let output = "R100\told_name.rs\tnew_name.rs\n";
        let entries = parse_name_status(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, FileStatus::Renamed);
        assert_eq!(entries[0].path, "new_name.rs");
        assert_eq!(entries[0].old_path, Some("old_name.rs".to_string()));
    }

    #[test]
    fn test_parse_empty() {
        let entries = parse_name_status("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_merge_numstat() {
        let mut entries = vec![
            FileEntry {
                path: "src/main.rs".into(),
                old_path: None,
                status: FileStatus::Modified,
                additions: 0,
                deletions: 0,
            },
            FileEntry {
                path: "src/new.rs".into(),
                old_path: None,
                status: FileStatus::Added,
                additions: 0,
                deletions: 0,
            },
        ];
        let numstat = "10\t5\tsrc/main.rs\n20\t0\tsrc/new.rs\n";
        merge_numstat(&mut entries, numstat);
        assert_eq!(entries[0].additions, 10);
        assert_eq!(entries[0].deletions, 5);
        assert_eq!(entries[1].additions, 20);
        assert_eq!(entries[1].deletions, 0);
    }
}
