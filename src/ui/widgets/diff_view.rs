use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::AppState;
use crate::git::model::{DiffLineKind, FileDiff};

pub fn draw_diff_view(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused {
        Color::Blue
    } else {
        Color::DarkGray
    };

    let title = if let Some(ref diff) = state.current_diff {
        let file_name = if !diff.new_file.is_empty() {
            diff.new_file.strip_prefix("b/").unwrap_or(&diff.new_file)
        } else {
            "Diff"
        };
        let mut title_parts = vec![file_name.to_string()];
        if diff.is_binary {
            title_parts.push("[binary]".to_string());
        }
        if diff.is_new_file {
            title_parts.push("[new]".to_string());
        }
        if diff.is_deleted {
            title_parts.push("[deleted]".to_string());
        }
        if diff.is_rename {
            title_parts.push("[renamed]".to_string());
        }
        format!(" {} ", title_parts.join(" "))
    } else {
        " Diff ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    let visible_height = inner.height as usize;

    match &state.current_diff {
        None => {
            let msg = if state.files.is_empty() {
                "No changes to display"
            } else if state.loading {
                "Loading diff..."
            } else {
                "Select a file to view diff"
            };
            let para = Paragraph::new(msg)
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(para, area);
        }
        Some(diff) => {
            if diff.is_binary {
                let para = Paragraph::new("Binary files differ")
                    .block(block)
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(para, area);
                return;
            }

            // Build all lines with virtual scrolling
            let lines = build_diff_lines(diff, state);

            // Apply scroll offset
            let visible_lines: Vec<Line> = lines
                .into_iter()
                .skip(state.diff_scroll)
                .take(visible_height)
                .collect();

            let para = Paragraph::new(visible_lines).block(block);
            f.render_widget(para, area);
        }
    }
}

fn build_diff_lines<'a>(diff: &'a FileDiff, state: &AppState) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let search_query = if !state.search.query.is_empty() {
        Some(state.search.query.to_lowercase())
    } else {
        None
    };

    let truncate_max = state.config.truncate_max_lines;
    let mut total_lines = 0;

    for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
        let is_current_hunk = hunk_idx == state.current_hunk;

        for (line_idx, diff_line) in hunk.lines.iter().enumerate() {
            if total_lines >= truncate_max {
                lines.push(Line::from(Span::styled(
                    "... (truncated, press 'o' to export full diff)",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::ITALIC),
                )));
                return lines;
            }

            let is_search_match = search_query
                .as_ref()
                .is_some_and(|q| diff_line.content.to_lowercase().contains(q));

            let is_current_search = state
                .search
                .matches
                .get(state.search.current_match)
                .is_some_and(|&(hi, li)| hi == hunk_idx && li == line_idx);

            let (fg, bg) = match diff_line.kind {
                DiffLineKind::HunkHeader => {
                    let marker = if is_current_hunk {
                        Color::Blue
                    } else {
                        Color::Cyan
                    };
                    (marker, Color::Reset)
                }
                DiffLineKind::Addition => (Color::Green, Color::Reset),
                DiffLineKind::Deletion => (Color::Red, Color::Reset),
                DiffLineKind::Context => (Color::White, Color::Reset),
                DiffLineKind::NoNewline => (Color::DarkGray, Color::Reset),
            };

            let mut style = Style::default().fg(fg);
            if bg != Color::Reset {
                style = style.bg(bg);
            }
            if is_current_hunk && diff_line.kind == DiffLineKind::HunkHeader {
                style = style.add_modifier(Modifier::BOLD);
            }
            if is_current_search {
                style = style.bg(Color::Yellow).fg(Color::Black);
            } else if is_search_match {
                style = style.bg(Color::DarkGray);
            }

            lines.push(Line::from(Span::styled(&diff_line.content, style)));
            total_lines += 1;
        }
    }

    lines
}
