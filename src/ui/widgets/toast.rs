use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn draw_toast(f: &mut Frame, area: Rect, message: &str) {
    let msg_len = message.chars().count().min(u16::MAX as usize) as u16;
    let width = msg_len.saturating_add(4).max(10).min(area.width);
    let x = area.x + area.width.saturating_sub(width).saturating_sub(1);
    let y = area.y + area.height.saturating_sub(4);
    let rect = Rect::new(x, y, width, 3);

    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(message)
        .block(block)
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));

    f.render_widget(para, rect);
}
