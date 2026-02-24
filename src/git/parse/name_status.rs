use crate::git::model::{FileEntry, FileStatus};

/// Parse `git diff --name-status -z` NUL-delimited output.
///
/// With `-z`, git outputs raw (unquoted) paths separated by NUL bytes:
///   STATUS\0PATH\0           — for regular changes (M, A, D, T, U, ?)
///   STATUS\0OLD_PATH\0NEW\0  — for renames/copies (R, C)
pub fn parse_name_status(output: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let fields: Vec<&str> = output.split('\0').collect();
    let mut i = 0;

    while i < fields.len() {
        let field = fields[i];
        if field.is_empty() {
            i += 1;
            continue;
        }

        let status_char = match field.chars().next() {
            Some(c) => c,
            None => {
                i += 1;
                continue;
            }
        };
        let status = FileStatus::from_char(status_char);

        match status {
            FileStatus::Renamed | FileStatus::Copied => {
                // Consume old_path and new_path
                if i + 2 < fields.len() {
                    let old_path = fields[i + 1];
                    let new_path = fields[i + 2];
                    entries.push(FileEntry {
                        path: new_path.to_string(),
                        old_path: Some(old_path.to_string()),
                        status,
                        additions: 0,
                        deletions: 0,
                        is_binary: false,
                    });
                    i += 3;
                } else {
                    break;
                }
            }
            _ => {
                // Consume path
                if i + 1 < fields.len() {
                    let path = fields[i + 1];
                    if !path.is_empty() {
                        entries.push(FileEntry {
                            path: path.to_string(),
                            old_path: None,
                            status,
                            additions: 0,
                            deletions: 0,
                            is_binary: false,
                        });
                    }
                    i += 2;
                } else {
                    break;
                }
            }
        }
    }
    entries
}

/// Expand a numstat rename path like "src/{old.rs => new.rs}"
/// to ("src/old.rs", "src/new.rs")
fn expand_rename_path(path: &str) -> Option<(String, String)> {
    if let Some(brace_start) = path.find('{') {
        if let Some(brace_end) = path.find('}').filter(|&end| end > brace_start) {
            let prefix = &path[..brace_start];
            let suffix = &path[brace_end + 1..];
            let inner = &path[brace_start + 1..brace_end];
            if let Some((old, new)) = inner.split_once(" => ") {
                return Some((
                    format!("{}{}{}", prefix, old, suffix),
                    format!("{}{}{}", prefix, new, suffix),
                ));
            }
        }
    }
    if let Some((old, new)) = path.split_once(" => ") {
        return Some((old.to_string(), new.to_string()));
    }
    None
}

