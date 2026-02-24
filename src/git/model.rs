use std::fmt;
use std::path::PathBuf;

/// A Git context representing a repository/worktree
#[derive(Debug, Clone)]
pub struct GitContext {
    pub repo_root: PathBuf,
    pub worktree_path: PathBuf,
    pub git_dir: Option<PathBuf>,
    pub current_branch: Option<String>,
    pub head_ref: Option<String>,
    pub is_detached: bool,
    pub is_unborn: bool,
    pub source: ContextSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextSource {
    Cli,
    Cwd,
    PaneDiscovery(String),
    Recent,
    Worktree,
}

impl fmt::Display for GitContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let branch = self
            .current_branch
            .as_deref()
            .unwrap_or(self.head_ref.as_deref().unwrap_or("(unknown)"));
        write!(f, "{} [{}]", self.worktree_path.display(), branch)
    }
}

/// The specification of what diff to show
#[derive(Debug, Clone)]
pub enum DiffSpec {
    Worktree(WorktreeMode),
    Compare {
        base: String,
        target: Option<String>,
    },
}

impl Default for DiffSpec {
    fn default() -> Self {
        DiffSpec::Worktree(WorktreeMode::Unstaged)
    }
}

impl fmt::Display for DiffSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffSpec::Worktree(WorktreeMode::Unstaged) => write!(f, "Worktree (unstaged)"),
            DiffSpec::Worktree(WorktreeMode::Staged) => write!(f, "Worktree (staged)"),
            DiffSpec::Compare { base, target } => {
                let t = target.as_deref().unwrap_or("HEAD");
                write!(f, "Compare: {}...{}", base, t)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorktreeMode {
    Unstaged,
    Staged,
}

/// A changed file entry
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    /// True when `git --numstat` reports `-` for additions/deletions (binary file).
    pub is_binary: bool,
}

impl fmt::Display for FileEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self.status {
            FileStatus::Added => "A",
            FileStatus::Modified => "M",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Copied => "C",
            FileStatus::Untracked => "?",
            FileStatus::TypeChanged => "T",
            FileStatus::Unmerged => "U",
            FileStatus::Unknown => " ",
        };
        if let Some(ref old) = self.old_path {
            write!(f, "{} {} -> {}", prefix, old, self.path)
        } else {
            write!(f, "{} {}", prefix, self.path)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    TypeChanged,
    Unmerged,
    Unknown,
}

impl FileStatus {
    pub fn from_char(c: char) -> Self {
        match c {
            'A' => FileStatus::Added,
            'M' => FileStatus::Modified,
            'D' => FileStatus::Deleted,
            'R' => FileStatus::Renamed,
            'C' => FileStatus::Copied,
            '?' => FileStatus::Untracked,
            'T' => FileStatus::TypeChanged,
            'U' => FileStatus::Unmerged,
            _ => FileStatus::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FileStatus::Added => "Added",
            FileStatus::Modified => "Modified",
            FileStatus::Deleted => "Deleted",
            FileStatus::Renamed => "Renamed",
            FileStatus::Copied => "Copied",
            FileStatus::Untracked => "Untracked",
            FileStatus::TypeChanged => "Type Changed",
            FileStatus::Unmerged => "Unmerged",
            FileStatus::Unknown => "Unknown",
        }
    }
}

/// A parsed diff for a single file
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub file_header: String,
    /// Extended header lines between `diff --git` and `---`/`+++`
    /// (e.g. `index ...`, `similarity index`, `rename from/to`, `new file mode`, etc.)
    pub extended_headers: Vec<String>,
    pub old_file: String,
    pub new_file: String,
    pub hunks: Vec<Hunk>,
    pub is_binary: bool,
    pub is_new_file: bool,
    pub is_deleted: bool,
    pub is_rename: bool,
    pub is_submodule: bool,
    pub is_truncated: bool,
}

/// A single hunk in a diff
#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

/// A single line in a diff
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    HunkHeader,
    NoNewline,
}

/// A worktree entry
#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub head_ref: Option<String>,
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_main: bool,
}

impl fmt::Display for WorktreeEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let branch = if let Some(ref b) = self.branch {
            b.as_str()
        } else if self.is_bare {
            "(bare)"
        } else if self.is_detached {
            "(detached)"
        } else {
            "(unknown)"
        };
        let marker = if self.is_main { " [main worktree]" } else { "" };
        write!(f, "{} ({}){}", self.path.display(), branch, marker)
    }
}

/// A ref entry (branch/tag)
#[derive(Debug, Clone)]
pub struct RefEntry {
    pub name: String,
    pub kind: RefKind,
    pub is_remote: bool,
}

impl fmt::Display for RefEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RefKind {
    Branch,
    Tag,
    Other,
}
