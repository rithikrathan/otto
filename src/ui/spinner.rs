use crate::app::JobKind;

pub const BRAILLE: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const BLOCKS: &[char] = &['▖', '▘', '▝', '▗'];
pub const LINE: &[char] = &['-', '\\', '|', '/'];

pub fn frame(tick: u64, kind: &JobKind) -> char {
    let frames = match kind {
        JobKind::Chat | JobKind::SearchPlan | JobKind::ChtshPlan => BRAILLE,
        JobKind::Models => BLOCKS,
        JobKind::SearchFetch | JobKind::ChtshFetch => LINE,
    };
    frames[(tick as usize) % frames.len()]
}
