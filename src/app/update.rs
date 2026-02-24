use std::time::Duration;

use tracing::debug;

use super::commands::{self, Command};
use super::event::{InputEvent, InternalEvent};
use super::state::*;
use crate::git::model::*;

/// Handle input events, returning an optional command to execute
pub fn handle_input(state: &mut AppState, event: InputEvent) -> Option<Command> {
    match event {
        InputEvent::Quit => {
            if state.overlay != Overlay::None {
                state.overlay = Overlay::None;
                state.selector = None;
                None
            } else {
                // Quit is handled in the main loop
                None
            }
        }

        // Navigation
        InputEvent::MoveDown => {
            match state.focus {
                FocusPane::FileList => {
                    let filtered_len = state.filtered_file_indices().len();
                    if filtered_len > 0 && state.file_selected < filtered_len - 1 {
                        state.file_selected += 1;
                        return Some(Command::LoadDiff);
                    }
                }
                FocusPane::DiffView => {
                    if state.diff_scroll < state.diff_total_lines.saturating_sub(1) {
                        state.diff_scroll += 1;
                    }
                }
            }
            None
        }
        InputEvent::MoveUp => {
            match state.focus {
                FocusPane::FileList => {
                    if state.file_selected > 0 {
                        state.file_selected -= 1;
                        return Some(Command::LoadDiff);
                    }
                }
                FocusPane::DiffView => {
                    state.diff_scroll = state.diff_scroll.saturating_sub(1);
                }
            }
            None
        }
        InputEvent::GoTop => {
            match state.focus {
                FocusPane::FileList => {
                    if state.file_selected != 0 {
                        state.file_selected = 0;
                        return Some(Command::LoadDiff);
                    }
                }
                FocusPane::DiffView => {
                    state.diff_scroll = 0;
                }
            }
            None
        }
        InputEvent::GoBottom => {
            match state.focus {
                FocusPane::FileList => {
                    let last = state.filtered_file_indices().len().saturating_sub(1);
                    if state.file_selected != last {
                        state.file_selected = last;
                        return Some(Command::LoadDiff);
                    }
                }
                FocusPane::DiffView => {
                    state.diff_scroll = state.diff_total_lines.saturating_sub(1);
                }
            }
            None
        }
        InputEvent::PageUp => {
            let page_size = (state.term_height as usize).saturating_sub(6);
            match state.focus {
                FocusPane::FileList => {
                    let old = state.file_selected;
                    state.file_selected = state.file_selected.saturating_sub(page_size);
                    if state.file_selected != old {
                        return Some(Command::LoadDiff);
                    }
                }
                FocusPane::DiffView => {
                    state.diff_scroll = state.diff_scroll.saturating_sub(page_size);
                }
            }
            None
        }
        InputEvent::PageDown => {
            let page_size = (state.term_height as usize).saturating_sub(6);
            match state.focus {
                FocusPane::FileList => {
                    let old = state.file_selected;
                    let max = state.filtered_file_indices().len().saturating_sub(1);
                    state.file_selected = (state.file_selected + page_size).min(max);
                    if state.file_selected != old {
                        return Some(Command::LoadDiff);
                    }
                }
                FocusPane::DiffView => {
                    let max = state.diff_total_lines.saturating_sub(1);
                    state.diff_scroll = (state.diff_scroll + page_size).min(max);
                }
            }
            None
        }
        InputEvent::SwitchFocus => {
            state.focus = match state.focus {
                FocusPane::FileList => FocusPane::DiffView,
                FocusPane::DiffView => FocusPane::FileList,
            };
            None
        }

        // Hunk navigation
        InputEvent::NextHunk => {
            if let Some(ref diff) = state.current_diff {
                if state.current_hunk + 1 < diff.hunks.len() {
                    state.current_hunk += 1;
                    // Scroll to hunk
                    scroll_to_hunk(state);
                }
            }
            None
        }
        InputEvent::PrevHunk => {
            if state.current_hunk > 0 {
                state.current_hunk -= 1;
                scroll_to_hunk(state);
            }
            None
        }

        // Search
        InputEvent::SearchInDiff => {
            state.overlay = Overlay::Search;
            state.search = SearchState::default();
            None
        }
        InputEvent::SearchInput(c) => {
            state.search.query.push(c);
            update_search_matches(state);
            None
        }
        InputEvent::SearchBackspace => {
            state.search.query.pop();
            update_search_matches(state);
            None
        }
        InputEvent::SearchConfirm | InputEvent::SearchCancel => {
            state.overlay = Overlay::None;
            None
        }
        InputEvent::SearchNext => {
            if !state.search.matches.is_empty() {
                state.search.current_match =
                    (state.search.current_match + 1) % state.search.matches.len();
                scroll_to_search_match(state);
            }
            None
        }
        InputEvent::SearchPrev => {
            if !state.search.matches.is_empty() {
                if state.search.current_match == 0 {
                    state.search.current_match = state.search.matches.len() - 1;
                } else {
                    state.search.current_match -= 1;
                }
                scroll_to_search_match(state);
            }
            None
        }

        // Mode toggles
        InputEvent::ToggleMode => {
            match &state.diff_spec {
                DiffSpec::Worktree(_) => {
                    if state.context.is_none() {
                        state.toast = Some(Toast::new(
                            "Context not loaded yet",
                            Duration::from_secs(2),
                        ));
                        return None;
                    }
                    if state.context.as_ref().is_some_and(|c| c.is_unborn) {
                        state.toast = Some(Toast::new(
                            "Cannot use Compare mode: no commits yet",
                            Duration::from_secs(3),
                        ));
                        return None;
                    }
                    // Switch to Compare mode: prefer default_base (validated against
                    // this repo). Fall back to last_base only if it exists in refs_cache.
                    let base = state.default_base.clone().or_else(|| {
                        let candidate = state.persistent.last_base.clone()?;
                        state
                            .refs_cache
                            .iter()
                            .any(|r| r.name == candidate)
                            .then_some(candidate)
                    });
                    if let Some(base) = base {
                        state.diff_spec = DiffSpec::Compare { base, target: None };
                    } else {
                        state.toast = Some(Toast::new(
                            "No valid base ref found for Compare mode",
                            Duration::from_secs(2),
                        ));
                        return None;
                    }
                }
                DiffSpec::Compare { .. } => {
                    state.diff_spec = DiffSpec::Worktree(WorktreeMode::Unstaged);
                }
            }
            state.generation += 1;
            Some(Command::Reload)
        }
        InputEvent::ToggleStagedUnstaged => {
            if let DiffSpec::Worktree(ref mut mode) = state.diff_spec {
                *mode = match mode {
                    WorktreeMode::Unstaged => WorktreeMode::Staged,
                    WorktreeMode::Staged => WorktreeMode::Unstaged,
                };
                state.generation += 1;
                Some(Command::Reload)
            } else {
                state.toast = Some(Toast::new(
                    "staged/unstaged toggle only in Worktree mode",
                    Duration::from_secs(2),
                ));
                None
            }
        }

        // Selectors
        InputEvent::OpenBaseSelector => {
            if let DiffSpec::Compare { .. } = &state.diff_spec {
                let items = sorted_ref_names(&state.refs_cache);
                state.selector = Some(SelectorState::new(items));
                state.overlay = Overlay::BaseSelector;
            } else {
                state.toast = Some(Toast::new(
                    "Switch to Compare mode first (m)",
                    Duration::from_secs(2),
                ));
            }
            None
        }
        InputEvent::OpenTargetSelector => {
            if let DiffSpec::Compare { .. } = &state.diff_spec {
                let items = sorted_ref_names(&state.refs_cache);
                state.selector = Some(SelectorState::new(items));
                state.overlay = Overlay::TargetSelector;
            } else {
                state.toast = Some(Toast::new(
                    "Switch to Compare mode first (m)",
                    Duration::from_secs(2),
                ));
            }
            None
        }
        InputEvent::OpenWorktreeSelector => {
            let items: Vec<String> = state
                .worktrees_cache
                .iter()
                .map(|w| w.to_string())
                .collect();
            let paths: Vec<String> = state
                .worktrees_cache
                .iter()
                .map(|w| w.path.display().to_string())
                .collect();
            state.selector = Some(SelectorState::with_data(items, paths));
            state.overlay = Overlay::WorktreeSelector;
            None
        }
        InputEvent::OpenContextSelector => Some(Command::OpenContextSelector),

        // Selector interaction
        InputEvent::SelectorInput(c) => {
            if let Some(ref mut sel) = state.selector {
                sel.query.push(c);
                sel.filter();
            }
            None
        }
        InputEvent::SelectorBackspace => {
            if let Some(ref mut sel) = state.selector {
                sel.query.pop();
                sel.filter();
            }
            None
        }
        InputEvent::SelectorMoveDown => {
            if let Some(ref mut sel) = state.selector {
                if sel.selected + 1 < sel.filtered.len() {
                    sel.selected += 1;
                }
            }
            None
        }
        InputEvent::SelectorMoveUp => {
            if let Some(ref mut sel) = state.selector {
                if sel.selected > 0 {
                    sel.selected -= 1;
                }
            }
            None
        }
        InputEvent::SelectorConfirm => {
            let overlay = state.overlay.clone();
            // For worktree/context selectors, use data (path) instead of label
            let selected = match overlay {
                Overlay::WorktreeSelector | Overlay::ContextSelector => state
                    .selector
                    .as_ref()
                    .and_then(|s| s.selected_data())
                    .map(|s| s.to_string()),
                _ => state
                    .selector
                    .as_ref()
                    .and_then(|s| s.selected_item())
                    .map(|s| s.to_string()),
            };
            state.overlay = Overlay::None;
            state.selector = None;

            if let Some(selected) = selected {
                match overlay {
                    Overlay::BaseSelector => {
                        if let DiffSpec::Compare { ref mut base, .. } = state.diff_spec {
                            *base = selected;
                        }
                        state.generation += 1;
                        return Some(Command::Reload);
                    }
                    Overlay::TargetSelector => {
                        if let DiffSpec::Compare { ref mut target, .. } = state.diff_spec {
                            *target = Some(selected);
                        }
                        state.generation += 1;
                        return Some(Command::Reload);
                    }
                    Overlay::WorktreeSelector => {
                        return Some(Command::SwitchWorktree(selected));
                    }
                    Overlay::ContextSelector => {
                        return Some(Command::SwitchContext(selected));
                    }
                    _ => {}
                }
            }
            None
        }
        InputEvent::SelectorCancel => {
            state.overlay = Overlay::None;
            state.selector = None;
            None
        }

        // File filter
        InputEvent::OpenFileFilter => {
            state.file_filter.clear();
            state.overlay = Overlay::FileFilter;
            None
        }
        InputEvent::FileFilterInput(c) => {
            state.file_filter.push(c);
            // Reset selection when filter changes
            state.file_selected = 0;
            None
        }
        InputEvent::FileFilterBackspace => {
            state.file_filter.pop();
            state.file_selected = 0;
            None
        }
        InputEvent::FileFilterConfirm => {
            state.overlay = Overlay::None;
            // Keep the filter active, load diff for newly selected file
            if !state.files.is_empty() && state.actual_selected_file_index().is_some() {
                return Some(Command::LoadDiff);
            }
            None
        }
        InputEvent::FileFilterCancel => {
            state.file_filter.clear();
            state.overlay = Overlay::None;
            None
        }

        // Output
        InputEvent::CopyHunk => Some(Command::CopyHunk),
        InputEvent::CopyFileDiff => Some(Command::CopyFileDiff),
        InputEvent::Export => {
            // Default filename based on current file
            let default_name = if let Some(idx) = state.actual_selected_file_index() {
                let file_path = &state.files[idx].path;
                let sanitized = file_path.replace('/', "_");
                format!("{}.patch", sanitized)
            } else {
                "diff.patch".to_string()
            };
            state.export = ExportState {
                path_input: default_name,
            };
            state.overlay = Overlay::Export;
            None
        }
        InputEvent::ExportInput(c) => {
            state.export.path_input.push(c);
            None
        }
        InputEvent::ExportBackspace => {
            state.export.path_input.pop();
            None
        }
        InputEvent::ExportConfirm => {
            let path = state.export.path_input.clone();
            state.overlay = Overlay::None;
            if !path.is_empty() {
                return Some(Command::ExportToFile(path));
            }
            None
        }
        InputEvent::ExportCancel => {
            state.overlay = Overlay::None;
            None
        }

        // Other
        InputEvent::Reload => {
            state.generation += 1;
            Some(Command::Reload)
        }
        InputEvent::Help => {
            state.overlay = Overlay::Help;
            None
        }
    }
}

