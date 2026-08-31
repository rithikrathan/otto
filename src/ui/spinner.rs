use crate::app::JobKind;

pub const BRAILLE: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const BLOCKS: &[char] = &['▖', '▘', '▝', '▗'];
pub const LINE: &[char] = &['-', '\\', '|', '/'];

pub fn frame(tick: u64, kind: &JobKind) -> char {
    let frames = match kind {
        JobKind::Chat | JobKind::ChtshPlan => BRAILLE,
        JobKind::Models => BLOCKS,
        JobKind::ChtshFetch | JobKind::DdgFetch | JobKind::WikiFetch => LINE,
    };
    frames[(tick as usize) % frames.len()]
}
