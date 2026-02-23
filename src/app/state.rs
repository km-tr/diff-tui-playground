use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing::{error, info};

use crate::config::{AppConfig, PersistentState};
use crate::discovery;
use crate::git::backend::GitBackend;
use crate::git::cli::GitCli;
use crate::git::model::*;
use crate::ui;
use crate::watch::FileWatcher;

use super::commands;
use super::event::{InputEvent, InternalEvent};
use super::update;

/// Which pane has focus
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusPane {
    FileList,
    DiffView,
}

/// Overlay/modal state
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    None,
    Help,
    BaseSelector,
    TargetSelector,
    WorktreeSelector,
    ContextSelector,
    Search,
    Export,
    FileFilter,
}

/// Toast message with expiry
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub expires: Instant,
}

impl Toast {
    pub fn new(msg: impl Into<String>, duration: Duration) -> Self {
        Self {
            message: msg.into(),
            expires: Instant::now() + duration,
        }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires
    }
}

/// Selector state (shared by all selectors)
#[derive(Debug, Clone)]
pub struct SelectorState {
    pub items: Vec<String>,
    /// Optional structured data parallel to `items`. When present,
    /// `selected_data()` returns the data value instead of the display label.
    /// Used by context/worktree selectors to avoid parsing paths from labels.
    pub data: Vec<String>,
    pub filtered: Vec<usize>,
    pub query: String,
    pub selected: usize,
}

impl SelectorState {
    pub fn new(items: Vec<String>) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        Self {
            items,
            data: Vec::new(),
            filtered,
            query: String::new(),
            selected: 0,
        }
    }

    pub fn with_data(items: Vec<String>, data: Vec<String>) -> Self {
        debug_assert_eq!(
            items.len(),
            data.len(),
            "SelectorState: items and data must have the same length"
        );
        let filtered: Vec<usize> = (0..items.len()).collect();
        Self {
            items,
            data,
            filtered,
            query: String::new(),
            selected: 0,
        }
    }

    pub fn filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            use fuzzy_matcher::skim::SkimMatcherV2;
            use fuzzy_matcher::FuzzyMatcher;
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(usize, i64)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    matcher
                        .fuzzy_match(item, &self.query)
                        .map(|score| (i, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        self.selected = 0;
    }

    pub fn selected_item(&self) -> Option<&str> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.items.get(i))
            .map(|s| s.as_str())
    }

    /// Returns the data value for the selected item if `data` was provided,
    /// otherwise falls back to the display label.
    pub fn selected_data(&self) -> Option<&str> {
        let idx = self.filtered.get(self.selected).copied()?;
        if !self.data.is_empty() {
            self.data.get(idx).map(|s| s.as_str())
        } else {
            self.items.get(idx).map(|s| s.as_str())
        }
    }
}

/// Search state
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<(usize, usize)>, // (hunk_index, line_index)
    pub current_match: usize,
}

/// Export dialog state
#[derive(Debug, Clone, Default)]
pub struct ExportState {
    pub path_input: String,
}

/// The full application state
pub struct AppState {
    pub context: Option<GitContext>,
    pub diff_spec: DiffSpec,
    pub focus: FocusPane,
    pub overlay: Overlay,

    // File list
    pub files: Vec<FileEntry>,
    pub file_selected: usize,
    pub file_scroll: usize,
    // Cache for filtered_file_indices (invalidated when files or file_filter changes)
    filtered_cache: Option<(u64, String, Vec<usize>)>, // (files_version, filter, indices)
    pub file_filter: String,

    // Diff view
    pub current_diff: Option<FileDiff>,
    pub diff_scroll: usize,
    pub current_hunk: usize,
    pub diff_total_lines: usize,

    // Selectors
    pub selector: Option<SelectorState>,
    pub refs_cache: Vec<RefEntry>,
    pub worktrees_cache: Vec<WorktreeEntry>,

    // Discovery
    pub pane_candidates: Vec<discovery::PaneCandidate>,

    // Search
    pub search: SearchState,

    // Export
    pub export: ExportState,

    // Toast/Error
    pub toast: Option<Toast>,
    pub last_error: Option<String>,

    // Generation tracking (prevent stale updates)
    pub generation: u64,

    // Config
    pub config: AppConfig,
    pub persistent: PersistentState,

    // Terminal size
    pub term_width: u16,
    pub term_height: u16,

