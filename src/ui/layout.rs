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
pub fn chunks(area: Rect, prompt: &Prompt) -> [Rect; 4] {
    // min terminal gate: need at least status(1)+prompt(1)+bottom(1)+1
    let prompt_h = prompt_height(prompt);
    let rows = Layout::vertical([
        Constraint::Min(1),               // buffer area
        Constraint::Length(1),            // separator statusline
        Constraint::Length(prompt_h + 2), // prompt box (borders + content)
        Constraint::Length(1),            // bottom statusline
    ])
    .split(area);
    [
        rows[0], // 0 buffer
        rows[1], // 1 status
        rows[2], // 2 prompt
        rows[3], // 3 bottom
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::Prompt;

    fn area() -> Rect {
        Rect::new(0, 0, 100, 30)
    }

    /// An empty prompt box must be 3 rows tall so the middle row holds the
    /// text between the top and bottom borders (border, content, border).
    #[test]
    fn empty_prompt_box_is_three_rows() {
        let p = Prompt::new();
        assert_eq!(chunks(area(), &p)[3].height, 3);
    }

    #[test]
    fn single_line_prompt_stays_three_rows() {
        let mut p = Prompt::new();
        p.text = "hello".to_string();
        assert_eq!(chunks(area(), &p)[3].height, 3);
    }

    #[test]
    fn multi_line_prompt_grows_past_three_rows() {
        let mut p = Prompt::new();
        p.text = "one two three four five six seven eight nine ten eleven twelve".to_string();
        p.width = 10; // force wrapping into multiple rows
        let h = chunks(area(), &p)[3].height;
        assert!(h > 3, "expected prompt to grow, got {h}");
        assert!(h <= MAX_LINES as u16 + 2);
    }
}
