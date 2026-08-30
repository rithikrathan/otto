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
static EPHEMERA_THEME: OnceLock<syntect::highlighting::Theme> = OnceLock::new();

fn get_syntax_set() -> &'static syntect::parsing::SyntaxSet {
    SYNTAX_SET.get_or_init(|| syntect::parsing::SyntaxSet::load_defaults_newlines())
}

fn get_ephemera_theme() -> &'static syntect::highlighting::Theme {
    EPHEMERA_THEME.get_or_init(|| {
        let xml = include_str!("../ephemera.tmTheme");
        let mut cursor = std::io::Cursor::new(xml);
        syntect::highlighting::ThemeSet::load_from_reader(&mut cursor).unwrap()
    })
}

fn syntect_style_to_ratatui(style: syntect::highlighting::Style) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(ratatui::style::Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b))
}

fn tokenize_markdown_line(text: &str, theme: &theme::Theme) -> Vec<ratatui::text::Span<'static>> {
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

fn wrap_spans(
    regions: Vec<(syntect::highlighting::Style, &str)>,
    max_width: usize,
    theme: &theme::Theme,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    let prefix = "│ ";
    let suffix = " │";
    let wrap_width = max_width.saturating_sub(prefix.chars().count() + suffix.chars().count());
    if wrap_width == 0 {
        return lines;
    }

    let mut current_line = vec![Span::styled(prefix, theme.muted())];
    let mut current_len = 0;

    for (style, text) in regions {
        let ratatui_style = syntect_style_to_ratatui(style);
        let mut text_remain = text.strip_suffix('\n').unwrap_or(text);
        if text_remain.is_empty() {
            continue;
        }

        while !text_remain.is_empty() {
            let space_left = wrap_width.saturating_sub(current_len);
            if space_left == 0 {
                current_line.push(Span::styled(suffix, theme.muted()));
                lines.push(Line::from(current_line));
                current_line = vec![Span::styled(prefix, theme.muted())];
                current_len = 0;
                continue;
            }

            let mut char_count = 0;
            let mut split_idx = text_remain.len();
            for (i, _) in text_remain.char_indices() {
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
        let padding = wrap_width.saturating_sub(current_len);
        if padding > 0 {
            current_line.push(Span::raw(" ".repeat(padding)));
        }
        current_line.push(Span::styled(suffix, theme.muted()));
        lines.push(Line::from(current_line));
    } else if lines.is_empty() {
        let padding = wrap_width;
        current_line.push(Span::raw(" ".repeat(padding)));
        current_line.push(Span::styled(suffix, theme.muted()));
        lines.push(Line::from(current_line));
    }

    lines
}

fn parse_markdown(doc: &str, theme: &theme::Theme, width: u16) -> ratatui::text::Text<'static> {
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
                let dashes = width_usize.saturating_sub(2);
                bottom.push_str(&"─".repeat(dashes));
                if width_usize > 1 {
                    bottom.push('╯');
                }
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
                let dashes = width_usize.saturating_sub(top.chars().count() + 1);
                top.push_str(&"─".repeat(dashes));
                if width_usize > top.chars().count() {
                    top.push('╮');
                }
                lines.push(Line::from(Span::styled(top, theme.muted())));

                let lang_clean = code_lang.to_lowercase();
                let ps = get_syntax_set();
                let theme = get_ephemera_theme();
                let syntax = ps
                    .find_syntax_by_token(&lang_clean)
                    .or_else(|| ps.find_syntax_by_extension(&lang_clean))
                    .or_else(|| ps.find_syntax_by_name(&code_lang))
                    .or_else(|| match lang_clean.as_str() {
                        "rs" | "rust" => ps.find_syntax_by_extension("rs"),
                        "py" | "python" | "python3" => ps.find_syntax_by_extension("py"),
                        "js" | "javascript" => ps.find_syntax_by_extension("js"),
                        "ts" | "typescript" => ps.find_syntax_by_extension("ts"),
                        "c" => ps.find_syntax_by_extension("c"),
                        "cpp" | "c++" | "cxx" => ps.find_syntax_by_extension("cpp"),
                        "go" | "golang" => ps.find_syntax_by_extension("go"),
                        "sh" | "bash" | "zsh" | "shell" => ps.find_syntax_by_extension("sh"),
                        "toml" => ps.find_syntax_by_extension("toml"),
                        "json" => ps.find_syntax_by_extension("json"),
                        "yaml" | "yml" => ps.find_syntax_by_extension("yaml"),
                        "sql" => ps.find_syntax_by_extension("sql"),
                        "html" => ps.find_syntax_by_extension("html"),
                        "css" => ps.find_syntax_by_extension("css"),
                        _ => None,
                    })
                    .unwrap_or_else(|| ps.find_syntax_plain_text());
                highlighter = Some(syntect::easy::HighlightLines::new(syntax, theme));
            }
            continue;
        }

        if in_code_block {
            let mut text = line.to_string();
            text.push('\n'); // Provide newline so syntect terminates line comments properly!
            
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

    let active_view = match app.active_buffer() {
        crate::buffers::BufferId::Chat => &app.chat.view,
        crate::buffers::BufferId::Search => &app.search.view,
        crate::buffers::BufferId::Chtsh => &app.chtsh.view,
    };

    let mut cache = active_view.cached_markdown.borrow_mut();
    if cache.is_none() || cache.as_ref().unwrap().0 != doc || cache.as_ref().unwrap().1 != inner_width {
        let parsed = parse_markdown(&doc, &theme, inner_width);
        *cache = Some((doc.clone(), inner_width, parsed));
    }

    let md = cache.as_ref().unwrap().2.clone();

    let mut total_lines = 0;
    for line in &md.lines {
        let w = line.width() as u16;
        if w == 0 {
            total_lines += 1;
        } else {
            total_lines += (w + inner_width - 1) / inner_width;
        }
    }
    
    let max_scroll = total_lines.saturating_sub(area.height) as usize;
    active_view.last_max_scroll.set(max_scroll);

    let scroll_y = if active_view.auto_scroll {
        max_scroll
    } else {
        active_view.scroll.min(max_scroll)
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme.muted());

    let paragraph = Paragraph::new(md)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y as u16, 0));

    frame.render_widget(paragraph, area);
}