    // Loading indicator
    pub loading: bool,

    // Pending commands from internal event handling (Vec to avoid dropping
    // commands when multiple events arrive in a single drain loop)
    pub pending_commands: Vec<commands::Command>,

    // Discovered default base branch
    pub default_base: Option<String>,

    // Fallback poll timer
    pub last_poll: Instant,
}

impl AppState {
    fn new(config: AppConfig) -> Self {
        let persistent = PersistentState::load();
        Self {
            context: None,
            diff_spec: DiffSpec::default(),
            focus: FocusPane::FileList,
            overlay: Overlay::None,
            files: Vec::new(),
            file_selected: 0,
            file_scroll: 0,
            filtered_cache: None,
            file_filter: String::new(),
            current_diff: None,
            diff_scroll: 0,
            current_hunk: 0,
            diff_total_lines: 0,
            selector: None,
            refs_cache: Vec::new(),
            worktrees_cache: Vec::new(),
            pane_candidates: Vec::new(),
            search: SearchState::default(),
            export: ExportState::default(),
            toast: None,
            last_error: None,
            generation: 0,
            config,
            persistent,
            term_width: 0,
            term_height: 0,
            loading: false,
            pending_commands: Vec::new(),
            default_base: None,
            last_poll: Instant::now(),
        }
    }

