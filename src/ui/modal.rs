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
    let Some(modal) = app.modal else { return };
    let theme = theme::current();

    let area = frame.area();
    let width = (area.width * 2 / 3).min(64).max(24);
    let height = modal_height(app);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let win = Rect::new(x, y, width, height);

    frame.render_widget(Clear, win);

    let (title, rows) = match modal {
        Modal::ModelPicker => (" model picker ", app.modal_rows()),
        Modal::Settings => (" settings ", app.modal_rows()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(theme.emphasis())
        .border_style(theme.accent());

    // Rows: label (and value for settings / check for active model).
    let items: Vec<ListItem> = rows
        .iter()
        .map(|(label, value, selected)| {
            let body = if value.is_empty() {
                label.clone()
            } else {
                format!("{label} = {value}")
            };
            let line = if *selected {
                Line::from(vec![
                    Span::styled("● ", theme.accent()),
                    Span::styled(body, theme.emphasis()),
                ])
            } else {
                Line::from(body)
            };
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.emphasis())
        .highlight_symbol("");

    let inner = list_inner(win);
    frame.render_widget(list, inner);

    // Footer with key hints.
    let hint = match modal {
        Modal::ModelPicker => " ↑/↓ move   Enter apply   Esc close ",
        Modal::Settings => " ↑/↓ move   Enter toggle/select   Esc close ",
    };
    let fh = Rect::new(
        win.x,
        win.y + win.height.saturating_sub(1).min(area.height - 1),
        win.width,
        1,
    );
    frame.render_widget(Paragraph::new(hint).style(theme.muted()), fh);
}

/// Height of the modal window based on its content.
fn modal_height(app: &App) -> u16 {
    let rows = match app.modal {
        Some(Modal::ModelPicker) => app.manage.models.len(),
        Some(Modal::Settings) => crate::app::settings_rows(),
        None => 0,
    };
    (rows as u16 + 3).clamp(6, 24)
}

/// The area inside a bordered window (excludes the border).
fn list_inner(win: Rect) -> Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(win)[1]
}
