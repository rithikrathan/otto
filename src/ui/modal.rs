//! Floating window rendering: a centered overlay for the model picker,
//! settings, and help. Draws over the underlying UI and clears the area behind it.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, Modal};

use super::theme;

/// Draw the active floating window (if any), centered over the main UI.
pub fn draw(frame: &mut Frame, app: &App) {
    let Some(ref modal) = app.modal else { return };
    let theme = theme::current();

    let area = frame.area();
    let width = match modal {
        Modal::ModelPicker => (area.width * 4 / 5).min(76).max(36),
        Modal::Help => (area.width * 4 / 5).min(72).max(36),
        _ => (area.width * 2 / 3).min(64).max(28),
    };
    let height = modal_height(app, area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let win = Rect::new(x, y, width, height);

    frame.render_widget(Clear, win);

    if matches!(app.modal, Some(Modal::ModelPicker)) {
        draw_model_picker(frame, app, win, theme);
    } else {
        draw_generic_modal(frame, app, win, theme);
    }
}

/// Draw the categorized Model & Provider Picker.
fn draw_model_picker(frame: &mut Frame, app: &App, win: Rect, theme: theme::Theme) {
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Select Model & Provider ")
        .title_style(theme.emphasis().fg(Color::Cyan))
        .border_style(theme.accent());

    let inner_area = outer_block.inner(win);
    frame.render_widget(outer_block, win);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search bar
            Constraint::Length(3), // Provider tabs
            Constraint::Min(4),    // Model list
            Constraint::Length(1), // Footer shortcuts
        ])
        .split(inner_area);

    // 1. Search Bar
    let search_focused = app.modal_search_focused;
    let search_border_style = if search_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        theme.muted()
    };
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if search_focused { " Search Models (active) " } else { " Search Models (press '.' to focus) " })
        .title_style(if search_focused { Style::default().fg(Color::Yellow) } else { theme.muted() })
        .border_style(search_border_style);

    let search_text = if app.modal_search.is_empty() {
        if search_focused {
            Line::from(vec![Span::styled("_", Style::default().fg(Color::Yellow))])
        } else {
            Line::from(vec![Span::styled("Type to filter models across all providers...", theme.muted())])
        }
    } else {
        if search_focused {
            Line::from(vec![
                Span::raw(&app.modal_search),
                Span::styled("_", Style::default().fg(Color::Yellow)),
            ])
        } else {
            Line::from(vec![Span::raw(&app.modal_search)])
        }
    };
    frame.render_widget(Paragraph::new(search_text).block(search_block), chunks[0]);

    // 2. Provider Pills Bar
    let provider_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Provider Categories ")
        .title_style(theme.emphasis())
        .border_style(theme.muted());

    let mut provider_spans = Vec::new();
    provider_spans.push(Span::styled(" < Tab/Left ", theme.muted()));

    for (i, p) in app.provider_list.iter().enumerate() {
        let is_selected = i == app.provider_index;
        let count = app.provider_models.get(p).map(|v| v.len()).unwrap_or(0);

        if is_selected {
            let label = format!(" [ {} ({count}) ] ", p.to_uppercase());
            provider_spans.push(Span::styled(
                label,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            let label = format!(" {} ", p);
            provider_spans.push(Span::styled(label, theme.muted()));
        }
        provider_spans.push(Span::raw(" "));
    }

    provider_spans.push(Span::styled("Right/Shift+Tab > ", theme.muted()));
    frame.render_widget(Paragraph::new(Line::from(provider_spans)).block(provider_block), chunks[1]);

    // 3. Models List
    let items_with_prov = app.filtered_models_with_provider();
    let list_title = if app.modal_search.is_empty() {
        format!(" {} Models ({}) ", app.active_provider_tab().to_uppercase(), items_with_prov.len())
    } else {
        format!(" Matching Models ({}) ", items_with_prov.len())
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(list_title)
        .title_style(theme.emphasis().fg(Color::Green))
        .border_style(theme.muted());

    let inner_list_area = chunks[2];

    let items: Vec<ListItem> = items_with_prov
        .iter()
        .enumerate()
        .map(|(i, (prov, model_name))| {
            let is_cursor = i == app.modal_index;
            let is_active = app.provider_name == *prov && app.model_name == *model_name;

            let mut line_spans = Vec::new();

            // Cursor prefix
            if is_cursor {
                line_spans.push(Span::styled(" > ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            } else {
                line_spans.push(Span::raw("   "));
            }

            // Provider badge if in global search
            if !app.modal_search.is_empty() {
                let p_badge = format!("[{prov}] ");
                line_spans.push(Span::styled(p_badge, Style::default().fg(Color::Magenta)));
            }

            // Model name
            let model_style = if is_cursor {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                theme.base()
            };
            line_spans.push(Span::styled(model_name.clone(), model_style));

            // Active indicator badge
            if is_active {
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(
                    " [ACTIVE] ",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ));
            }

            ListItem::new(Line::from(line_spans))
        })
        .collect();

    let list_widget = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::Rgb(35, 35, 45)));

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.modal_index));
    frame.render_stateful_widget(list_widget, inner_list_area, &mut state);

    // 4. Footer Keybindings
    let footer_spans = vec![
        Span::styled(" [.] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("Search  ", theme.muted()),
        Span::styled(" [Tab/Left/Right] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("Provider  ", theme.muted()),
        Span::styled(" [Up/Down] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("Select  ", theme.muted()),
        Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled("Switch  ", theme.muted()),
        Span::styled(" [Esc] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled("Close", theme.muted()),
    ];
    frame.render_widget(Paragraph::new(Line::from(footer_spans)), chunks[3]);
}

/// Generic modal renderer for Settings, Help, etc.
fn draw_generic_modal(frame: &mut Frame, app: &App, win: Rect, theme: theme::Theme) {
    let modal = app.modal.as_ref().unwrap();

    let (title, rows) = match modal {
        Modal::Settings => (" Settings ", app.modal_rows()),
        Modal::Help => (" Keyboard Shortcuts & Help ", app.modal_rows()),
        _ => (" Modal ", app.modal_rows()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_style(theme.emphasis().fg(Color::Cyan))
        .border_style(theme.accent());

    let inner = block.inner(win);
    frame.render_widget(block, win);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, value, _))| {
            let is_sel = i == app.modal_index;
            let mut spans = Vec::new();
            if is_sel {
                spans.push(Span::styled(" > ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            } else {
                spans.push(Span::raw("   "));
            }

            spans.push(Span::styled(
                label.clone(),
                if is_sel { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { theme.base() },
            ));

            if !value.is_empty() {
                spans.push(Span::styled(format!("  ->  {value}"), theme.muted()));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).highlight_style(Style::default().bg(Color::Rgb(35, 35, 45)));
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.modal_index));
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let hint = match modal {
        Modal::Settings => " [Up/Down] Move   [Enter] Toggle/Edit   [Esc] Close ",
        Modal::Help => " [Up/Down/PageUp/PageDown] Scroll   [Esc / Enter] Close ",
        _ => " [Up/Down] Move   [Enter] Select   [Esc] Close ",
    };
    frame.render_widget(Paragraph::new(hint).style(theme.muted()), chunks[1]);
}

/// Height of the modal window based on its content.
fn modal_height(app: &App, max_screen_height: u16) -> u16 {
    match &app.modal {
        Some(Modal::ModelPicker) => (max_screen_height * 4 / 5).clamp(14, 24),
        Some(Modal::Help) => (max_screen_height * 4 / 5).clamp(16, 26),
        Some(Modal::Settings) => 12,
        None => 8,
    }
}


