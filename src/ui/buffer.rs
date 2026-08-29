//! Render the active buffer's markdown history as a scrollable region.

use crate::app::App;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::theme;

/// Assemble the active buffer's blocks into a single markdown document.
fn active_document(app: &App) -> String {
    let mut doc = String::new();
    match app.active_buffer() {
        crate::buffers::BufferId::Chat => {
            for b in &app.chat.view.blocks {
                doc.push_str(&format!("**{}:**\n\n{}\n\n---\n\n", b.kind, b.markdown));
            }
        }
        crate::buffers::BufferId::Search => {
            for b in &app.search.view.blocks {
                doc.push_str(&format!("{}\n\n---\n\n", b.markdown));
            }
        }
        crate::buffers::BufferId::Chtsh => {
            for b in &app.chtsh.view.blocks {
                doc.push_str(&format!("{}\n\n---\n\n", b.markdown));
            }
        }
    }
    doc
}

fn parse_markdown<'a>(doc: &'a str, _theme: &theme::Theme, _width: u16) -> ratatui::text::Text<'a> {
    tui_markdown::from_str(doc)
}

/// Draw the scrollable buffer region for the active buffer.
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let theme = theme::current();

    let doc = active_document(app);
    let inner_width = area.width.saturating_sub(1); // Account for left border
    let md = parse_markdown(&doc, &theme, inner_width);

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme.muted());

    let paragraph = Paragraph::new(md)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.active_scroll(), 0));

    frame.render_widget(paragraph, area);
}
