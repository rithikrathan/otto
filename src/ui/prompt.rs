use crate::app::App;
use crate::buffers::BufferId;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::Theme;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    if app.active_buffer() == BufferId::Chtsh {
        draw_chtsh_input(frame, app, area, theme);
        return;
    }
    let prompt = &mut app.prompt;

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
                cell.set_fg(theme.muted().fg.unwrap_or(ratatui::style::Color::DarkGray));
            } else {
                cell.set_style(cell.style().add_modifier(ratatui::style::Modifier::REVERSED));
            }
        }
    }
}

/// Compact input component for cht.sh.
fn draw_chtsh_input(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    use crate::buffers::chtsh::ChtshFocus;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Line 1: Scope and Query inputs
            Constraint::Length(1), // Line 2: Fuzzy suggestions
        ])
        .split(area);

    // Line 1 (Inputs): [ Scope ] │ [ Query ] side-by-side
    let scope_active = app.chtsh.focus == ChtshFocus::Scope;
    let query_active = app.chtsh.focus == ChtshFocus::Query;

    let input_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(2),
            Constraint::Percentage(63),
        ])
        .split(chunks[0]);

    // Scope block with cursor insertion
    let scope_text = app.chtsh.scope.value();
    let scope_cursor_idx = app.chtsh.scope.cursor.min(scope_text.len());
    let mut scope_display = Vec::new();
    scope_display.push(Span::styled("Scope: ", if scope_active { theme.accent() } else { theme.muted() }));
    if scope_active {
        scope_display.push(Span::raw(&scope_text[..scope_cursor_idx]));
        scope_display.push(Span::styled("█", theme.accent()));
        scope_display.push(Span::raw(&scope_text[scope_cursor_idx..]));
    } else {
        scope_display.push(Span::styled(scope_text, theme.base()));
    }
    frame.render_widget(Paragraph::new(Line::from(scope_display)), input_chunks[0]);

    // Divider
    frame.render_widget(Paragraph::new(Span::styled("│", theme.muted())), input_chunks[1]);

    // Query block with cursor insertion
    let query_text = app.chtsh.query.value();
    let query_cursor_idx = app.chtsh.query.cursor.min(query_text.len());
    let mut query_display = Vec::new();
    query_display.push(Span::styled("Query: ", if query_active { theme.accent() } else { theme.muted() }));
    if query_active {
        query_display.push(Span::raw(&query_text[..query_cursor_idx]));
        query_display.push(Span::styled("█", theme.accent()));
        query_display.push(Span::raw(&query_text[query_cursor_idx..]));
    } else {
        query_display.push(Span::styled(query_text, theme.base()));
    }
    frame.render_widget(Paragraph::new(Line::from(query_display)), input_chunks[2]);

    // Line 2 (Fuzzy Suggestions): Horizontal top matches
    let mut sug_spans = Vec::new();
    sug_spans.push(Span::styled("Suggestions: ", theme.muted()));
    if app.chtsh.suggestions.is_empty() {
        sug_spans.push(Span::styled("(none)", theme.muted()));
    } else {
        for (i, sug) in app.chtsh.suggestions.iter().enumerate() {
            if i > 0 {
                sug_spans.push(Span::styled(" │ ", theme.muted()));
            }
            if i == app.chtsh.selected_suggestion {
                sug_spans.push(Span::styled(
                    format!("▶ {}", sug),
                    theme.accent().add_modifier(ratatui::style::Modifier::BOLD),
                ));
            } else {
                sug_spans.push(Span::styled(sug, theme.muted()));
            }
        }
    }
    frame.render_widget(Paragraph::new(Line::from(sug_spans)), chunks[1]);
}