/// Handle internal (async) events
pub fn handle_internal_event(state: &mut AppState, event: InternalEvent) {
    match event {
        InternalEvent::GitFilesUpdated { generation, files } => {
            if generation < state.generation {
                debug!(
                    "Ignoring stale files update (gen {} < {})",
                    generation, state.generation
                );
                return;
            }
            state.files = files;
            state.invalidate_filtered_cache();
            // Clamp to filtered list length so selection stays valid with active filters
            let filtered_len = state.filtered_file_indices().len();
            state.file_selected = if filtered_len > 0 {
                state.file_selected.min(filtered_len - 1)
            } else {
                0
            };
            state.loading = false;
            state.current_diff = None;
            state.diff_scroll = 0;
            state.current_hunk = 0;
            // Auto-load diff for selected file
            if !state.files.is_empty() {
                state.pending_commands.push(Command::LoadDiff);
            }
        }
        InternalEvent::GitDiffUpdated {
            generation,
            file_path,
            diff,
        } => {
            if generation < state.generation {
                debug!("Ignoring stale diff update");
                return;
            }
            // Drop diff if no file is currently selected (e.g. filter yields empty list)
            // or if it's for a file that is no longer selected
            match state.actual_selected_file_index() {
                None => {
                    debug!("Ignoring diff update: no file selected");
                    return;
                }
                Some(idx) => {
                    if idx < state.files.len() && state.files[idx].path != file_path {
                        debug!("Ignoring diff for deselected file: {}", file_path);
                        return;
                    }
                }
            }
            // Calculate total lines for scrolling, accounting for truncation
            let raw_total: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
            let truncate_max = state.config.truncate_max_lines;
            let total = if raw_total > truncate_max {
                truncate_max + 1 // +1 for truncation message
            } else {
                raw_total
            };
            state.diff_total_lines = total;
            state.current_diff = Some(diff);
            state.diff_scroll = 0;
            state.current_hunk = 0;
            // Clear stale search matches from previous diff
            state.search.matches.clear();
            state.search.current_match = 0;
        }
        InternalEvent::GitRefsLoaded { refs } => {
            state.refs_cache = refs;
        }
        InternalEvent::GitWorktreesLoaded { worktrees } => {
            state.worktrees_cache = worktrees;
        }
        InternalEvent::ContextResolved {
            context,
            default_base,
        } => {
            state.context = Some(context);
            state.default_base = default_base.clone();

            // Restore diff spec from persistence
            match state.persistent.last_mode.as_deref() {
                Some("compare") => {
                    if !state.context.as_ref().is_some_and(|c| c.is_unborn) {
                        // Use the discovered default base (validated against
                        // this repo's refs). Do NOT fall back to the global
                        // persisted last_base — it may reference a ref that
                        // does not exist in this repository.
                        if let Some(base) = default_base {
                            state.diff_spec = DiffSpec::Compare { base, target: None };
                        }
                        // If no valid base was found, stay in Worktree mode
                        // rather than risk an invalid ref.
                    }
                }
                _ => {
                    let wt_mode = match state.persistent.last_worktree_mode.as_deref() {
                        Some("staged") => WorktreeMode::Staged,
                        _ => WorktreeMode::Unstaged,
                    };
                    state.diff_spec = DiffSpec::Worktree(wt_mode);
                }
            }

            state.generation += 1;
            state.pending_commands.push(Command::InitialLoad);
        }
        InternalEvent::PanesDiscovered { panes } => {
            state.pane_candidates = panes;
            // If the context selector is currently open, rebuild it so
            // newly discovered panes appear without reopening.
            // Use rebuild_context_selector directly (not OpenContextSelector)
            // to avoid re-triggering pane discovery in a loop.
            if state.overlay == Overlay::ContextSelector {
                commands::rebuild_context_selector(state);
            }
        }
        InternalEvent::WatchTriggered => {
            state.generation += 1;
            state.loading = true;
            state.pending_commands.push(Command::Reload);
        }
        InternalEvent::Error(msg) => {
            state.last_error = Some(msg.clone());
            state.loading = false;
            state.toast = Some(Toast::new(msg, Duration::from_secs(5)));
        }
        InternalEvent::Toast(msg) => {
            state.toast = Some(Toast::new(msg, Duration::from_secs(3)));
        }
    }
}

