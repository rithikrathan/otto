use super::{Block, BufferState};

#[derive(Debug, Clone, Default)]
pub struct WikiBuffer {
    pub view: BufferState,
    pub last_query: Option<String>,
}

impl WikiBuffer {
    pub fn set_result(&mut self, query: &str, markdown: String) {
        self.last_query = Some(query.to_string());
        self.view.blocks = vec![Block {
            kind: "wiki".to_string(),
            markdown,
        }];
        self.view.scroll = 0;
    }

    pub fn push_error(&mut self, msg: &str) {
        self.view.blocks.push(Block {
            kind: "error".to_string(),
            markdown: format!("*{msg}*"),
        });
        self.view.scroll = 9999;
    }

    pub fn clear(&mut self) {
        self.view.blocks.clear();
        self.view.scroll = 0;
        self.last_query = None;
    }
}
