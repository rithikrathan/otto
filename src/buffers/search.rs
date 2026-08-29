//! Search buffer: model-refined query -> provider -> concise markdown result.

use super::{Block, BufferState};

#[derive(Debug, Clone, Default)]
pub struct SearchBuffer {
    /// /api/tags used by the model to plan the query.
    pub view: BufferState,
    /// Last query (for re-run / display).
    pub last_query: Option<String>,
}

impl SearchBuffer {
    pub fn add_result(&mut self, query: &str, markdown: &str) -> Block {
        // TODO(stub): model's last_query and append a block.
        Block {
            kind: "search",
            markdown: format!("**Search » {}**\n\n{}", query, markdown),
        }
    }

    pub fn clear(&mut self) {
        self.view.blocks.clear();
        self.view.scroll = 0;
        self.last_query = None;
    }
}