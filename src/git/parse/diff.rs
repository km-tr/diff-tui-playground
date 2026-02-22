use crate::git::model::{DiffLine, DiffLineKind, FileDiff, Hunk};
use regex::Regex;
use std::sync::LazyLock;

static HUNK_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$").unwrap());

/// Parse unified diff output for a single file
pub fn parse_file_diff(output: &str) -> FileDiff {
    let lines: Vec<&str> = output.lines().collect();
    let mut hunks = Vec::new();
    let mut file_header = String::new();
    let mut old_file = String::new();
    let mut new_file = String::new();
    let mut is_binary = false;
    let mut is_new_file = false;
    let mut is_deleted = false;
    let mut is_rename = false;
    let mut is_submodule = false;

    let mut i = 0;

    // Parse file header lines (diff --git, index, etc.)
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("diff --git") {
            file_header = line.to_string();
        } else if let Some(rest) = line.strip_prefix("--- ") {
            old_file = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            new_file = rest.to_string();
            i += 1;
            break;
        } else if line.starts_with("Binary files") {
            is_binary = true;
            i += 1;
            break;
        } else if line.starts_with("new file mode") {
            is_new_file = true;
        } else if line.starts_with("deleted file mode") {
            is_deleted = true;
        } else if line.starts_with("rename from") || line.starts_with("rename to") {
            is_rename = true;
        } else if line.starts_with("Submodule")
            || (line.starts_with("index ") && line.contains(" 160000"))
            || line.starts_with("new file mode 160000")
            || line.starts_with("deleted file mode 160000")
        {
            is_submodule = true;
        }
        i += 1;
    }

    // Parse hunks
    while i < lines.len() {
        let line = lines[i];
        if let Some(caps) = HUNK_HEADER_RE.captures(line) {
            let old_start = caps[1].parse::<u32>().unwrap_or(0);
            let old_count = caps
                .get(2)
                .map_or(1, |m| m.as_str().parse::<u32>().unwrap_or(1));
            let new_start = caps[3].parse::<u32>().unwrap_or(0);
            let new_count = caps
                .get(4)
                .map_or(1, |m| m.as_str().parse::<u32>().unwrap_or(1));
            let header = line.to_string();

            let mut hunk_lines = Vec::new();
            hunk_lines.push(DiffLine {
                kind: DiffLineKind::HunkHeader,
                content: header.clone(),
            });

            i += 1;
            while i < lines.len() {
                let hline = lines[i];
                if hline.starts_with("@@ ") || hline.starts_with("diff --git") {
                    break;
                }
                let kind = if hline.starts_with('+') {
                    DiffLineKind::Addition
                } else if hline.starts_with('-') {
                    DiffLineKind::Deletion
                } else if hline.starts_with('\\') {
                    DiffLineKind::NoNewline
                } else {
                    DiffLineKind::Context
                };
                hunk_lines.push(DiffLine {
                    kind,
                    content: hline.to_string(),
                });
                i += 1;
            }

            hunks.push(Hunk {
                header,
                old_start,
                old_count,
                new_start,
                new_count,
                lines: hunk_lines,
            });
        } else {
            i += 1;
        }
    }

    FileDiff {
        file_header,
        old_file,
        new_file,
        hunks,
        is_binary,
        is_new_file,
        is_deleted,
        is_rename,
        is_submodule,
    }
}

