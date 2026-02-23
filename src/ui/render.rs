use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::*;
use crate::git::model::*;
use crate::util::path::display_path;

use super::widgets::{diff_view, error_panel, file_list, help, selector, toast};

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let size = f.area();

    // Main layout: Header | Body | Footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Body
            Constraint::Length(1), // Footer
        ])
        .split(size);

    draw_header(f, main_chunks[0], state);
    draw_body(f, main_chunks[1], state);
    draw_footer(f, main_chunks[2], state);

    // Overlays
    match &state.overlay {
        Overlay::Help => {
            help::draw_help(f, size);
        }
        Overlay::BaseSelector
        | Overlay::TargetSelector
        | Overlay::WorktreeSelector
        | Overlay::ContextSelector => {
            if let Some(ref sel) = state.selector {
                let title = match &state.overlay {
                    Overlay::BaseSelector => "Select Base Ref",
                    Overlay::TargetSelector => "Select Target Ref",
                    Overlay::WorktreeSelector => "Select Worktree",
                    Overlay::ContextSelector => "Select Context",
                    _ => "Select",
                };
                selector::draw_selector(f, size, sel, title);
            }
        }
        Overlay::Search => {
            draw_search_bar(f, size, state);
        }
        Overlay::FileFilter => {
            draw_file_filter_bar(f, size, state);
        }
        Overlay::Export => {
            draw_export_dialog(f, size, state);
        }
        Overlay::None => {}
    }

    // Toast
    if let Some(ref t) = state.toast {
        if !t.is_expired() {
            toast::draw_toast(f, size, &t.message);
        }
    }

    // Error panel
    if let Some(ref err) = state.last_error {
        if state.context.is_none() {
            error_panel::draw_error(f, main_chunks[1], err);
        }
    }
}

fn draw_header(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: Context info
    let ctx_text = if let Some(ref ctx) = state.context {
        let path = display_path(&ctx.worktree_path);
        let branch = ctx
            .current_branch
            .as_deref()
            .unwrap_or(ctx.head_ref.as_deref().unwrap_or("(no HEAD)"));
        let status = if ctx.is_unborn {
            " [unborn]"
        } else if ctx.is_detached {
            " [detached]"
        } else {
            ""
        };
        format!("{} @ {}{}", path, branch, status)
    } else {
        "No repository".to_string()
    };

    let context_block = Block::default()
        .title(" Context ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let context_para = Paragraph::new(ctx_text)
        .block(context_block)
        .style(Style::default().fg(Color::White));
    f.render_widget(context_para, chunks[0]);

    // Right: DiffSpec + summary
    let spec_text = format!("{}", state.diff_spec);
    let summary = if state.files.is_empty() {
        if state.loading {
            "Loading...".to_string()
        } else {
            "No changes".to_string()
        }
    } else {
        let total_add: u32 = state.files.iter().map(|f| f.additions).sum();
        let total_del: u32 = state.files.iter().map(|f| f.deletions).sum();
        format!("{} files  +{}/-{}", state.files.len(), total_add, total_del)
    };

    let diff_block = Block::default()
        .title(" DiffSpec ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let diff_para = Paragraph::new(format!("{}  {}", spec_text, summary))
        .block(diff_block)
        .style(Style::default().fg(Color::White));
    f.render_widget(diff_para, chunks[1]);
}

fn draw_body(f: &mut Frame, area: Rect, state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // Left: File list
    let is_focused = state.focus == FocusPane::FileList;
    file_list::draw_file_list(f, chunks[0], state, is_focused);

    // Right: Diff viewer
    let is_focused = state.focus == FocusPane::DiffView;
    diff_view::draw_diff_view(f, chunks[1], state, is_focused);
}

fn draw_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let keys = match state.overlay {
        Overlay::None => {
            let mode_key = match &state.diff_spec {
                DiffSpec::Worktree(_) => "s:staged/unstaged",
                DiffSpec::Compare { .. } => "b:base t:target",
            };
            format!(
                " j/k:move  n/p:hunk  Tab:focus  m:mode  {}  c:context  w:worktree  /:filter  y:copy  o:export  ?:help  q:quit",
                mode_key
            )
        }
        Overlay::Help => " q/?:close ".to_string(),
        Overlay::Search => " Enter:close  Ctrl+n/p:next/prev ".to_string(),
        Overlay::FileFilter => " Enter:apply  Esc:cancel  type to filter ".to_string(),
        Overlay::Export => " Enter:export  Esc:cancel ".to_string(),
        _ => " Enter:select  Esc:cancel  type to filter ".to_string(),
    };

    let footer = Paragraph::new(Line::from(vec![Span::styled(
        keys,
        Style::default().fg(Color::DarkGray),
    )]));
    f.render_widget(footer, area);
}

fn draw_search_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let width = area.width.min(60);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = area.height.saturating_sub(4);
    let rect = Rect::new(x, y, width, 3);

    let match_info = if state.search.matches.is_empty() {
        if state.search.query.is_empty() {
            String::new()
        } else {
            " (no matches)".to_string()
        }
    } else {
        format!(
            " ({}/{})",
            state.search.current_match + 1,
            state.search.matches.len()
        )
    };

    let block = Block::default()
        .title(format!(" Search{} ", match_info))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let para = Paragraph::new(state.search.query.as_str())
        .block(block)
        .style(Style::default().fg(Color::White));

    // Clear area
    f.render_widget(ratatui::widgets::Clear, rect);
    f.render_widget(para, rect);
}

fn draw_file_filter_bar(f: &mut Frame, area: Rect, state: &mut AppState) {
    let width = area.width.min(60);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = area.height.saturating_sub(4);
    let rect = Rect::new(x, y, width, 3);

    let count = state.filtered_file_indices().len();
    let info = if state.file_filter.is_empty() {
        String::new()
    } else {
        format!(" ({} matches)", count)
    };

    let block = Block::default()
        .title(format!(" Filter Files{} ", info))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let para = Paragraph::new(state.file_filter.as_str())
        .block(block)
        .style(Style::default().fg(Color::White));

    f.render_widget(ratatui::widgets::Clear, rect);
    f.render_widget(para, rect);
}

fn draw_export_dialog(f: &mut Frame, area: Rect, state: &AppState) {
    let width = area.width.min(60);
    let height = 5;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);

    let block = Block::default()
        .title(" Export to file (Enter to confirm, Esc to cancel) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = vec![
        Line::from(Span::styled("Path:", Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(
            state.export.path_input.as_str(),
            Style::default().fg(Color::White),
        )),
    ];

    let para = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::White));

    f.render_widget(ratatui::widgets::Clear, rect);
    f.render_widget(para, rect);
}
