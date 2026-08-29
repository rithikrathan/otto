//! Switchable buffers. Each owns its own scrollable markdown history.
//!
//! Tab / Shift+Tab cycles through them; the active buffer fills the top area.

pub mod chat;
pub mod chtsh;
pub mod search;

/// Identity of a switchable buffer tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferId {
    Chat,
    Search,
    Chtsh,
}

impl BufferId {
    /// Short label shown in the tab bar.
    pub fn label(self) -> &'static str {
        match self {
            BufferId::Chat => "Chat",
            BufferId::Search => "Search",
            BufferId::Chtsh => "cht.sh",
        }
    }
}

/// The markdown history a buffer displays (shared shape).
///
/// TODO(stub): threading through actual rendering happens in the UI layer.
#[derive(Debug, Clone, Default)]
pub struct BufferState {
    /// Rendered markdown blocks in display order.
    pub blocks: Vec<Block>,
    /// Vertical scroll offset (rows from the top of the content).
    pub scroll: usize,
}

/// One message/result in a buffer.
#[derive(Debug, Clone)]
pub struct Block {
    /// Role/kind header, e.g. "you", "ollama", "search", "cht.sh".
    pub kind: String,
    /// Raw markdown content.
    pub markdown: String,
}
