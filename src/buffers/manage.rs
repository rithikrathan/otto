//! Manage buffer: model selection, chat management, export, settings.

use super::BufferState;

/// Sub-pane shown inside the Manage buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagePane {
    Model,
    Chats,
    Export,
    Settings,
}

#[derive(Debug, Clone)]
pub struct ManageBuffer {
    pub pane: ManagePane,
    /// Models from `/api/tags`.
    pub models: Vec<String>,
    /// Selected model index.
    pub model_index: usize,
    /// TODO(stub): persisted conversation list for chat management.
    pub view: BufferState,
}

impl Default for ManageBuffer {
    fn default() -> Self {
        Self {
            pane: ManagePane::Model,
            models: Vec::new(),
            model_index: 0,
            view: BufferState::default(),
        }
    }
}

impl ManageBuffer {
    pub fn selected_model(&self) -> Option<&str> {
        self.models.get(self.model_index).map(String::as_str)
    }

    pub fn clear(&mut self) {
        self.view.blocks.clear();
        self.view.scroll = 0;
    }
}