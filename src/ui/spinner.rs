//! Spinner animation for in-flight jobs (depends on the frame ticker).
//!
//! TODO(stub): a simple frame-count based spinner:

/// Braille / line spinner frames.
pub const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The frame to show for a given tick index.
pub fn frame(tick: u64) -> char {
    FRAMES[(tick as usize) % FRAMES.len()]
}