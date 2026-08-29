//! cht.sh buffer: model-built URL -> fetch -> text rendered in the buffer.

use super::{Block, BufferState};

#[derive(Debug, Clone, Default)]
pub struct ChtshBuffer {
    pub view: BufferState,
    pub last_query: Option<String>,
}

impl ChtshBuffer {
    pub fn add_result(&mut self, query: &str, text: &str) {
        self.view.blocks.push(Block {
            kind: "cht.sh".to_string(),
            markdown: format!("**cht.sh/{}**\n\n```sh\n{}\n```", query, text),
        });
        self.view.scroll = 9999;
    }

    pub fn clear(&mut self) {
        self.view.blocks.clear();
        self.view.scroll = 0;
        self.last_query = None;
    }
}
