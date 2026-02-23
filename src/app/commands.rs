use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;

use super::event::InternalEvent;
use super::state::*;
use crate::config::DiscoveryMode;
use crate::discovery;
use crate::git::backend::GitBackend;
use crate::git::cli::GitCli;
use crate::git::model::*;
use crate::util::clipboard;

/// Commands that require side effects (git calls, clipboard, etc.)
#[derive(Debug)]
pub enum Command {
    Reload,
    InitialLoad,
    LoadDiff,
    CopyHunk,
    CopyFileDiff,
    ExportToFile(String),
    SwitchWorktree(String),
    SwitchContext(String),
    OpenContextSelector,
}

pub fn execute(cmd: Command, state: &mut AppState, git: &Arc<GitCli>, tx: &Sender<InternalEvent>) {
    match cmd {
        Command::Reload => {
            state.loading = true;
            reload_files(state, git, tx);
        }
        Command::InitialLoad => {
            state.loading = true;
            reload_files(state, git, tx);
            load_refs_and_worktrees(state, git, tx);
        }
        Command::LoadDiff => {
            load_diff(state, git, tx);
        }
        Command::CopyHunk => {
            copy_hunk(state);
        }
        Command::CopyFileDiff => {
            copy_file_diff(state);
        }
        Command::ExportToFile(path) => {
            export_to_file(state, &path);
        }
        Command::SwitchWorktree(selection) => {
            switch_to_path(state, git, tx, &selection, ContextSource::Worktree);
        }
        Command::SwitchContext(selection) => {
            switch_to_path(state, git, tx, &selection, ContextSource::Recent);
        }
        Command::OpenContextSelector => {
            open_context_selector(state, git, tx);
        }
    }
}

fn reload_files(state: &mut AppState, git: &Arc<GitCli>, tx: &Sender<InternalEvent>) {
    if state.context.is_none() {
        return;
    }
    let ctx = state.context.as_ref().unwrap().clone();
    let spec = state.diff_spec.clone();
    let git = git.clone();
    let tx = tx.clone();
    let gen = state.generation;

    std::thread::spawn(move || match git.changed_files(&ctx.worktree_path, &spec) {
        Ok(files) => {
            let _ = tx.send(InternalEvent::GitFilesUpdated {
                generation: gen,
                files,
            });
        }
        Err(e) => {
            let _ = tx.send(InternalEvent::Error(format!("Reload failed: {}", e)));
        }
    });
}

fn load_refs_and_worktrees(state: &AppState, git: &Arc<GitCli>, tx: &Sender<InternalEvent>) {
    if let Some(ref ctx) = state.context {
        let git = git.clone();
        let tx = tx.clone();
        let path = ctx.worktree_path.clone();
        std::thread::spawn(move || {
            match git.list_refs(&path) {
                Ok(refs) => {
                    let _ = tx.send(InternalEvent::GitRefsLoaded { refs });
                }
                Err(e) => {
                    tracing::warn!("Failed to list refs: {}", e);
                }
            }
            match git.list_worktrees(&path) {
                Ok(wts) => {
                    let _ = tx.send(InternalEvent::GitWorktreesLoaded { worktrees: wts });
                }
                Err(e) => {
                    tracing::warn!("Failed to list worktrees: {}", e);
                }
            }
        });
    }
}

fn load_diff(state: &mut AppState, git: &Arc<GitCli>, tx: &Sender<InternalEvent>) {
    if state.context.is_none() || state.files.is_empty() {
        return;
    }
    let idx = match state.actual_selected_file_index() {
        Some(i) => i,
        None => return,
    };
    if idx >= state.files.len() {
        return;
    }
    let ctx = state.context.as_ref().unwrap().clone();
    let spec = state.diff_spec.clone();
    let file_path = state.files[idx].path.clone();
    let git = git.clone();
    let tx = tx.clone();
    let gen = state.generation;
    let ctx_lines = state.config.unified_context;

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
                let _ = tx.send(InternalEvent::Error(format!("Failed to load diff: {}", e)));
            }
        }
    });
}