/// Parse full diff output that may contain multiple files, returning one FileDiff per file
pub fn parse_multi_file_diff(output: &str) -> Vec<FileDiff> {
    let mut diffs = Vec::new();
    let mut current_chunk = String::new();

    for line in output.lines() {
        if line.starts_with("diff --git") && !current_chunk.is_empty() {
            diffs.push(parse_file_diff(&current_chunk));
            current_chunk.clear();
        }
        current_chunk.push_str(line);
        current_chunk.push('\n');
    }
    if !current_chunk.is_empty() {
        diffs.push(parse_file_diff(&current_chunk));
    }

    diffs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"Hello\");
+    println!(\"Hello, world!\");
+    println!(\"Goodbye\");
 }
";
        let result = parse_file_diff(diff);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.old_file, "a/src/main.rs");
        assert_eq!(result.new_file, "b/src/main.rs");
        assert!(!result.is_binary);
        assert!(!result.is_new_file);

        let hunk = &result.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 3);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 4);
        // hunk header + 1 context + 1 deletion + 2 additions + 1 context = 6
        assert_eq!(hunk.lines.len(), 6);
        assert_eq!(hunk.lines[0].kind, DiffLineKind::HunkHeader);
        assert_eq!(hunk.lines[1].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Deletion);
        assert_eq!(hunk.lines[3].kind, DiffLineKind::Addition);
        assert_eq!(hunk.lines[4].kind, DiffLineKind::Addition);
        assert_eq!(hunk.lines[5].kind, DiffLineKind::Context);
    }

    #[test]
    fn test_parse_binary_diff() {
        let diff = "\
diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
";
        let result = parse_file_diff(diff);
        assert!(result.is_binary);
        assert!(result.hunks.is_empty());
    }

    #[test]
    fn test_parse_new_file() {
        let diff = "\
diff --git a/new.rs b/new.rs
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,3 @@
+fn hello() {
+    println!(\"hello\");
+}
";
        let result = parse_file_diff(diff);
        assert!(result.is_new_file);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].lines.len(), 4); // header + 3 additions
    }

    #[test]
    fn test_parse_multi_hunk() {
        let diff = "\
diff --git a/file.rs b/file.rs
index abc..def 100644
--- a/file.rs
+++ b/file.rs
@@ -1,3 +1,4 @@
 line1
+added1
 line2
 line3
@@ -10,3 +11,4 @@
 line10
+added2
 line11
 line12
";
        let result = parse_file_diff(diff);
        assert_eq!(result.hunks.len(), 2);
        assert_eq!(result.hunks[0].old_start, 1);
        assert_eq!(result.hunks[1].old_start, 10);
    }

    #[test]
    fn test_parse_rename() {
        let diff = "\
diff --git a/old.rs b/new.rs
similarity index 95%
rename from old.rs
rename to new.rs
index abc..def 100644
--- a/old.rs
+++ b/new.rs
@@ -1,3 +1,3 @@
 fn main() {
-    old();
+    new();
 }
";
        let result = parse_file_diff(diff);
        assert!(result.is_rename);
        assert_eq!(result.hunks.len(), 1);
    }

    #[test]
    fn test_parse_multi_file_diff() {
        let diff = "\
diff --git a/file1.rs b/file1.rs
index abc..def 100644
--- a/file1.rs
+++ b/file1.rs
@@ -1,1 +1,1 @@
-old
+new
diff --git a/file2.rs b/file2.rs
index abc..def 100644
--- a/file2.rs
+++ b/file2.rs
@@ -1,1 +1,1 @@
-foo
+bar
";
        let diffs = parse_multi_file_diff(diff);
        assert_eq!(diffs.len(), 2);
    }

    #[test]
    fn test_parse_empty() {
        let diff = "";
        let result = parse_file_diff(diff);
        assert!(result.hunks.is_empty());
        assert!(!result.is_binary);
    }

    #[test]
    fn test_parse_no_newline_at_eof() {
        let diff = "\
diff --git a/file.rs b/file.rs
index abc..def 100644
--- a/file.rs
+++ b/file.rs
@@ -1,2 +1,2 @@
 line1
-line2
\\ No newline at end of file
+line2_new
\\ No newline at end of file
";
        let result = parse_file_diff(diff);
        assert_eq!(result.hunks.len(), 1);
        let hunk = &result.hunks[0];
        let no_nl_lines: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::NoNewline)
            .collect();
        assert_eq!(no_nl_lines.len(), 2);
    }
}
