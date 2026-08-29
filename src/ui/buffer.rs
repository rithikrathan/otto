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
        crate::buffers::BufferId::Manage => {
            // Manage buffer renders its own list, so nothing here.
        }
    }
    doc
}

/// Draw the scrollable buffer region for the active buffer.
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let theme = theme::current();

    // The Manage buffer shows a model list instead of markdown history.
    if app.active_buffer() == crate::buffers::BufferId::Manage {
        return draw_manage(frame, app, area, &theme);
    }

    let doc = active_document(app);
    let md = tui_markdown::from_str(&doc);

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme.muted());

    let paragraph = Paragraph::new(md)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.active_scroll(), 0));

    frame.render_widget(paragraph, area);
}

/// Model-list view for the Manage buffer.
fn draw_manage(frame: &mut Frame, app: &App, area: Rect, theme: &theme::Theme) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{List, ListItem};

    let items: Vec<ListItem> = app
        .manage
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let mark = if Some(i) == Some(app.manage.model_index) {
                "●"
            } else {
                " "
            };
            ListItem::new(Line::from(vec![
                Span::styled(mark, theme.accent()),
                Span::raw(" "),
                Span::raw(m.clone()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(theme.muted())
                .title(format!(" models  ({} applied)", app.model_name))
                .title_style(theme.muted()),
        )
        .highlight_style(theme.emphasis())
        .highlight_symbol("");
    frame.render_widget(list, area);
}