/// Sort refs: local branches first, then remote branches, then tags
fn sorted_ref_names(refs: &[RefEntry]) -> Vec<String> {
    let mut local: Vec<&str> = Vec::new();
    let mut remote: Vec<&str> = Vec::new();
    let mut tags: Vec<&str> = Vec::new();

    for r in refs {
        match r.kind {
            RefKind::Tag => tags.push(&r.name),
            _ if r.is_remote => remote.push(&r.name),
            _ => local.push(&r.name),
        }
    }

    local.sort_unstable();
    remote.sort_unstable();
    tags.sort_unstable();

    let mut result: Vec<String> = Vec::with_capacity(local.len() + remote.len() + tags.len());
    result.extend(local.into_iter().map(String::from));
    result.extend(remote.into_iter().map(String::from));
    result.extend(tags.into_iter().map(String::from));
    result
}

fn scroll_to_hunk(state: &mut AppState) {
    if let Some(ref diff) = state.current_diff {
        let mut line_offset = 0;
        for (i, hunk) in diff.hunks.iter().enumerate() {
            if i == state.current_hunk {
                state.diff_scroll =
                    line_offset.min(state.diff_total_lines.saturating_sub(1));
                break;
            }
            line_offset += hunk.lines.len();
        }
    }
}

fn update_search_matches(state: &mut AppState) {
    state.search.matches.clear();
    state.search.current_match = 0;
    if state.search.query.is_empty() {
        return;
    }
    if let Some(ref diff) = state.current_diff {
        let query_lower = state.search.query.to_lowercase();
        for (hi, hunk) in diff.hunks.iter().enumerate() {
            for (li, line) in hunk.lines.iter().enumerate() {
                if line.content.to_lowercase().contains(&query_lower) {
                    state.search.matches.push((hi, li));
                }
            }
        }
    }
}

fn scroll_to_search_match(state: &mut AppState) {
    if state.search.matches.is_empty() {
        return;
    }
    let (hunk_idx, line_idx) = state.search.matches[state.search.current_match];
    state.current_hunk = hunk_idx;
    // Scroll to the exact matched line rather than the hunk header
    if let Some(ref diff) = state.current_diff {
        let mut line_offset = 0;
        for (i, hunk) in diff.hunks.iter().enumerate() {
            if i == hunk_idx {
                let target = line_offset + line_idx;
                state.diff_scroll = target.min(state.diff_total_lines.saturating_sub(1));
                return;
            }
            line_offset += hunk.lines.len();
        }
    }
    scroll_to_hunk(state);
}
