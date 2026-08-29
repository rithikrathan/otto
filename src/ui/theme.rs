//! Semantic color theme.
//!
//! Colors are referenced by role, not value, and degrade gracefully to the
//! 16-ANSI foundation. `NO_COLOR` disables color entirely (monochrome mode),
//! keeping the interface usable in every terminal. Minimal palette by design —
//! this TUI runs in a narrow split beside an editor and must not clash with it.

use ratatui::style::{Color, Modifier, Style};

/// Central, app-wide style slots.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mono: bool,
}

/// Semantic slots (see tui-design §4). Chosen to be calm and editor-friendly.
impl Theme {
    /// Base / default text.
    pub fn base(self) -> Style {
        if self.mono {
            Style::default()
        } else {
            Style::default().fg(Color::Gray)
        }
    }

    /// Emphasized text (headers, focused items).
    pub fn emphasis(self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    /// Muted / secondary metadata (dims in color mode).
    pub fn muted(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }

    /// Interactive / focus accent.
    pub fn accent(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Cyan)
        }
    }

    /// Statusline background / bar fill.
    pub fn statusbar(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        }
    }

    /// Spinner / active accent.
    pub fn spinner(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        }
    }

    /// Markdown code block background/foreground
    pub fn markdown_code(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Cyan)
        }
    }

    /// Markdown horizontal rule
    pub fn markdown_hr(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }
}

/// Build the theme, honoring `NO_COLOR`.
pub fn current() -> Theme {
    let mono = std::env::var_os("NO_COLOR").is_some();
    Theme { mono }
}
