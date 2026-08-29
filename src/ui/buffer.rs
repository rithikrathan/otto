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
    let mut in_code_block = false;
    let mut code_lang = String::new();

    let width_usize = width as usize;

    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                let mut bottom = String::from("╰");
                bottom.push_str(&"─".repeat(width_usize.saturating_sub(1)));
                lines.push(Line::from(Span::styled(bottom, theme.muted())));
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = trimmed.strip_prefix("```").unwrap_or("").trim().to_string();
                let mut top = String::from("╭─ ");
                if !code_lang.is_empty() {
                    top.push_str(&code_lang);
                    top.push_str(" ");
                }
                top.push_str(&"─".repeat(width_usize.saturating_sub(top.chars().count())));
                lines.push(Line::from(Span::styled(top, theme.muted())));
            }
            continue;
        }

        if in_code_block {
            lines.push(Line::from(vec![
                Span::styled("│ ", theme.muted()),
                Span::styled(line.to_string(), theme.markdown_code()),
            ]));
            continue;
        }

        if trimmed == "---" {
            let hr = "─".repeat(width_usize);
            lines.push(Line::from(Span::styled(hr, theme.muted())));
            continue;
        }

        if trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ") {
            lines.push(Line::from(Span::styled(line.to_string(), theme.emphasis())));
            continue;
        }

        // Bold extraction for header topics (e.g. `**Search:**`)
        if trimmed.starts_with("**") {
            if let Some((bold, rest)) = trimmed[2..].split_once("**") {
                lines.push(Line::from(vec![
                    Span::styled(format!("**{}**", bold), theme.emphasis()),
                    Span::raw(rest.to_string()),
                ]));
                continue;
            }
        }

        lines.push(Line::from(line.to_string()));
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