fn copy_hunk(state: &mut AppState) {
    if let Some(ref diff) = state.current_diff {
        if let Some(hunk) = diff.hunks.get(state.current_hunk) {
            let text: String = hunk
                .lines
                .iter()
                .map(|l| l.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            match clipboard::copy_to_clipboard(&text) {
                Ok(()) => {
                    state.toast = Some(Toast::new("Hunk copied", Duration::from_secs(2)));
                }
                Err(e) => {
                    state.toast = Some(Toast::new(
                        format!("Copy failed: {}", e),
                        Duration::from_secs(3),
                    ));
                }
            }
        } else {
            state.toast = Some(Toast::new("No hunk to copy", Duration::from_secs(2)));
        }
    } else {
        state.toast = Some(Toast::new("No diff to copy", Duration::from_secs(2)));
    }
}

fn build_full_diff_text(diff: &crate::git::model::FileDiff) -> String {
    let mut text = String::new();
    if !diff.file_header.is_empty() {
        text.push_str(&diff.file_header);
        text.push('\n');
    }
    // Extended headers: index, similarity index, rename from/to, mode changes, Binary files, etc.
    for header in &diff.extended_headers {
        text.push_str(header);
        text.push('\n');
    }
    if !diff.old_file.is_empty() {
        text.push_str("--- ");
        text.push_str(&diff.old_file);
        text.push('\n');
    }
    if !diff.new_file.is_empty() {
        text.push_str("+++ ");
        text.push_str(&diff.new_file);
        text.push('\n');
    }
    for hunk in &diff.hunks {
        if !hunk.header.is_empty() {
            text.push_str(&hunk.header);
            text.push('\n');
        }
        for line in &hunk.lines {
            text.push_str(&line.content);
            text.push('\n');
        }
    }
    text
}

fn copy_file_diff(state: &mut AppState) {
    if let Some(ref diff) = state.current_diff {
        let text = build_full_diff_text(diff);
        match clipboard::copy_to_clipboard(&text) {
            Ok(()) => {
                let msg = if diff.is_truncated {
                    "File diff copied (truncated — output exceeded size limit)"
                } else {
                    "File diff copied"
                };
                state.toast = Some(Toast::new(msg, Duration::from_secs(2)));
            }
            Err(e) => {
                state.toast = Some(Toast::new(
                    format!("Copy failed: {}", e),
                    Duration::from_secs(3),
                ));
            }
        }
    } else {
        state.toast = Some(Toast::new("No diff to copy", Duration::from_secs(2)));
    }
}

fn export_to_file(state: &mut AppState, path: &str) {
    if let Some(ref diff) = state.current_diff {
        let text = build_full_diff_text(diff);
        match std::fs::write(path, &text) {
            Ok(()) => {
                let msg = if diff.is_truncated {
                    format!("Exported to {} (truncated — output exceeded size limit)", path)
                } else {
                    format!("Exported to {}", path)
                };
                state.toast = Some(Toast::new(msg, Duration::from_secs(3)));
            }
            Err(e) => {
                state.toast = Some(Toast::new(
                    format!("Export failed: {}", e),
                    Duration::from_secs(3),
                ));
            }
        }
    }
}

fn switch_to_path(
    state: &mut AppState,
    git: &Arc<GitCli>,
    tx: &Sender<InternalEvent>,
    path_str: &str,
    source: ContextSource,
) {
    // path_str is the raw path from selector data (not a formatted label)
    let path = PathBuf::from(path_str);

    if !path.exists() {
        state.toast = Some(Toast::new(
            format!("Path does not exist: {}", path_str),
            Duration::from_secs(3),
        ));
        return;
    }

    resolve_and_switch(state, git, tx, &path, source);
}

fn resolve_and_switch(
    state: &mut AppState,
    git: &Arc<GitCli>,
    tx: &Sender<InternalEvent>,
    path: &Path,
    source: ContextSource,
) {
    match git.repo_root(path) {
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
                    state.toast = Some(Toast::new(
                        format!("Failed to check repo state: {}", e),
                        Duration::from_secs(3),
                    ));
                    return;
                }
            };
            let detached = match git.is_detached(&worktree_root) {
                Ok(v) => v,
                Err(e) => {
                    state.toast = Some(Toast::new(
                        format!("Failed to check repo state: {}", e),
                        Duration::from_secs(3),
                    ));
                    return;
                }
            };

            let ctx = GitContext {
                repo_root,
                worktree_path: worktree_root.clone(),
                git_dir: None,
                current_branch: branch.clone(),
                head_ref: head,
                is_detached: detached,
                is_unborn: unborn,
                source,
            };

            // Recompute default base for the new repo
            state.default_base =
                git.find_default_base(&worktree_root, &state.config.default_base);

            state.context = Some(ctx);
            state.generation += 1;

            // Update persistent — store the worktree path so we can switch back to it
            state
                .persistent
                .add_recent_context(worktree_root, branch, state.config.max_recent_contexts);
            if let Err(e) = state.persistent.save() {
                tracing::warn!("Failed to save persistent state: {}", e);
            }

            state.toast = Some(Toast::new(
                format!("Switched to {}", path.display()),
                Duration::from_secs(2),
            ));

            // Reload
            reload_files(state, git, tx);

            // Reload refs and worktrees
            load_refs_and_worktrees(state, git, tx);
        }
        Err(e) => {
            state.toast = Some(Toast::new(
                format!("Not a git repo: {}", e),
                Duration::from_secs(3),
            ));
        }
    }
}

fn open_context_selector(state: &mut AppState, _git: &Arc<GitCli>, tx: &Sender<InternalEvent>) {
    // Build context candidates from:
    // 1. Current context
    // 2. Worktrees
    // 3. Recent contexts
    // 4. Pane discovery candidates
    let mut items: Vec<String> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    let mut seen_paths: Vec<PathBuf> = Vec::new();

    // Current
    if let Some(ref ctx) = state.context {
        items.push(format!("{} [current]", ctx));
        paths.push(ctx.worktree_path.display().to_string());
        seen_paths.push(ctx.worktree_path.clone());
    }

    // Worktrees
    for wt in &state.worktrees_cache {
        if !seen_paths.contains(&wt.path) {
            items.push(format!("{} [worktree]", wt));
            paths.push(wt.path.display().to_string());
            seen_paths.push(wt.path.clone());
        }
    }

    // Recent contexts
    for rc in &state.persistent.recent_contexts {
        if !seen_paths.contains(&rc.path) {
            let label = if let Some(ref b) = rc.branch {
                format!("{} ({}) [recent]", rc.path.display(), b)
            } else {
                format!("{} [recent]", rc.path.display())
            };
            items.push(label);
            paths.push(rc.path.display().to_string());
            seen_paths.push(rc.path.clone());
        }
    }

    // Pane candidates
    for pc in &state.pane_candidates {
        if !seen_paths.contains(&pc.repo_root) {
            items.push(format!("{} [{}]", pc.repo_root.display(), pc.label));
            paths.push(pc.repo_root.display().to_string());
            seen_paths.push(pc.repo_root.clone());
        }
    }

    state.selector = Some(SelectorState::with_data(items, paths));
    state.overlay = Overlay::ContextSelector;

    // Trigger pane discovery refresh
    let config_mode = state.config.discovery.clone();
    if config_mode != DiscoveryMode::Off {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let panes = discovery::discover_panes(&config_mode);
            let _ = tx.send(InternalEvent::PanesDiscovered { panes });
        });
    }
}
