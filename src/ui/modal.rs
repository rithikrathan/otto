//! Floating window rendering: a centered overlay for the model picker and
//! settings. Draws over the underlying UI and clears the area behind it.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, Modal};

use super::theme;

/// Draw the active floating window (if any), centered over the main UI.
pub fn draw(frame: &mut Frame, app: &App) {
    let Some(ref modal) = app.modal else { return };
    let theme = theme::current();

    let area = frame.area();
    let width = (area.width * 2 / 3).min(64).max(24);
    let height = modal_height(app);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let win = Rect::new(x, y, width, height);

    frame.render_widget(Clear, win);

    let (search_rect, list_rect) = if matches!(app.modal, Some(Modal::ModelPicker)) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(win);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, win)
    };

    let (title, rows) = match &modal {
        Modal::ModelPicker => (" model picker ", app.modal_rows()),
        Modal::Settings => (" settings ", app.modal_rows()),
        Modal::SearchQueryPicker(_) => (" select query ", app.modal_rows()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(theme.emphasis())
        .border_style(theme.accent());

    // Rows: label (and value for settings / check for active model).
    let items: Vec<ListItem> = rows
        .iter()
        .map(|(label, value, _)| {
            let body = if value.is_empty() {
                label.clone()
            } else {
                format!("{label} = {value}")
            };
            let max_w = width.saturating_sub(6) as usize;
            let mut lines = Vec::new();
            let mut current_line = String::new();
            for word in body.split_whitespace() {
                if current_line.is_empty() {
                    current_line.push_str(word);
                } else if current_line.len() + 1 + word.len() <= max_w {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    lines.push(current_line);
                    current_line = word.to_string();
                }
            }
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            if lines.is_empty() {
                lines.push(String::new());
            }

            let text_lines: Vec<Line> = lines
                .into_iter()
                .enumerate()
                .map(|(i, l)| {
                    if i == 0 {
                        Line::from(vec![Span::raw("  "), Span::raw(l)])
                    } else {
                        Line::from(vec![Span::raw("    "), Span::raw(l)])
                    }
                })
                .collect();

            ListItem::new(ratatui::text::Text::from(text_lines))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.accent())
        .highlight_symbol("▶");

    let inner = list_inner(list_rect);
    
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.modal_index));
    frame.render_stateful_widget(list, inner, &mut state);

    if let Some(rect) = search_rect {
        let style = if app.modal_search_focused {
            theme.accent()
        } else {
            theme.muted()
        };
        let sb = Block::default()
            .borders(Borders::ALL)
            .title(" search (press '.' to focus) ")
            .title_style(style)
            .border_style(style);
        
        let text = if app.modal_search.is_empty() && !app.modal_search_focused {
            Span::styled("...", theme.muted())
        } else {
            Span::raw(&app.modal_search)
        };
        
        frame.render_widget(Paragraph::new(text).block(sb), rect);
    }

    // Footer with key hints.
    let hint = match &modal {
        Modal::ModelPicker => " . focus search   ↑/↓ move   Enter select   Esc close ",
        Modal::Settings => " ↑/↓ move   Enter toggle/select   Esc close ",
        Modal::SearchQueryPicker(_) => " ↑/↓ move   Enter execute search   Esc close ",
    };
    let fh = Rect::new(
        list_rect.x,
        list_rect.y + list_rect.height.saturating_sub(1).min(area.height - 1),
        list_rect.width,
        1,
    );
    frame.render_widget(Paragraph::new(hint).style(theme.muted()), fh);
}

/// Height of the modal window based on its content.
fn modal_height(app: &App) -> u16 {
    let mut extra = 0;
    let rows = match &app.modal {
        Some(Modal::ModelPicker) => app.filtered_models().len(),
        Some(Modal::Settings) => crate::app::settings_rows(),
        Some(Modal::SearchQueryPicker(opts)) => opts.len(),
        None => 0,
    };
    (rows as u16 + 3 + extra).clamp(6, 24)
}

/// The area inside a bordered window (excludes the border).
fn list_inner(win: Rect) -> Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(win)[1]
}
