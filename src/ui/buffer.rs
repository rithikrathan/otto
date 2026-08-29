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

fn parse_markdown<'a>(doc: &'a str, theme: &theme::Theme, width: u16) -> ratatui::text::Text<'a> {
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    let mut in_code = false;

    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            lines.push(Line::from(Span::styled(line.to_string(), theme.markdown_hr())));
            continue;
        }

        if in_code {
            lines.push(Line::from(Span::styled(line.to_string(), theme.markdown_code())));
            continue;
        }

        if trimmed == "---" {
            let hr = "─".repeat(width as usize);
            lines.push(Line::from(Span::styled(hr, theme.markdown_hr())));
            continue;
        }

        let mut is_bold = false;
        let mut spans = Vec::new();
        let mut parts = line.split("**");
        if let Some(first) = parts.next() {
            let is_header = first.starts_with("# ") || first.starts_with("## ") || first.starts_with("### ");
            let base_style = if is_header { theme.emphasis() } else { theme.base() };
            
            spans.push(Span::styled(first.to_string(), base_style));
            
            for part in parts {
                is_bold = !is_bold;
                let style = if is_bold { theme.emphasis() } else { base_style };
                spans.push(Span::styled(part.to_string(), style));
            }
        }
        lines.push(Line::from(spans));
    }
    
    ratatui::text::Text::from(lines)
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
