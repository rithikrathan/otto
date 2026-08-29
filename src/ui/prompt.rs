//! The expandable prompt box: wraps, grows to 5 lines, then scrolls internally.

use crate::app::input::Prompt;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::Theme;

/// Draw the prompt box into `area`, honoring `prompt.scroll` (first visible row).
pub fn draw(frame: &mut Frame, prompt: &mut Prompt, area: Rect, theme: &Theme) {
    let width = area.width.saturating_sub(4) as usize; // minus borders and horizontal padding
    if width == 0 { return; }
    prompt.width = width;

    let mut lines = Vec::new();
    let mut row: usize = 0;
    let mut col: usize = 0;
    let mut current_line = String::new();
    let mut cursor_row: usize = 0;
    let mut cursor_col: usize = 0;

    let text_len = prompt.text.len();
    for (i, ch) in prompt.text.char_indices() {
        if i == prompt.cursor {
            cursor_row = row;
            cursor_col = col;
        }
        if ch == '\n' {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            row += 1;
            col = 0;
        } else if col >= width {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            current_line.push(ch);
            row += 1;
            col = 1;
        } else {
            current_line.push(ch);
            col += 1;
        }
    }
    if prompt.cursor >= text_len {
        cursor_row = row;
        cursor_col = col;
    }
    
    let mut last_spans = vec![ratatui::text::Span::raw(current_line)];
    
    // Ghost text for slash command autocomplete
    if prompt.cursor == text_len {
        if let Some(comp) = crate::cmd::autocomplete(&prompt.text) {
            last_spans.push(ratatui::text::Span::styled(comp, theme.muted()));
        }
    }
    
    lines.push(Line::from(last_spans));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" input ")
        .title_style(theme.muted())
        .border_style(theme.muted())
        .padding(ratatui::widgets::Padding::horizontal(1));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((prompt.scroll as u16, 0));

    frame.render_widget(paragraph, area);

    // Draw the cursor manually using a reversed block to prevent hardware cursor flicker.
    let x = area.x + 2 + cursor_col as u16; // 1 for border, 1 for padding
    let y = area.y + 1 + cursor_row.saturating_sub(prompt.scroll) as u16;
    if x < area.x + area.width && y < area.y + area.height {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            let symbol = cell.symbol();
            if symbol == " " || symbol.is_empty() {
                cell.set_symbol("█");
                // match the muted theme color or just default
                cell.set_fg(theme.muted().fg.unwrap_or(ratatui::style::Color::DarkGray));
            } else {
                cell.set_style(cell.style().add_modifier(ratatui::style::Modifier::REVERSED));
            }
        }
    }
}
