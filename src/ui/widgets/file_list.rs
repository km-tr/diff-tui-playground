use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::state::AppState;
use crate::git::model::FileStatus;

pub fn draw_file_list(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused {
        Color::Blue
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Files ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if state.files.is_empty() {
        let msg = if state.loading {
            "Loading..."
        } else if state.context.is_none() {
            "No repository"
        } else {
            "No changes"
        };
        let para = ratatui::widgets::Paragraph::new(msg)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(para, area);
        return;
    }

    // Visible height
    let inner_height = area.height.saturating_sub(2) as usize;

    // Adjust scroll to keep selection visible
    let scroll = {
        let mut scroll = state.file_scroll;
        if state.file_selected < scroll {
            scroll = state.file_selected;
        }
        if state.file_selected >= scroll + inner_height {
            scroll = state.file_selected.saturating_sub(inner_height - 1);
        }
        scroll
    };

    let items: Vec<ListItem> = state
        .files
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height)
        .map(|(i, file)| {
            let status_color = match file.status {
                FileStatus::Added => Color::Green,
                FileStatus::Modified => Color::Yellow,
                FileStatus::Deleted => Color::Red,
                FileStatus::Renamed => Color::Cyan,
                FileStatus::Copied => Color::Cyan,
                FileStatus::Untracked => Color::Magenta,
                _ => Color::White,
            };

            let status_char = match file.status {
                FileStatus::Added => "A",
                FileStatus::Modified => "M",
                FileStatus::Deleted => "D",
                FileStatus::Renamed => "R",
                FileStatus::Copied => "C",
                FileStatus::Untracked => "?",
                FileStatus::TypeChanged => "T",
                FileStatus::Unknown => " ",
            };

            let stat = if file.additions > 0 || file.deletions > 0 {
                format!(" +{}/-{}", file.additions, file.deletions)
            } else {
                String::new()
            };

            let style = if i == state.file_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", status_char),
                    Style::default().fg(status_color),
                ),
                Span::styled(&file.path, style),
                Span::styled(stat, Style::default().fg(Color::DarkGray)),
            ]);

            ListItem::new(line).style(if i == state.file_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            })
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}