    /// Get the filtered file list indices (cached; recomputed only when
    /// `file_filter` or `generation` changes).
    pub fn filtered_file_indices(&mut self) -> Vec<usize> {
        if let Some((gen, ref filter, ref indices)) = self.filtered_cache {
            if gen == self.generation && *filter == self.file_filter {
                return indices.clone();
            }
        }
        let result: Vec<usize> = if self.file_filter.is_empty() {
            (0..self.files.len()).collect()
        } else {
            use fuzzy_matcher::skim::SkimMatcherV2;
            use fuzzy_matcher::FuzzyMatcher;
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(usize, i64)> = self
                .files
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    matcher
                        .fuzzy_match(&f.path, &self.file_filter)
                        .map(|score| (i, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            scored.into_iter().map(|(i, _)| i).collect()
        };
        self.filtered_cache = Some((self.generation, self.file_filter.clone(), result.clone()));
        result
    }

    /// Get the actual selected file index in the unfiltered list
    pub fn actual_selected_file_index(&mut self) -> Option<usize> {
        let filtered = self.filtered_file_indices();
        filtered.get(self.file_selected).copied()
    }
}

/// The main application
pub struct App {
    state: AppState,
    repo_path: PathBuf,
    git: Arc<GitCli>,
    internal_tx: Sender<InternalEvent>,
    internal_rx: Receiver<InternalEvent>,
}

impl App {
    pub fn new(config: AppConfig, repo_path: PathBuf) -> Self {
        let (tx, rx) = bounded(256);
        let max_bytes = config.truncate_max_bytes;
        Self {
            state: AppState::new(config),
            repo_path,
            git: Arc::new(GitCli::with_max_bytes(max_bytes)),
            internal_tx: tx,
            internal_rx: rx,
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        let size = terminal.size()?;
        self.state.term_width = size.width;
        self.state.term_height = size.height;

        // Resolve context asynchronously
        self.resolve_context_async();

        // Start watcher (will be None until context resolves; that's fine)
        let mut _watcher: Option<FileWatcher> = None;

        info!("Entering main loop");

        loop {
            // Draw
            terminal.draw(|f| {
                ui::render::draw(f, &mut self.state);
            })?;

            // Process internal events (non-blocking)
            while let Ok(ev) = self.internal_rx.try_recv() {
                update::handle_internal_event(&mut self.state, ev);
            }

            // Execute all pending commands from internal event handling
            if !self.state.pending_commands.is_empty() {
                let cmds: Vec<_> = std::mem::take(&mut self.state.pending_commands);
                for cmd in cmds {
                    commands::execute(cmd, &mut self.state, &self.git, &self.internal_tx);
                }
                // Start watcher on first context load (not on every command batch).
                // Context-switch restarts are handled by the is_context_switch guard below.
                if _watcher.is_none() && self.state.context.is_some() {
                    _watcher = self.start_watcher();
                }
            }

            // Clean up expired toast
            if let Some(ref toast) = self.state.toast {
                if toast.is_expired() {
                    self.state.toast = None;
                }
            }

            // Fallback polling (if configured)
            if let Some(poll_secs) = self.state.config.watch.poll_interval_secs {
                if self.state.context.is_some()
                    && self.state.last_poll.elapsed() >= Duration::from_secs(poll_secs)
                {
                    self.state.last_poll = Instant::now();
                    self.state.generation += 1;
                    commands::execute(
                        commands::Command::Reload,
                        &mut self.state,
                        &self.git,
                        &self.internal_tx,
                    );
                }
            }

            // Poll for input (50ms tick)
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        if let Some(input) = self.map_key(key) {
                            if matches!(input, InputEvent::Quit)
                                && self.state.overlay == Overlay::None
                            {
                                self.save_state();
                                break;
                            }
                            let cmd = update::handle_input(&mut self.state, input);
                            if let Some(cmd) = cmd {
                                let is_context_switch = matches!(
                                    cmd,
                                    commands::Command::SwitchContext(_)
                                        | commands::Command::SwitchWorktree(_)
                                );
                                commands::execute(
                                    cmd,
                                    &mut self.state,
                                    &self.git,
                                    &self.internal_tx,
                                );
                                // Restart watcher when context may have changed
                                if is_context_switch && self.state.context.is_some() {
                                    _watcher = self.start_watcher();
                                }
                            }
                        }
                    }
                    Event::Resize(w, h) => {
                        self.state.term_width = w;
                        self.state.term_height = h;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn resolve_context_async(&self) {
        let git = self.git.clone();
        let tx = self.internal_tx.clone();
        let repo_path = self.repo_path.clone();
        let default_bases = self.state.config.default_base.clone();

        std::thread::spawn(move || match git.repo_root(&repo_path) {
            Ok(worktree_root) => {
                // Derive the true repository root from the git common directory.
                // For linked worktrees, --show-toplevel returns the worktree path
                // while --git-common-dir points to the main repo's .git directory.
                let repo_root = git
                    .git_common_dir(&worktree_root)
                    .ok()
                    .and_then(|common| common.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| worktree_root.clone());

                let branch = git.current_branch(&worktree_root).unwrap_or(None);
                let head = git.head_ref(&worktree_root).unwrap_or(None);
                let unborn = match git.is_unborn(&worktree_root) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = tx.send(InternalEvent::Error(format!(
                            "Failed to check unborn state: {}",
                            e
                        )));
                        return;
                    }
                };
                let detached = match git.is_detached(&worktree_root) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = tx.send(InternalEvent::Error(format!(
                            "Failed to check detached state: {}",
                            e
                        )));
                        return;
                    }
                };
                let default_base = git.find_default_base(&worktree_root, &default_bases);

                let ctx = GitContext {
                    repo_root,
                    worktree_path: worktree_root,
                    git_dir: None,
                    current_branch: branch,
                    head_ref: head,
                    is_detached: detached,
                    is_unborn: unborn,
                    source: ContextSource::Cwd,
                };

                let _ = tx.send(InternalEvent::ContextResolved {
                    context: ctx,
                    default_base,
                });
            }
            Err(e) => {
                let _ = tx.send(InternalEvent::Error(format!("Not a git repository: {}", e)));
            }
        });
    }

    fn start_watcher(&self) -> Option<FileWatcher> {
        if !self.state.config.watch.enabled {
            return None;
        }
        let ctx = self.state.context.as_ref()?;
        let tx = self.internal_tx.clone();
        let debounce = Duration::from_millis(self.state.config.watch.debounce_ms);

        match FileWatcher::new(&ctx.worktree_path, debounce, tx) {
            Ok(w) => {
                info!("File watcher started");
                Some(w)
            }
            Err(e) => {
                error!("Failed to start file watcher: {}", e);
                None
            }
        }
    }

    fn save_state(&mut self) {
        let mode = match &self.state.diff_spec {
            DiffSpec::Worktree(_) => "worktree",
            DiffSpec::Compare { .. } => "compare",
        };
        let wt_mode = match &self.state.diff_spec {
            DiffSpec::Worktree(m) => *m,
            _ => WorktreeMode::Unstaged,
        };
        let base = match &self.state.diff_spec {
            DiffSpec::Compare { base, .. } => Some(base.as_str()),
            _ => None,
        };
        self.state.persistent.save_mode(mode, wt_mode, base);
        if let Err(e) = self.state.persistent.save() {
            error!("Failed to save persistent state: {}", e);
        }
    }

    fn map_key(&self, key: KeyEvent) -> Option<InputEvent> {
        // Search overlay
        if self.state.overlay == Overlay::Search {
            return match key.code {
                KeyCode::Esc => Some(InputEvent::SearchCancel),
                KeyCode::Enter => Some(InputEvent::SearchConfirm),
                KeyCode::Backspace => Some(InputEvent::SearchBackspace),
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::SearchNext)
                }
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::SearchPrev)
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::Quit)
                }
                KeyCode::Char(c) => Some(InputEvent::SearchInput(c)),
                _ => None,
            };
        }

        // File filter overlay
        if self.state.overlay == Overlay::FileFilter {
            return match key.code {
                KeyCode::Esc => Some(InputEvent::FileFilterCancel),
                KeyCode::Enter => Some(InputEvent::FileFilterConfirm),
                KeyCode::Backspace => Some(InputEvent::FileFilterBackspace),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::Quit)
                }
                KeyCode::Char(c) => Some(InputEvent::FileFilterInput(c)),
                _ => None,
            };
        }

        // Export overlay
        if self.state.overlay == Overlay::Export {
            return match key.code {
                KeyCode::Esc => Some(InputEvent::ExportCancel),
                KeyCode::Enter => Some(InputEvent::ExportConfirm),
                KeyCode::Backspace => Some(InputEvent::ExportBackspace),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::Quit)
                }
                KeyCode::Char(c) => Some(InputEvent::ExportInput(c)),
                _ => None,
            };
        }

        // Selector overlays
        if matches!(
            self.state.overlay,
            Overlay::BaseSelector
                | Overlay::TargetSelector
                | Overlay::WorktreeSelector
                | Overlay::ContextSelector
        ) {
            return match key.code {
                KeyCode::Esc => Some(InputEvent::SelectorCancel),
                KeyCode::Enter => Some(InputEvent::SelectorConfirm),
                KeyCode::Backspace => Some(InputEvent::SelectorBackspace),
                KeyCode::Down => Some(InputEvent::SelectorMoveDown),
                KeyCode::Up => Some(InputEvent::SelectorMoveUp),
                KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::SelectorMoveDown)
                }
                KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::SelectorMoveUp)
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::Quit)
                }
                KeyCode::Char(c) => Some(InputEvent::SelectorInput(c)),
                _ => None,
            };
        }

        // Help overlay
        if self.state.overlay == Overlay::Help {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => Some(InputEvent::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(InputEvent::Quit)
                }
                _ => None,
            };
        }

        // Normal mode
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Some(InputEvent::Quit),
            KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::MoveUp),
            KeyCode::Char('g') | KeyCode::Home => Some(InputEvent::GoTop),
            KeyCode::Char('G') | KeyCode::End => Some(InputEvent::GoBottom),
            KeyCode::PageUp => Some(InputEvent::PageUp),
            KeyCode::PageDown => Some(InputEvent::PageDown),
            KeyCode::Tab => Some(InputEvent::SwitchFocus),
            KeyCode::Char('n') => Some(InputEvent::NextHunk),
            KeyCode::Char('N') => Some(InputEvent::SearchNext),
            KeyCode::Char('p') => Some(InputEvent::PrevHunk),
            KeyCode::Char('P') => Some(InputEvent::SearchPrev),
            KeyCode::Char('f') => Some(InputEvent::SearchInDiff),
            KeyCode::Char('/') => Some(InputEvent::OpenFileFilter),
            KeyCode::Char('m') => Some(InputEvent::ToggleMode),
            KeyCode::Char('s') => Some(InputEvent::ToggleStagedUnstaged),
            KeyCode::Char('b') => Some(InputEvent::OpenBaseSelector),
            KeyCode::Char('t') => Some(InputEvent::OpenTargetSelector),
            KeyCode::Char('w') => Some(InputEvent::OpenWorktreeSelector),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InputEvent::Quit)
            }
            KeyCode::Char('c') => Some(InputEvent::OpenContextSelector),
            KeyCode::Char('y') => Some(InputEvent::CopyHunk),
            KeyCode::Char('Y') => Some(InputEvent::CopyFileDiff),
            KeyCode::Char('o') => Some(InputEvent::Export),
            KeyCode::Char('r') => Some(InputEvent::Reload),
            KeyCode::Char('?') => Some(InputEvent::Help),
            _ => None,
        }
    }
}
