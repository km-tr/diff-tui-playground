use crate::discovery::PaneCandidate;
use crate::git::model::{FileDiff, FileEntry, GitContext, RefEntry, WorktreeEntry};

/// Events from user input
#[derive(Debug)]
pub enum InputEvent {
    MoveDown,
    MoveUp,
    GoTop,
    GoBottom,
    PageUp,
    PageDown,
    SwitchFocus,
    NextHunk,
    PrevHunk,
    SearchInDiff,
    ToggleMode,
    ToggleStagedUnstaged,
    OpenBaseSelector,
    OpenTargetSelector,
    OpenWorktreeSelector,
    OpenContextSelector,
    CopyHunk,
    CopyFileDiff,
    Export,
    Reload,
    Help,
    Quit,
    // Selector interactions
    SelectorInput(char),
    SelectorBackspace,
    SelectorConfirm,
    SelectorCancel,
    SelectorMoveDown,
    SelectorMoveUp,
    // Search interactions
    SearchInput(char),
    SearchBackspace,
    SearchConfirm,
    SearchCancel,
    SearchNext,
    SearchPrev,
    Resize(u16, u16),
}

/// Events from internal/async operations
#[derive(Debug)]
pub enum InternalEvent {
    GitFilesUpdated {
        generation: u64,
        files: Vec<FileEntry>,
    },
    GitDiffUpdated {
        generation: u64,
        file_path: String,
        diff: FileDiff,
    },
    GitRefsLoaded {
        refs: Vec<RefEntry>,
    },
    GitWorktreesLoaded {
        worktrees: Vec<WorktreeEntry>,
    },
    ContextResolved {
        context: GitContext,
    },
    PanesDiscovered {
        panes: Vec<PaneCandidate>,
    },
    WatchTriggered,
    Error(String),
    Toast(String),
}
