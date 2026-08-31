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
        crate::buffers::BufferId::Ddg => {
            for b in &app.ddg.view.blocks {
                push_block(&mut doc, None, &b.markdown, "\n\n---\n\n");
            }
        }
        crate::buffers::BufferId::Chtsh => {
            for b in &app.chtsh.view.blocks {
                push_block(&mut doc, None, &b.markdown, "\n\n---\n\n");
            }
        }
        crate::buffers::BufferId::Wiki => {
            for b in &app.wiki.view.blocks {
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

/// A link discovered in a rendered markdown line. `url_index` is the index of
/// the `Line` in the rendered `Text` that this link belongs to.
#[derive(Debug, Clone)]
pub struct LinkHit {
    pub url: String,
}

/// Detect the URL for a markdown line. Handles `[label](url)` and bare URLs.
fn line_url(text: &str) -> Option<String> {
    // [label](url) -> url
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start..];
        let Some(close) = rest.find(']') else { break };
        let after = &rest[close + 1..];
        if let Some(openp) = after.strip_prefix('(') {
            let url = scan_url(openp)?.trim().to_string();
            if !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) {
                return Some(url);
            }
        }
        rest = &rest[close + 1..];
    }
    // bare http(s):// URL
    for part in text.split_whitespace() {
        let p = part.trim_end_matches(".,;)");
        if p.starts_with("http://") || p.starts_with("https://") {
            return Some(p.to_string());
        }
    }
    None
}

/// Read a URL starting just after the opening `(` of `[label](url)`, stopping
/// at its matching `)`. Handles parentheses inside the URL (e.g.
/// `https://en.wikipedia.org/wiki/Rust_(programming_language)`).
fn scan_url(openp: &str) -> Option<&str> {
    let mut depth = 1usize;
    for (i, c) in openp.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&openp[..i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn link_style(theme: &theme::Theme) -> ratatui::style::Style {
    theme
        .base()
        .fg(ratatui::style::Color::Rgb(0x58, 0xA6, 0xFF))
        .add_modifier(ratatui::style::Modifier::UNDERLINED)
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
        } else if c == '[' {
            // Possible [label](url) markdown link.
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            let mut label = String::new();
            let mut is_link = false;
            let mut url = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    if chars.peek() == Some(&'(') {
                        chars.next(); // consume '('
                        // Read the URL, balancing parentheses so that `)` inside
                        // the URL (e.g. `Rust_(programming_language)`) stays part
                        // of it and only the closing `)` ends the link.
                        let mut depth = 0u32;
                        for uc in chars.by_ref() {
                            match uc {
                                '(' => {
                                    depth += 1;
                                    url.push(uc);
                                }
                                ')' if depth == 0 => {
                                    is_link = true;
                                    break;
                                }
                                ')' => {
                                    depth -= 1;
                                    url.push(uc);
                                }
                                _ => url.push(uc),
                            }
                        }
                    }
                    break;
                }
                label.push(ic);
            }
            if is_link {
                spans.push(Span::styled(
                    label.clone(),
                    link_style(theme),
                ));
            } else {
                // Not a link: re-emit the literal text we consumed.
                let literal = format!("[{label}]");
                spans.push(Span::raw(literal));
            }
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

/// Returns the rendered `Text` plus a per-line URL map (same length as
/// `Text.lines`; `None` when that line is not a clickable link).
fn parse_markdown(
    doc: &str,
    theme: &theme::Theme,
    width: u16,
) -> (ratatui::text::Text<'static>, Vec<Option<String>>) {
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    let mut urls: Vec<Option<String>> = Vec::new();
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
                urls.push(None);
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
                urls.push(None);

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
                    let wrapped = wrap_spans(regions, width_usize, theme);
                    let n = wrapped.len();
                    lines.extend(wrapped);
                    urls.extend(std::iter::repeat(None).take(n));
                    continue;
                }
            }

            // Fallback if highlight fails
            let regions = vec![(syntect::highlighting::Style::default(), text.as_str())];
            let wrapped = wrap_spans(regions, width_usize, theme);
            let n = wrapped.len();
            lines.extend(wrapped);
            urls.extend(std::iter::repeat(None).take(n));
            continue;
        }

        if trimmed == "---" {
            let hr = "─".repeat(width_usize);
            lines.push(Line::from(Span::styled(hr, theme.muted())));
            urls.push(None);
            continue;
        }

        if trimmed.starts_with("# ") {
            let header = trimmed.strip_prefix("# ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 30, 0)))));
            urls.push(None);
            continue;
        } else if trimmed.starts_with("## ") {
            let header = trimmed.strip_prefix("## ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 66, 15)))));
            urls.push(None);
            continue;
        } else if trimmed.starts_with("### ") {
            let header = trimmed.strip_prefix("### ").unwrap_or("").to_string();
            lines.push(Line::from(Span::styled(header, theme.emphasis().fg(ratatui::style::Color::Rgb(255, 99, 71)))));
            urls.push(None);
            continue;
        }

        let url = line_url(line);
        lines.push(Line::from(tokenize_markdown_line(line, theme)));
        urls.push(url);
    }
    (ratatui::text::Text::from(lines), urls)
}

