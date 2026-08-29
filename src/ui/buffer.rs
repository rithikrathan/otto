//! Render the active buffer's markdown history as a scrollable region.

use crate::app::App;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::theme;

/// Assemble the active buffer's blocks into a single markdown document.
fn active_document(app: &App) -> String {
    let mut doc = String::new();

    let push_block = |doc: &mut String, prefix: Option<&str>, markdown: &str, suffix: &str| {
        let unclosed = markdown.lines().filter(|l| l.trim().starts_with("```")).count() % 2 != 0;
        if let Some(p) = prefix {
            doc.push_str(p);
        }
        doc.push_str(markdown);
        if unclosed {
            doc.push_str("\n```\n");
        }
        doc.push_str(suffix);
    };

    match app.active_buffer() {
        crate::buffers::BufferId::Chat => {
            for b in &app.chat.view.blocks {
                let prefix = format!("**{}:**\n\n", b.kind);
                push_block(&mut doc, Some(&prefix), &b.markdown, "\n\n---\n\n");
            }
        }
        crate::buffers::BufferId::Search => {
            for b in &app.search.view.blocks {
                push_block(&mut doc, None, &b.markdown, "\n\n---\n\n");
            }
        }
        crate::buffers::BufferId::Chtsh => {
            for b in &app.chtsh.view.blocks {
                push_block(&mut doc, None, &b.markdown, "\n\n---\n\n");
            }
        }
    }
    doc
}

use std::sync::OnceLock;

static SYNTAX_SET: OnceLock<syntect::parsing::SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<syntect::highlighting::ThemeSet> = OnceLock::new();

fn get_syntax_set() -> &'static syntect::parsing::SyntaxSet {
    SYNTAX_SET.get_or_init(|| syntect::parsing::SyntaxSet::load_defaults_newlines())
}

fn get_theme_set() -> &'static syntect::highlighting::ThemeSet {
    THEME_SET.get_or_init(|| syntect::highlighting::ThemeSet::load_defaults())
}

fn syntect_style_to_ratatui(style: syntect::highlighting::Style) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(ratatui::style::Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b))
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
            spans.push(Span::styled(bold_text, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 158, 100))));
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

fn wrap_spans<'a>(
    regions: Vec<(syntect::highlighting::Style, &str)>,
    max_width: usize,
    theme: &theme::Theme,
) -> Vec<ratatui::text::Line<'a>> {
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    let prefix = "│ ";
    let wrap_width = max_width.saturating_sub(prefix.chars().count());
    if wrap_width == 0 {
        return lines;
    }

    let mut current_line = vec![Span::styled(prefix, theme.muted())];
    let mut current_len = 0;

    for (style, text) in regions {
        let ratatui_style = syntect_style_to_ratatui(style);
        let mut text_remain = text;

        while !text_remain.is_empty() {
            let space_left = wrap_width.saturating_sub(current_len);
            if space_left == 0 {
                lines.push(Line::from(current_line));
                current_line = vec![Span::styled(prefix, theme.muted())];
                current_len = 0;
                continue;
            }

            let mut char_count = 0;
            let mut split_idx = text_remain.len();
            for (i, c) in text_remain.char_indices() {
                if char_count == space_left {
                    split_idx = i;
                    break;
                }
                char_count += 1;
            }

            let chunk = &text_remain[..split_idx];
            current_line.push(Span::styled(chunk.to_string(), ratatui_style));
            current_len += char_count;
            text_remain = &text_remain[split_idx..];
        }
    }

    if current_len > 0 {
        lines.push(Line::from(current_line));
    } else if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled(prefix, theme.muted())]));
    }

    lines
}

fn parse_markdown<'a>(doc: &'a str, theme: &theme::Theme, width: u16) -> ratatui::text::Text<'a> {
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    let mut in_code_block = false;

    let width_usize = width as usize;
    let mut highlighter: Option<syntect::easy::HighlightLines> = None;

    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                let mut bottom = String::from("╰");
                bottom.push_str(&"─".repeat(width_usize.saturating_sub(1)));
                lines.push(Line::from(Span::styled(bottom, theme.muted())));
                in_code_block = false;
                highlighter = None;
            } else {
                in_code_block = true;
                let code_lang = trimmed.strip_prefix("```").unwrap_or("").trim().to_string();
                
                let mut top = String::from("╭─ ");
                if !code_lang.is_empty() {
                    top.push_str(&code_lang);
                    top.push_str(" ");
                }
                top.push_str(&"─".repeat(width_usize.saturating_sub(top.chars().count())));
                lines.push(Line::from(Span::styled(top, theme.muted())));

                let ps = get_syntax_set();
                let ts = get_theme_set();
                let syntax = ps.find_syntax_by_token(&code_lang).unwrap_or_else(|| ps.find_syntax_plain_text());
                highlighter = Some(syntect::easy::HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]));
            }
            continue;
        }

        if in_code_block {
            let text = line.to_string();
            
            if let Some(ref mut hl) = highlighter {
                if let Ok(regions) = hl.highlight_line(&text, get_syntax_set()) {
                    lines.extend(wrap_spans(regions, width_usize, theme));
                    continue;
                }
            }
            
            // Fallback if highlight fails
            let regions = vec![(syntect::highlighting::Style::default(), text.as_str())];
            lines.extend(wrap_spans(regions, width_usize, theme));
            continue;
        }

        if trimmed == "---" {
            let hr = "─".repeat(width_usize);
            lines.push(Line::from(Span::styled(hr, theme.muted())));
            continue;
        }

        if trimmed.starts_with("# ") {
            let header = trimmed.strip_prefix("# ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 30, 0)))));
            continue;
        } else if trimmed.starts_with("## ") {
            let header = trimmed.strip_prefix("## ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 66, 15)))));
            continue;
        } else if trimmed.starts_with("### ") {
            let header = trimmed.strip_prefix("### ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 99, 71)))));
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
