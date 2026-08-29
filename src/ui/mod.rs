//! Rendering: layout, tabs, scrollable buffer area, statuslines, prompt box.

pub mod buffer;
pub mod layout;
pub mod modal;
pub mod prompt;
pub mod spinner;
pub mod theme;

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, JobKind};

/// Render the whole app once per frame.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = theme::current();
    if frame.area().width < 20 || frame.area().height < 10 {
        let msg = "Terminal too small. Please resize to at least 20x10.";
        let p = Paragraph::new(msg)
            .alignment(ratatui::layout::Alignment::Center)
            .style(theme.emphasis());
        let area = frame.area();
        let y = area.height.saturating_sub(1) / 2;
        let centered = ratatui::layout::Rect::new(area.x, y, area.width, 1);
        frame.render_widget(p, centered);
        return;
    }

    let rows = layout::chunks(frame.area(), &app.prompt);

    buffer::draw(frame, app, rows[0]);
    draw_statusline(frame, app, rows[1], &theme);
    prompt::draw(frame, &mut app.prompt, rows[2], &theme);
    draw_bottom(frame, app, rows[3], &theme);

    // Floating window overlays the main UI last so it is on top.
    modal::draw(frame, app);
}


/// The separator statusline between the buffer and the prompt box.
///
/// Shows active buffer · model · token stats (read / write / ctx %) · spinner.
fn draw_statusline(frame: &mut Frame, app: &App, area: Rect, theme: &theme::Theme) {
    let model = &app.model_name;
    let (r, w) = (app.tokens.prompt_tokens, app.tokens.eval_tokens);
    let ctx = app.tokens.context_percent();

    let mut left = vec![
        Span::styled(
            format!(" {} ", app.active_buffer().label().to_uppercase()),
            theme.statusbar(),
        ),
        Span::styled(" · ", theme.muted()),
    ];
    if let Some(job) = app.busy.first() {
        left.push(Span::styled(
            format!(" {} {}", spinner::frame(app.tick, job), busy_label(job)),
            theme.spinner(),
        ));
        left.push(Span::styled(" · ", theme.muted()));
    }

    let mut right = Vec::new();
    if area.width > 60 {
        right.push(Span::styled(model, theme.muted()));
        right.push(Span::styled(" · ", theme.muted()));
    }
    
    right.extend(vec![
        Span::raw(format!("in {}", r)),
        Span::styled(" · ", theme.muted()),
        Span::raw(format!("out {}", w)),
        Span::styled(" · ", theme.muted()),
        Span::styled(format!("ctx {}", ctx), theme.muted()),
    ]);

    // Right-align the stats block at the end of the statusline.
    let mut line = Line::from(left);
    line.spans.extend(right);
    frame.render_widget(Paragraph::new(line).style(theme.base()), area);
}

fn busy_label(job: &JobKind) -> &'static str {
    match job {
        JobKind::Chat => "thinking",
        JobKind::SearchPlan => "planning search",
        JobKind::SearchFetch => "searching",
        JobKind::ChtshPlan => "planning",
        JobKind::ChtshFetch => "fetching",
        JobKind::Stt => "listening",
        JobKind::Models => "loading models",
    }
}

/// The bottom statusline: mode / key hints · STT state.
fn draw_bottom(frame: &mut Frame, app: &App, area: Rect, theme: &theme::Theme) {
    let hint = if app.pending_abort {
        "Press ESC again to stop prompt"
    } else {
        match app.active_buffer() {
            crate::buffers::BufferId::Chat => "[Enter]send [Ctrl+K]clear",
            crate::buffers::BufferId::Search => "[Enter]search",
            crate::buffers::BufferId::Chtsh => "[Enter]query",
        }
    };
    let mic = if app.busy.contains(&JobKind::Stt) {
        "● REC"
    } else {
        "[Ctrl+M]mic"
    };
    let line = Line::from(vec![
        Span::styled(" ", theme.muted()),
        Span::styled(hint, theme.muted()),
        Span::styled(" │ ", theme.muted()),
        Span::styled(mic, theme.muted()),
        Span::styled("  [Tab]switch  [Ctrl+Q]quit", theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.muted()), area);
}
