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
    pub filtered: Vec<usize>,
    pub query: String,
    pub selected: usize,
}

impl SelectorState {
    pub fn new(items: Vec<String>) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        Self {
            items,
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
}

/// Search state
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<(usize, usize)>, // (hunk_index, line_index)
    pub current_match: usize,
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
            toast: None,
            last_error: None,
            generation: 0,
            config,
            persistent,
            term_width: 0,
            term_height: 0,
            loading: false,
        }
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
        Self {
            state: AppState::new(config),
            repo_path,
            git: Arc::new(GitCli::new()),
            internal_tx: tx,
            internal_rx: rx,
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        let size = terminal.size()?;
        self.state.term_width = size.width;
        self.state.term_height = size.height;

        // Resolve context
        self.resolve_context()?;

        // Load initial data
        self.load_files();
        self.load_refs();
        self.load_worktrees();

        // Start watcher
        let _watcher = self.start_watcher();

        info!("Entering main loop");

        loop {
            // Draw
            terminal.draw(|f| {
                ui::render::draw(f, &self.state);
            })?;

            // Process internal events (non-blocking)
            while let Ok(ev) = self.internal_rx.try_recv() {
                update::handle_internal_event(&mut self.state, ev);
            }

            // Clean up expired toast
            if let Some(ref toast) = self.state.toast {
                if toast.is_expired() {
                    self.state.toast = None;
                }
            }

            // Poll for input
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if let Some(input) = self.map_key(key) {
                        if matches!(input, InputEvent::Quit) && self.state.overlay == Overlay::None
                        {
                            // Save state before quitting
                            self.save_state();
                            break;
                        }
                        let cmd = update::handle_input(&mut self.state, input);
                        if let Some(cmd) = cmd {
                            commands::execute(cmd, &mut self.state, &self.git, &self.internal_tx);
                        }
                    }
                } else if let Event::Resize(w, h) = event::read()? {
                    self.state.term_width = w;
                    self.state.term_height = h;
                }
            }
        }

        Ok(())
    }

    fn resolve_context(&mut self) -> Result<()> {
        let git = &self.git;
        match git.repo_root(&self.repo_path) {
            Ok(root) => {
                let branch = git.current_branch(&root).unwrap_or(None);
                let head = git.head_ref(&root).unwrap_or(None);
                let unborn = git.is_unborn(&root).unwrap_or(false);
                let detached = git.is_detached(&root).unwrap_or(false);

                let ctx = GitContext {
                    repo_root: root.clone(),
                    worktree_path: root.clone(),
                    git_dir: None,
                    current_branch: branch.clone(),
                    head_ref: head,
                    is_detached: detached,
                    is_unborn: unborn,
                    source: ContextSource::Cwd,
                };

                // Determine default base for Compare mode
                if let Some(base) = git.find_default_base(&root, &self.state.config.default_base) {
                    if !unborn {
                        // Restore last mode from persistent state, or default
                        let mode = self.state.persistent.last_mode.as_deref();
                        let last_base = self.state.persistent.last_base.clone();
                        match mode {
                            Some("compare") => {
                                self.state.diff_spec = DiffSpec::Compare {
                                    base: last_base.unwrap_or(base),
                                    target: None,
                                };
                            }
                            _ => {
                                // Store the default base for later
                                self.state.diff_spec = DiffSpec::Worktree(
                                    if self.state.persistent.last_worktree_mode.as_deref()
                                        == Some("staged")
                                    {
                                        WorktreeMode::Staged
                                    } else {
                                        WorktreeMode::Unstaged
                                    },
                                );
                            }
                        }
                    }
                }

                // Update persistent state
                self.state.persistent.add_recent_context(
                    root,
                    branch,
                    self.state.config.max_recent_contexts,
                );

                self.state.context = Some(ctx);
                info!("Context resolved: {:?}", self.state.context);
            }
            Err(e) => {
                self.state.last_error = Some(format!("Not a git repository: {}", e));
            }
        }
        Ok(())
    }

    fn load_files(&self) {
        if self.state.context.is_none() {
            return;
        }
        let ctx = self.state.context.as_ref().unwrap().clone();
        let spec = self.state.diff_spec.clone();
        let git = self.git.clone();
        let tx = self.internal_tx.clone();
        let gen = self.state.generation;

        std::thread::spawn(move || match git.changed_files(&ctx.worktree_path, &spec) {
            Ok(files) => {
                let _ = tx.send(InternalEvent::GitFilesUpdated {
                    generation: gen,
                    files,
                });
            }
            Err(e) => {
                let _ = tx.send(InternalEvent::Error(format!("Failed to get files: {}", e)));
            }
        });
    }

    fn load_refs(&self) {
        if self.state.context.is_none() {
            return;
        }
        let ctx = self.state.context.as_ref().unwrap().clone();
        let git = self.git.clone();
        let tx = self.internal_tx.clone();

        std::thread::spawn(move || match git.list_refs(&ctx.worktree_path) {
            Ok(refs) => {
                let _ = tx.send(InternalEvent::GitRefsLoaded { refs });
            }
            Err(e) => {
                let _ = tx.send(InternalEvent::Error(format!("Failed to list refs: {}", e)));
            }
        });
    }

    fn load_worktrees(&self) {
        if self.state.context.is_none() {
            return;
        }
        let ctx = self.state.context.as_ref().unwrap().clone();
        let git = self.git.clone();
        let tx = self.internal_tx.clone();

        std::thread::spawn(move || match git.list_worktrees(&ctx.worktree_path) {
            Ok(worktrees) => {
                let _ = tx.send(InternalEvent::GitWorktreesLoaded { worktrees });
            }
            Err(e) => {
                let _ = tx.send(InternalEvent::Error(format!(
                    "Failed to list worktrees: {}",
                    e
                )));
            }
        });
    }

    pub fn load_diff_for_selected(&self) {
        if self.state.context.is_none() || self.state.files.is_empty() {
            return;
        }
        let ctx = self.state.context.as_ref().unwrap().clone();
        let spec = self.state.diff_spec.clone();
        let file_path = self.state.files[self.state.file_selected].path.clone();
        let git = self.git.clone();
        let tx = self.internal_tx.clone();
        let gen = self.state.generation;
        let ctx_lines = self.state.config.unified_context;

        std::thread::spawn(move || {
            match git.file_diff(&ctx.worktree_path, &spec, &file_path, ctx_lines) {
                Ok(diff) => {
                    let _ = tx.send(InternalEvent::GitDiffUpdated {
                        generation: gen,
                        file_path,
                        diff,
                    });
                }
                Err(e) => {
                    let _ = tx.send(InternalEvent::Error(format!("Failed to get diff: {}", e)));
                }
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
        // Check if we're in a selector or search overlay
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
                KeyCode::Char(c) => Some(InputEvent::SearchInput(c)),
                _ => None,
            };
        }

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
                KeyCode::Down | KeyCode::Char('j')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    Some(InputEvent::SelectorMoveDown)
                }
                KeyCode::Up | KeyCode::Char('k')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    Some(InputEvent::SelectorMoveUp)
                }
                KeyCode::Char(c) => Some(InputEvent::SelectorInput(c)),
                _ => None,
            };
        }

        if self.state.overlay == Overlay::Help {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => Some(InputEvent::Quit),
                _ => None,
            };
        }

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
            KeyCode::Char('p') => Some(InputEvent::PrevHunk),
            KeyCode::Char('f') => Some(InputEvent::SearchInDiff),
            KeyCode::Char('m') => Some(InputEvent::ToggleMode),
            KeyCode::Char('s') => Some(InputEvent::ToggleStagedUnstaged),
            KeyCode::Char('b') => Some(InputEvent::OpenBaseSelector),
            KeyCode::Char('t') => Some(InputEvent::OpenTargetSelector),
            KeyCode::Char('w') => Some(InputEvent::OpenWorktreeSelector),
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
