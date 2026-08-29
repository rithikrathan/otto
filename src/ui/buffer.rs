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

fn tokenize_markdown_line<'a>(text: &str, theme: &theme::Theme) -> Vec<ratatui::text::Span<'a>> {
    use ratatui::text::Span;
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            let mut bold_text = String::new();
            while let Some(ic) = chars.next() {
                if ic == '*' && chars.peek() == Some(&'*') {
                    chars.next();
                    break;
                }
                bold_text.push(ic);
            }
            spans.push(Span::styled(bold_text, theme.emphasis()));
        } else if c == '`' {
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            let mut code_text = String::new();
            while let Some(ic) = chars.next() {
                if ic == '`' {
                    break;
                }
                code_text.push(ic);
            }
            spans.push(Span::styled(code_text, theme.markdown_code().bg(ratatui::style::Color::Rgb(40, 40, 40))));
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        spans.push(Span::raw(current));
    }
    spans
}

fn parse_markdown<'a>(doc: &'a str, theme: &theme::Theme, width: u16) -> ratatui::text::Text<'a> {
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();

    let width_usize = width as usize;
    let code_bg = ratatui::style::Color::Rgb(30, 30, 30);

    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                let mut bottom = String::from("╰");
                bottom.push_str(&"─".repeat(width_usize.saturating_sub(1)));
                lines.push(Line::from(Span::styled(bottom, theme.muted().bg(code_bg))));
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
                lines.push(Line::from(Span::styled(top, theme.muted().bg(code_bg))));
            }
            continue;
        }

        if in_code_block {
            let mut text = line.to_string();
            let display_width = text.chars().count() + 2;
            if display_width < width_usize {
                text.push_str(&" ".repeat(width_usize.saturating_sub(display_width)));
            }
            
            lines.push(Line::from(vec![
                Span::styled("│ ", theme.muted().bg(code_bg)),
                Span::styled(text, theme.markdown_code().bg(code_bg)),
            ]));
            continue;
        }

        if trimmed == "---" {
            let hr = "─".repeat(width_usize);
            lines.push(Line::from(Span::styled(hr, theme.muted())));
            continue;
        }

        if trimmed.starts_with("# ") {
            let header = trimmed.strip_prefix("# ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Cyan))));
            continue;
        } else if trimmed.starts_with("## ") {
            let header = trimmed.strip_prefix("## ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::LightCyan))));
            continue;
        } else if trimmed.starts_with("### ") {
            let header = trimmed.strip_prefix("### ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis())));
            continue;
        }

        lines.push(Line::from(tokenize_markdown_line(line, theme)));
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
