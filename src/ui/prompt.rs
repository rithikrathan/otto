//! The expandable prompt box: wraps, grows to 5 lines, then scrolls internally.

use crate::app::input::Prompt;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::Theme;

/// Draw the prompt box into `area`, honoring `prompt.scroll` (first visible row).
pub fn draw(frame: &mut Frame, prompt: &mut Prompt, area: Rect, theme: &Theme) {
    let width = area.width.saturating_sub(2) as usize; // minus borders
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
    lines.push(Line::from(current_line));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" input ")
        .title_style(theme.muted())
        .border_style(theme.muted());

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((prompt.scroll as u16, 0));

    frame.render_widget(paragraph, area);

    // Draw the cursor onto the caret position.
    let x = area.x + 1 + cursor_col as u16;
    let y = area.y + 1 + cursor_row.saturating_sub(prompt.scroll) as u16;
    if x < area.x + area.width && y < area.y + area.height {
        frame.set_cursor_position((x, y));
    }
}
