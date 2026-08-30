use crate::chtsh::fuzzy_suggest;
use super::{Block, BufferState};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputField {
    pub text: String,
    pub cursor: usize,
}

impl InputField {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.text.remove(prev);
        self.cursor = prev;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let len = self.text[self.cursor..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.text.replace_range(self.cursor..self.cursor + len, "");
    }

    pub fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[..self.cursor];
        let mut iter = before.char_indices().rev().peekable();
        while let Some(&(_, c)) = iter.peek() {
            if c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        while let Some(&(_, c)) = iter.peek() {
            if !c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        let new_cursor = iter.peek().map(|&(i, c)| i + c.len_utf8()).unwrap_or(0);
        self.text.replace_range(new_cursor..self.cursor, "");
        self.cursor = new_cursor;
    }

    pub fn delete_word_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let after = &self.text[self.cursor..];
        let mut iter = after.char_indices().peekable();
        while let Some(&(_, c)) = iter.peek() {
            if !c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        while let Some(&(_, c)) = iter.peek() {
            if c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        let end = self.cursor + iter.peek().map(|&(i, _)| i).unwrap_or(after.len());
        self.text.replace_range(self.cursor..end, "");
    }

    pub fn move_left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.cursor = prev;
        true
    }

    pub fn move_right(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = self.text[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
            .unwrap_or(self.text.len());
        self.cursor = next;
        true
    }

    pub fn move_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[..self.cursor];
        let mut iter = before.char_indices().rev().peekable();
        while let Some(&(_, c)) = iter.peek() {
            if c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        while let Some(&(_, c)) = iter.peek() {
            if !c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        let new_cursor = iter.peek().map(|&(i, c)| i + c.len_utf8()).unwrap_or(0);
        self.cursor = new_cursor;
    }

    pub fn move_word_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let after = &self.text[self.cursor..];
        let mut iter = after.char_indices().peekable();
        while let Some(&(_, c)) = iter.peek() {
            if !c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        while let Some(&(_, c)) = iter.peek() {
            if c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        let next = self.cursor + iter.peek().map(|&(i, _)| i).unwrap_or(after.len());
        self.cursor = next;
    }

    pub fn move_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChtshFocus {
    #[default]
    Scope,
    Query,
}

#[derive(Debug, Clone, Default)]
pub struct ChtshBuffer {
    pub view: BufferState,
    pub scope: InputField,
    pub query: InputField,
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
        let first_token = query.split('/').next().unwrap_or("").trim().to_lowercase();
        let code_tag = match first_token.as_str() {
            "rs" | "rust" => "rust",
            "py" | "python" | "python3" => "python",
            "js" | "javascript" | "node" => "javascript",
            "ts" | "typescript" => "typescript",
            "c" => "c",
            "cpp" | "c++" => "cpp",
            "go" | "golang" => "go",
            "lua" => "lua",
            "rb" | "ruby" => "ruby",
            "sh" | "bash" | "zsh" => "sh",
            "html" => "html",
            "css" => "css",
            "json" => "json",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "sql" => "sql",
            other if !other.is_empty() => other,
            _ => "sh",
        };

        let formatted = if text.contains("```") {
            format!("### cht.sh/{}\n\n{}", query, text)
        } else {
            format!("### cht.sh/{}\n\n```{}\n{}\n```", query, code_tag, text)
        };

        self.view.blocks.push(Block {
            kind: "cht.sh".to_string(),
            markdown: formatted,
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
        self.suggestions.clear();
    }

    pub fn set_focus(&mut self, focus: ChtshFocus) {
        if self.focus != focus {
            self.focus = focus;
            self.selected_suggestion = 0;
            self.suggestions.clear();
        }
    }

    pub fn insert_char(&mut self, c: char) {
        match self.focus {
            ChtshFocus::Scope => self.scope.insert_char(c),
            ChtshFocus::Query => self.query.insert_char(c),
        }
        self.selected_suggestion = 0;
        self.refresh_suggestions();
    }

    pub fn delete_backward(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.delete_backward(),
            ChtshFocus::Query => self.query.delete_backward(),
        }
        self.selected_suggestion = 0;
        // Explicitly clear suggestions on backspace/delete operations per request
        self.suggestions.clear();
    }

    pub fn delete_forward(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.delete_forward(),
            ChtshFocus::Query => self.query.delete_forward(),
        }
        self.selected_suggestion = 0;
        self.suggestions.clear();
    }

    pub fn delete_word_backward(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.delete_word_backward(),
            ChtshFocus::Query => self.query.delete_word_backward(),
        }
        self.selected_suggestion = 0;
        self.suggestions.clear();
    }

    pub fn delete_word_forward(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.delete_word_forward(),
            ChtshFocus::Query => self.query.delete_word_forward(),
        }
        self.selected_suggestion = 0;
        self.suggestions.clear();
    }

    pub fn move_left(&mut self) {
        let moved = match self.focus {
            ChtshFocus::Scope => self.scope.move_left(),
            ChtshFocus::Query => self.query.move_left(),
        };
        if !moved && self.focus == ChtshFocus::Query {
            self.focus = ChtshFocus::Scope;
            self.scope.move_end();
        }
    }

    pub fn move_right(&mut self) {
        let moved = match self.focus {
            ChtshFocus::Scope => self.scope.move_right(),
            ChtshFocus::Query => self.query.move_right(),
        };
        if !moved && self.focus == ChtshFocus::Scope {
            self.focus = ChtshFocus::Query;
            self.query.move_start();
        }
    }

    pub fn move_word_backward(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.move_word_backward(),
            ChtshFocus::Query => self.query.move_word_backward(),
        }
    }

    pub fn move_word_forward(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.move_word_forward(),
            ChtshFocus::Query => self.query.move_word_forward(),
        }
    }

    pub fn move_start(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.move_start(),
            ChtshFocus::Query => self.query.move_start(),
        }
    }

    pub fn move_end(&mut self) {
        match self.focus {
            ChtshFocus::Scope => self.scope.move_end(),
            ChtshFocus::Query => self.query.move_end(),
        }
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
                    self.scope.set_text(&sug);
                    self.focus = ChtshFocus::Query;
                }
                ChtshFocus::Query => {
                    self.query.set_text(&sug);
                }
            }
            self.selected_suggestion = 0;
            self.suggestions.clear();
        }
    }

    pub fn refresh_suggestions(&mut self) {
        match self.focus {
            ChtshFocus::Scope => {
                if !self.scope.is_empty() {
                    self.suggestions = fuzzy_suggest(&self.root_list, self.scope.value(), 5);
                } else {
                    self.suggestions.clear();
                }
            }
            ChtshFocus::Query => {
                if !self.topic_list.is_empty() && !self.query.is_empty() {
                    self.suggestions = fuzzy_suggest(&self.topic_list, self.query.value(), 5);
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

