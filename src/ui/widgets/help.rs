use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn draw_help(f: &mut Frame, area: Rect) {
    let width = area.width.min(65);
    let height = area.height.min(34);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);

    f.render_widget(Clear, rect);

    let block = Block::default()
        .title(" Help (q/? to close) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default().fg(Color::White);

    let lines = vec![
        Line::from(Span::styled("Navigation", header_style)),
        help_line("j/k, Up/Down", "Move up/down", key_style, desc_style),
        help_line("g/G, Home/End", "Go to top/bottom", key_style, desc_style),
        help_line("PgUp/PgDn", "Page up/down", key_style, desc_style),
        help_line(
            "Tab",
            "Switch focus (files <-> diff)",
            key_style,
            desc_style,
        ),
        Line::from(""),
        Line::from(Span::styled("Diff", header_style)),
        help_line("n/p", "Next/previous hunk", key_style, desc_style),
        help_line("N/P", "Next/previous search match", key_style, desc_style),
        help_line("f", "Search in diff", key_style, desc_style),
        help_line("/", "Filter files", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("Mode", header_style)),
        help_line("m", "Toggle Worktree <-> Compare", key_style, desc_style),
        help_line("s", "Toggle unstaged <-> staged", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("Selectors", header_style)),
        help_line("b", "Base ref selector (Compare)", key_style, desc_style),
        help_line("t", "Target ref selector (Compare)", key_style, desc_style),
        help_line("w", "Worktree selector", key_style, desc_style),
        help_line("c", "Context selector", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("Output", header_style)),
        help_line("y", "Copy current hunk", key_style, desc_style),
        help_line("Y", "Copy file diff", key_style, desc_style),
        help_line("o", "Export diff", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("Other", header_style)),
        help_line("r", "Reload", key_style, desc_style),
        help_line("?", "This help", key_style, desc_style),
        help_line("q/Esc", "Quit", key_style, desc_style),
    ];

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, rect);
}

fn help_line<'a>(key: &'a str, desc: &'a str, key_style: Style, desc_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {:14}", key), key_style),
        Span::styled(desc, desc_style),
    ])
}
