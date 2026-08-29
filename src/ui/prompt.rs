//! The expandable prompt box: wraps, grows to 5 lines, then scrolls internally.

use crate::app::input::Prompt;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::theme::Theme;

/// Draw the prompt box into `area`, honoring `prompt.scroll` (first visible row).
pub fn draw(frame: &mut Frame, prompt: &mut Prompt, area: Rect, theme: &Theme) {
    let width = area.width.saturating_sub(2) as usize; // minus borders
    prompt.width = width;

    let lines = prompt
        .text
        .lines()
        .map(|l| Line::from(l.to_string()))
        .collect::<Vec<_>>();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" input ")
        .title_style(theme.muted())
        .border_style(theme.muted());

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((prompt.scroll as u16, 0));

    frame.render_widget(paragraph, area);

    // Draw the cursor onto the caret position.
    if let Some((row, col)) = caret_position(prompt) {
        let x = area.x + 1 + col as u16;
        let y = area.y + 1 + row.saturating_sub(prompt.scroll) as u16;
        if x < area.x + area.width && y < area.y + area.height {
            frame.set_cursor_position((x, y));
        }
    }
}

/// Compute the caret (row, col) inside the content area (excluding borders).
fn caret_position(prompt: &Prompt) -> Option<(usize, usize)> {
    let before = &prompt.text[..prompt.cursor.min(prompt.text.len())];
    // Count rows from the caret's line + wrapped position.
    let mut row = 0usize;
    let mut col = 0usize;
    let mut chars = 0usize;
    let buf: Vec<(usize, char)> = before.char_indices().collect();
    for (i, (_, ch)) in buf.iter().enumerate() {
        let _ = i;
        if *ch == '\n' {
            row += 1;
            col = 0;
        } else if col >= prompt.width {
            // wrap to next row
            row += 1;
            col = 0;
            col += 1;
        } else {
            col += 1;
        }
        chars += 1;
    }
    let _ = chars;
    Some((row, col))
}
