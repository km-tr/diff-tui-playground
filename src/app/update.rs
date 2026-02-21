use std::time::Duration;

use tracing::debug;

use super::commands::Command;
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
                    if !state.files.is_empty() && state.file_selected < state.files.len() - 1 {
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
                    let last = state.files.len().saturating_sub(1);
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
                    let max = state.files.len().saturating_sub(1);
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
                    if state.context.as_ref().is_none_or(|c| c.is_unborn) {
                        state.toast = Some(Toast::new(
                            "Cannot use Compare mode: no commits yet",
                            Duration::from_secs(3),
                        ));
                        return None;
                    }
                    // Switch to Compare mode
                    let base = state
                        .persistent
                        .last_base
                        .clone()
                        .unwrap_or_else(|| "main".to_string());
                    state.diff_spec = DiffSpec::Compare { base, target: None };
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
                let items: Vec<String> = state.refs_cache.iter().map(|r| r.name.clone()).collect();
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
                let items: Vec<String> = state.refs_cache.iter().map(|r| r.name.clone()).collect();
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
            state.selector = Some(SelectorState::new(items));
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
            let selected = state
                .selector
                .as_ref()
                .and_then(|s| s.selected_item())
                .map(|s| s.to_string());
            let overlay = state.overlay.clone();
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

        // Output
        InputEvent::CopyHunk => Some(Command::CopyHunk),
        InputEvent::CopyFileDiff => Some(Command::CopyFileDiff),
        InputEvent::Export => {
            state.overlay = Overlay::Export;
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

        InputEvent::Resize(w, h) => {
            state.term_width = w;
            state.term_height = h;
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
            state.file_selected = state.file_selected.min(state.files.len().saturating_sub(1));
            state.loading = false;
            // Auto-load diff for first file
            if !state.files.is_empty() {
                // Trigger diff load via command
                state.current_diff = None;
                state.diff_scroll = 0;
                state.current_hunk = 0;
            }
        }
        InternalEvent::GitDiffUpdated {
            generation,
            file_path: _,
            diff,
        } => {
            if generation < state.generation {
                debug!("Ignoring stale diff update");
                return;
            }
            // Calculate total lines for scrolling
            let total: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
            state.diff_total_lines = total;
            state.current_diff = Some(diff);
            state.diff_scroll = 0;
            state.current_hunk = 0;
        }
        InternalEvent::GitRefsLoaded { refs } => {
            state.refs_cache = refs;
        }
        InternalEvent::GitWorktreesLoaded { worktrees } => {
            state.worktrees_cache = worktrees;
        }
        InternalEvent::ContextResolved { context } => {
            state.context = Some(context);
            state.generation += 1;
        }
        InternalEvent::PanesDiscovered { panes } => {
            state.pane_candidates = panes;
        }
        InternalEvent::WatchTriggered => {
            state.generation += 1;
            state.loading = true;
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

fn scroll_to_hunk(state: &mut AppState) {
    if let Some(ref diff) = state.current_diff {
        let mut line_offset = 0;
        for (i, hunk) in diff.hunks.iter().enumerate() {
            if i == state.current_hunk {
                state.diff_scroll = line_offset;
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
    let (hunk_idx, _line_idx) = state.search.matches[state.search.current_match];
    state.current_hunk = hunk_idx;
    scroll_to_hunk(state);
}