/// Parse `git diff --numstat -z` NUL-delimited output and merge into existing entries.
///
/// With `-z`, numstat output uses NUL as the record terminator:
///   ADD\tDEL\tPATH\0              — for regular changes
///   ADD\tDEL\t\0OLD_PATH\0NEW\0   — for renames (path field empty, then old\0new\0)
pub fn merge_numstat(entries: &mut [FileEntry], numstat_output: &str) {
    let fields: Vec<&str> = numstat_output.split('\0').collect();
    let mut i = 0;

    while i < fields.len() {
        let field = fields[i];
        if field.is_empty() {
            i += 1;
            continue;
        }

        // Use splitn(3, ...) so that tab characters inside filenames are preserved
        let parts: Vec<&str> = field.splitn(3, '\t').collect();
        if parts.len() < 3 {
            i += 1;
            continue;
        }

        // Binary files show "-" for additions/deletions
        let is_binary = parts[0] == "-" || parts[1] == "-";
        let additions = parts[0].parse::<u32>().unwrap_or(0);
        let deletions = parts[1].parse::<u32>().unwrap_or(0);
        let path = parts[2];

        if path.is_empty() {
            // Rename: path is empty, next two NUL-fields are old_path and new_path
            if i + 2 < fields.len() {
                let old_path = fields[i + 1];
                let new_path = fields[i + 2];
                if let Some(entry) = entries.iter_mut().find(|e| {
                    e.path == new_path
                        || e.path == old_path
                        || e.old_path.as_deref() == Some(old_path)
                }) {
                    entry.additions = additions;
                    entry.deletions = deletions;
                    entry.is_binary = is_binary;
                }
                i += 3;
            } else {
                break;
            }
        } else {
            // Regular file or brace-expanded rename
            let expanded = expand_rename_path(path);
            if let Some(entry) = entries.iter_mut().find(|e| {
                if e.path == path {
                    return true;
                }
                if let Some((ref old, ref new)) = expanded {
                    if e.path == *new || e.path == *old {
                        return true;
                    }
                    if let Some(ref op) = e.old_path {
                        if *op == *old {
                            return true;
                        }
                    }
                }
                false
            }) {
                entry.additions = additions;
                entry.deletions = deletions;
                entry.is_binary = is_binary;
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_status_basic() {
        // NUL-delimited format: STATUS\0PATH\0...
        let output = "M\0src/main.rs\0A\0src/new.rs\0D\0old.rs\0";
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
        let output = "R100\0old_name.rs\0new_name.rs\0";
        let entries = parse_name_status(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, FileStatus::Renamed);
        assert_eq!(entries[0].path, "new_name.rs");
        assert_eq!(entries[0].old_path, Some("old_name.rs".to_string()));
    }

    #[test]
    fn test_parse_name_status_unicode() {
        // Non-ASCII paths should come through raw (not C-quoted) with -z
        let output = "M\0src/日本語.rs\0A\0données/café.txt\0";
        let entries = parse_name_status(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "src/日本語.rs");
        assert_eq!(entries[1].path, "données/café.txt");
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
                is_binary: false,
            },
            FileEntry {
                path: "src/new.rs".into(),
                old_path: None,
                status: FileStatus::Added,
                additions: 0,
                deletions: 0,
                is_binary: false,
            },
        ];
        // NUL-delimited numstat: ADD\tDEL\tPATH\0
        let numstat = "10\t5\tsrc/main.rs\x0020\t0\tsrc/new.rs\0";
        merge_numstat(&mut entries, numstat);
        assert_eq!(entries[0].additions, 10);
        assert_eq!(entries[0].deletions, 5);
        assert_eq!(entries[1].additions, 20);
        assert_eq!(entries[1].deletions, 0);
    }

    #[test]
    fn test_merge_numstat_rename_nul() {
        // With -z, renames have empty path then old\0new\0
        let mut entries = vec![FileEntry {
            path: "src/new.rs".into(),
            old_path: Some("src/old.rs".into()),
            status: FileStatus::Renamed,
            additions: 0,
            deletions: 0,
            is_binary: false,
        }];
        let numstat = "3\t1\t\0src/old.rs\0src/new.rs\0";
        merge_numstat(&mut entries, numstat);
        assert_eq!(entries[0].additions, 3);
        assert_eq!(entries[0].deletions, 1);
    }

    #[test]
    fn test_merge_numstat_rename_brace() {
        let mut entries = vec![FileEntry {
            path: "src/new.rs".into(),
            old_path: Some("src/old.rs".into()),
            status: FileStatus::Renamed,
            additions: 0,
            deletions: 0,
            is_binary: false,
        }];
        let numstat = "3\t1\tsrc/{old.rs => new.rs}\0";
        merge_numstat(&mut entries, numstat);
        assert_eq!(entries[0].additions, 3);
        assert_eq!(entries[0].deletions, 1);
    }

    #[test]
    fn test_merge_numstat_rename_plain() {
        let mut entries = vec![FileEntry {
            path: "new_name.rs".into(),
            old_path: Some("old_name.rs".into()),
            status: FileStatus::Renamed,
            additions: 0,
            deletions: 0,
            is_binary: false,
        }];
        let numstat = "5\t2\told_name.rs => new_name.rs\0";
        merge_numstat(&mut entries, numstat);
        assert_eq!(entries[0].additions, 5);
        assert_eq!(entries[0].deletions, 2);
    }

    #[test]
    fn test_expand_rename_path() {
        assert_eq!(
            expand_rename_path("src/{old.rs => new.rs}"),
            Some(("src/old.rs".into(), "src/new.rs".into()))
        );
        assert_eq!(
            expand_rename_path("old.rs => new.rs"),
            Some(("old.rs".into(), "new.rs".into()))
        );
        assert_eq!(expand_rename_path("src/file.rs"), None);
    }
}
