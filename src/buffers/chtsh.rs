use crate::chtsh::fuzzy_suggest;
use super::{Block, BufferState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChtshFocus {
    #[default]
    Scope,
    Query,
}

#[derive(Debug, Clone, Default)]
pub struct ChtshBuffer {
    pub view: BufferState,
    pub scope: String,
    pub query: String,
    pub focus: ChtshFocus,
    pub suggestions: Vec<String>,
    pub selected_suggestion: usize,
    pub root_list: Vec<String>,
    pub topic_list: Vec<String>,
    pub last_topic_scope: Option<String>,
    pub last_query: Option<String>,
}

impl ChtshBuffer {
    pub fn add_result(&mut self, query: &str, text: &str) {
        self.view.blocks.push(Block {
            kind: "cht.sh".to_string(),
            markdown: format!("### cht.sh/{}\n\n```sh\n{}\n```", query, text),
        });
        self.view.scroll = 9999;
    }

    pub fn clear(&mut self) {
        self.view.blocks.clear();
        self.view.scroll = 0;
        self.last_query = None;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            ChtshFocus::Scope => ChtshFocus::Query,
            ChtshFocus::Query => ChtshFocus::Scope,
        };
        self.selected_suggestion = 0;
        self.refresh_suggestions();
    }

    pub fn insert_char(&mut self, c: char) {
        match self.focus {
            ChtshFocus::Scope => self.scope.push(c),
            ChtshFocus::Query => self.query.push(c),
        }
        self.selected_suggestion = 0;
        self.refresh_suggestions();
    }

    pub fn backspace(&mut self) {
        match self.focus {
            ChtshFocus::Scope => {
                self.scope.pop();
            }
            ChtshFocus::Query => {
                self.query.pop();
            }
        }
        self.selected_suggestion = 0;
        self.refresh_suggestions();
    }

    pub fn next_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected_suggestion = (self.selected_suggestion + 1) % self.suggestions.len();
        }
    }

    pub fn prev_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            if self.selected_suggestion == 0 {
                self.selected_suggestion = self.suggestions.len() - 1;
            } else {
                self.selected_suggestion -= 1;
            }
        }
    }

    pub fn accept_suggestion(&mut self) {
        if let Some(sug) = self.suggestions.get(self.selected_suggestion).cloned() {
            match self.focus {
                ChtshFocus::Scope => {
                    self.scope = sug;
                    // Move to query upon completing scope
                    self.focus = ChtshFocus::Query;
                }
                ChtshFocus::Query => {
                    self.query = sug;
                }
            }
            self.selected_suggestion = 0;
            self.refresh_suggestions();
        }
    }

    pub fn refresh_suggestions(&mut self) {
        match self.focus {
            ChtshFocus::Scope => {
                self.suggestions = fuzzy_suggest(&self.root_list, &self.scope, 5);
            }
            ChtshFocus::Query => {
                if !self.topic_list.is_empty() {
                    self.suggestions = fuzzy_suggest(&self.topic_list, &self.query, 5);
                } else {
                    self.suggestions.clear();
                }
            }
        }
        if self.selected_suggestion >= self.suggestions.len() {
            self.selected_suggestion = 0;
        }
    }
}