/// A clickable link region in buffer-local content row coordinates.
#[derive(Debug, Clone)]
pub struct LinkRect {
    pub row0: u16,
    pub row1: u16,
    pub url: String,
}

/// Draw the scrollable buffer region for the active buffer.
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = theme::current();

    let doc = active_document(app);
    let inner_width = area.width.saturating_sub(1); // Account for left border

    let active_view = match app.active_buffer() {
        crate::buffers::BufferId::Chat => &app.chat.view,
        crate::buffers::BufferId::Ddg => &app.ddg.view,
        crate::buffers::BufferId::Chtsh => &app.chtsh.view,
        crate::buffers::BufferId::Wiki => &app.wiki.view,
    };

    let mut cache = active_view.cached_markdown.borrow_mut();
    if cache.is_none() || cache.as_ref().unwrap().0 != doc || cache.as_ref().unwrap().1 != inner_width {
        let (parsed, urls) = parse_markdown(&doc, &theme, inner_width);
        *cache = Some((doc.clone(), inner_width, parsed, urls));
    }
    let cached = cache.as_ref().unwrap();
    let md = cached.2.clone();
    let line_urls = cached.3.clone();

    let mut total_lines = 0;
    let mut link_layout: Vec<LinkRect> = Vec::new();
    for (i, line) in md.lines.iter().enumerate() {
        let w = line.width() as u16;
        let rows = if w == 0 {
            1
        } else {
            (w + inner_width - 1) / inner_width
        };
        if let Some(url) = &line_urls[i] {
            link_layout.push(LinkRect {
                row0: total_lines as u16,
                row1: (total_lines + rows) as u16 - 1,
                url: url.clone(),
            });
        }
        total_lines += rows;
    }

    let max_scroll = total_lines.saturating_sub(area.height) as usize;
    active_view.last_max_scroll.set(max_scroll);

    let scroll_y = if active_view.auto_scroll {
        max_scroll
    } else {
        active_view.scroll.min(max_scroll)
    } as u16;

    // Expose buffer geometry + link map for mouse click lookups.
    app.buffer_area = (area.x, area.y, area.width, area.height);
    app.link_scroll_y = scroll_y;
    app.link_inner_width = inner_width;
    app.link_layout = link_layout;

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme.muted());

    let paragraph = Paragraph::new(md)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y, 0));

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_url_extracts_markdown_link() {
        assert_eq!(
            line_url("- [Nikola Tesla](https://en.wikipedia.org/wiki/Nikola_Tesla)"),
            Some("https://en.wikipedia.org/wiki/Nikola_Tesla".to_string())
        );
    }

    #[test]
    fn line_url_extracts_bare_url() {
        assert_eq!(
            line_url("see https://example.com/page"),
            Some("https://example.com/page".to_string())
        );
    }

    #[test]
    fn line_url_none_without_link() {
        assert_eq!(line_url("just some plain text here"), None);
        assert_eq!(line_url("- docker is a tool"), None);
    }

    #[test]
    fn line_url_keeps_parentheses_inside_url() {
        // Wikipedia disambiguation URLs contain `(...)` inside the URL; the
        // parser must keep it and only stop at the final `)`.
        let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        assert_eq!(
            line_url(&format!("- [Rust (programming language)]({url})")),
            Some(url.to_string())
        );
    }

    #[test]
    fn tokenizer_styles_markdown_links_underlined_blue() {
        let t = theme::current();
        let spans = tokenize_markdown_line("- [label](https://x.io)", &t);
        // A link span exists with a blue, underlined style.
        let link = spans.iter().find(|s| s.content == "label");
        assert!(link.is_some(), "expected a styled label span");
        let link = link.unwrap();
        assert!(link.style.fg.is_some());
        assert!(
            link.style
                .add_modifier
                .contains(ratatui::style::Modifier::UNDERLINED)
        );
    }

    #[test]
    fn tokenizer_keeps_parentheses_inside_url_and_label() {
        let t = theme::current();
        let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        let line = format!("- [Rust (programming language)]({url})");
        let spans = tokenize_markdown_line(&line, &t);
        // The label keeps its inner parentheses and is styled as a link.
        let link = spans.iter().find(|s| s.content == "Rust (programming language)");
        assert!(link.is_some(), "expected parenthesized label span");
        let link = link.unwrap();
        assert!(link.style.add_modifier.contains(ratatui::style::Modifier::UNDERLINED));
        // line_url recovers the full URL including the parenthesized suffix.
        assert_eq!(line_url(&line), Some(url.to_string()));
    }

    #[test]
    fn wiki_markdown_title_is_a_link_not_raw_markup() {
        use crate::wiki;
        let md = wiki::render_markdown(
            "navier strokes",
            &[wiki::WikiHit {
                title: "Navier–Stokes equations".into(),
                url: "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations".into(),
            }],
            Some(&wiki::WikiSummary {
                title: "Navier–Stokes equations".into(),
                extract: "The Navier–Stokes equations describe the motion of viscous fluids."
                    .into(),
                url: "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations".into(),
            }),
        );
        // The title is a clean clickable markdown link, not leaked `[[ ]]`
        // wiki markup or a bold-only (non-clickable) heading.
        assert!(md.contains("[Navier–Stokes equations](https://"));
        assert!(!md.contains("[["));

        let t = theme::current();
        let (text, urls) = parse_markdown(&md, &t, 80);
        let urls: Vec<_> = urls.iter().flatten().cloned().collect();
        // Every extracted URL must be complete (nothing truncated by `)`),
        // including the top title link.
        assert!(urls.iter().any(|u| u == "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations"));
        for u in &urls {
            assert!(u.ends_with("_equations"), "got truncated url: {u}");
        }
    }

    #[test]
    fn click_mapping_hits_wiki_links() {
        use crate::app::App;
        use crate::wiki;
        use ratatui::backend::TestBackend;

        // Populate the wiki buffer with a realistic result.
        let mut app = App::new();
        app.active = app
            .tabs
            .iter()
            .position(|t| *t == crate::buffers::BufferId::Wiki)
            .unwrap();
        let md = wiki::render_markdown(
            "navier strokes",
            &[
                wiki::WikiHit {
                    title: "Navier–Stokes equations".into(),
                    url: "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations".into(),
                },
                wiki::WikiHit {
                    title: "Rust (programming language)".into(),
                    url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".into(),
                },
            ],
            Some(&wiki::WikiSummary {
                title: "Navier–Stokes equations".into(),
                extract: "The Navier–Stokes equations describe the motion of viscous fluids."
                    .into(),
                url: "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations".into(),
            }),
        );
        app.wiki.set_result("navier strokes", md);

        // Render with a test backend (draw populates link geometry on app).
        let mut terminal = ratatui::Terminal::new(TestBackend::new(100, 40)).unwrap();
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect::new(0, 0, 100, 40);
                super::draw(f, &mut app, area);
            })
            .unwrap();

        let (ax, ay, aw, _ah) = app.buffer_area;
        assert!(!app.link_layout.is_empty(), "expected link_layout to be populated");

        // Simulate the exact open_clicked_link mapping for every link target.
        let mut found: Vec<String> = Vec::new();
        for lr in &app.link_layout {
            // Click the first row of the link, one column in from the border.
            let content_row = lr.row0;
            let local_row = content_row.saturating_sub(app.link_scroll_y);
            let row = ay + local_row;
            let col = ax + 1;
            // Guard: only if on-screen.
            if row >= ay && row < ay + _ah && col >= ax && col < ax + aw {
                let local_col = col - ax;
                if local_col >= 1 {
                    let cr = local_row + app.link_scroll_y;
                    if let Some(lr2) = app
                        .link_layout
                        .iter()
                        .find(|lr| cr >= lr.row0 && cr <= lr.row1)
                    {
                        found.push(lr2.url.clone());
                    }
                }
            }
        }
        // The source link and both More-articles links should all be found.
        assert!(
            found.iter().any(|u| u.contains("Navier") && u.contains("_equations")),
            "source/wiki link not clickable, found: {found:?}"
        );
        assert!(
            found.iter().any(|u| u.contains("Rust_(programming_language)")),
            "paren-disambiguation link not clickable, found: {found:?}"
        );
    }
}