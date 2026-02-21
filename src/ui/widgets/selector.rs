use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::state::SelectorState;

pub fn draw_selector(f: &mut Frame, area: Rect, sel: &SelectorState, title: &str) {
    // Center the selector
    let width = area.width.clamp(30, 70);
    let height = area.height.clamp(5, 20);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);

    // Clear background
    f.render_widget(Clear, rect);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(rect);

    // Split: 1 line for query, rest for items
    let query_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let list_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );

    // Draw block
    f.render_widget(block, rect);

    // Draw query
    let query_display = if sel.query.is_empty() {
        Span::styled("Type to filter...", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(&sel.query, Style::default().fg(Color::White))
    };
    let query_para = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        query_display,
    ]));
    f.render_widget(query_para, query_area);

    // Draw items
    let visible_height = list_area.height as usize;
    let scroll = if sel.selected >= visible_height {
        sel.selected - visible_height + 1
    } else {
        0
    };

    let items: Vec<ListItem> = sel
        .filtered
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(i, &idx)| {
            let item = &sel.items[idx];
            let style = if i == sel.selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(item.as_str(), style)))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, list_area);
}
