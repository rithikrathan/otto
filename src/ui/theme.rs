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
            Style::default().fg(Color::Rgb(221, 204, 204)) // fg: #ddcccc
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
            Style::default().fg(Color::Rgb(105, 105, 105)) // comment: #696969
        }
    }

    /// Interactive / focus accent.
    pub fn accent(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Rgb(255, 37, 37)) // kw: #ff2525
        }
    }

    /// Statusline background / bar fill.
    pub fn statusbar(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            // pmenu_bg: #17171d, pmenu_fg: #fc6142
            Style::default().bg(Color::Rgb(23, 23, 29)).fg(Color::Rgb(252, 97, 66))
        }
    }

    /// Spinner / active accent.
    pub fn spinner(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(255, 170, 0)) // search_highlight: #ffaa00
        }
    }

    /// Active tab style
    pub fn tab_active(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            // bufferline_selection: #fd1b1b
            Style::default().bg(Color::Rgb(253, 27, 27)).fg(Color::Black).add_modifier(Modifier::BOLD)
        }
    }


    /// Markdown code block background/foreground
    pub fn markdown_code(self) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Rgb(228, 178, 171)) // string: #e4b2ab
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
