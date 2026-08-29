//! Layout: computes the vertical slices of the interface.
//!
//! Top to bottom:
//!   tabs (1) -> scrollable buffer area (fills) -> statusline (1)
//!   -> prompt box (1..=5, scrollable) -> bottom statusline (1)

use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::input::{Prompt, MAX_LINES};

/// Number of rows occupied by the prompt box for layout purposes.
pub fn prompt_height(prompt: &Prompt) -> u16 {
    let lines = prompt.wrapped_line_count();
    (lines.max(1).min(MAX_LINES)) as u16
}

/// The vertical chunks for `area`.
pub fn chunks(area: Rect, prompt: &Prompt) -> [Rect; 5] {
    // min terminal gate: need at least tabs(1)+status(1)+prompt(1)+bottom(1)+1
    let prompt_h = prompt_height(prompt);
    let rows = Layout::vertical([
        Constraint::Length(1),                // tabs
        Constraint::Min(1),                   // buffer area
        Constraint::Length(1),                // separator statusline
        Constraint::Length(prompt_h + 1),     // prompt box (+1 breathing row)
        Constraint::Length(1),                // bottom statusline
    ])
    .split(area);
    [
        rows[0], // 0 tabs
        rows[1], // 1 buffer
        rows[2], // 2 status
        rows[3], // 3 prompt
        rows[4], // 4 bottom
    ]
}
