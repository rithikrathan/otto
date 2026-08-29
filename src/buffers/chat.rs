//! Chat buffer: normal conversation with the selected Ollama model.

use super::{Block, BufferState};

/// A chat message exchanged with the model (the "context").
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct ChatBuffer {
    /// Full message history sent to Ollama (the "context").
    pub history: Vec<ChatMessage>,
    /// What the UI renders.
    pub view: BufferState,
    /// Streaming assistant reply assembling in the current block.
    pub streaming: bool,
}

impl ChatBuffer {
    /// Append the user's prompt as a block + history entry and return its block.
    pub fn add_user(&mut self, text: &str) -> Block {
        let block = Block {
            kind: "you",
            markdown: text.to_string(),
        };
        self.view.blocks.push(block.clone());
        block
    }

    /// Start a new assistant block that will stream in.
    pub fn begin_assistant(&mut self) {
        self.view.blocks.push(Block {
            kind: "ollama",
            markdown: String::new(),
        });
        self.streaming = true;
    }

    /// Append a delta to the currently-streaming assistant block.
    pub fn push_assistant(&mut self, delta: &str) {
        if let Some(block) = self.view.blocks.last_mut() {
            if block.kind == "ollama" {
                block.markdown.push_str(delta);
            }
        }
        // Also record into history (as one growing assistant message).
        if let Some(last) = self.history.last_mut() {
            if last.role == "assistant" {
                last.content.push_str(delta);
                return;
            }
        }
        self.history.push(ChatMessage {
            role: "assistant".into(),
            content: delta.to_string(),
        });
    }

    /// Finalize the streaming assistant block.
    pub fn finish_assistant(&mut self) {
        self.streaming = false;
    }

    /// Clear the conversation (resets Ollama context).
    pub fn clear(&mut self) {
        self.history.clear();
        self.view.blocks.clear();
        self.view.scroll = 0;
        self.streaming = false;
    }
}
