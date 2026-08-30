//! Documentation search buffer: interactive results list + full rendered markdown document view.

use crate::search::{format_search_results_markdown, SearchResult};
use super::{Block, BufferState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    Results,
    Document,
}

#[derive(Debug, Clone, Default)]
pub struct SearchBuffer {
    pub view: BufferState,
    pub mode: SearchMode,
    pub results: Vec<SearchResult>,
    pub selected_result: usize,
    pub active_doc_url: Option<String>,
    pub active_doc_title: Option<String>,
    pub active_doc_markdown: Option<String>,
    pub last_query: Option<String>,
}

impl SearchBuffer {
    pub fn set_results(&mut self, query: &str, results: Vec<SearchResult>) {
        self.last_query = Some(query.to_string());
        self.results = results;
        self.selected_result = 0;
        self.mode = SearchMode::Results;
        self.render_results_view();
    }

    pub fn render_results_view(&mut self) {
        let query = self.last_query.as_deref().unwrap_or("");
        let md = format_search_results_markdown(query, &self.results, self.selected_result);
        self.view.blocks = vec![Block {
            kind: "search_results".to_string(),
            markdown: md,
        }];
        self.mode = SearchMode::Results;
    }

    pub fn set_document(&mut self, url: &str, title: &str, markdown: &str) {
        self.active_doc_url = Some(url.to_string());
        self.active_doc_title = Some(title.to_string());
        self.active_doc_markdown = Some(markdown.to_string());
        self.mode = SearchMode::Document;

        let full_md = format!("# {}\n\n`{}`\n\n---\n\n{}", title, url, markdown);
        self.view.blocks = vec![Block {
            kind: "doc_view".to_string(),
            markdown: full_md,
        }];
        self.view.scroll = 0;
    }

    pub fn back_to_results(&mut self) -> bool {
        if self.mode == SearchMode::Document && !self.results.is_empty() {
            self.render_results_view();
            true
        } else {
            false
        }
    }

    pub fn next_result(&mut self) {
        if !self.results.is_empty() && self.mode == SearchMode::Results {
            self.selected_result = (self.selected_result + 1) % self.results.len();
            self.render_results_view();
        }
    }

    pub fn prev_result(&mut self) {
        if !self.results.is_empty() && self.mode == SearchMode::Results {
            if self.selected_result == 0 {
                self.selected_result = self.results.len() - 1;
            } else {
                self.selected_result -= 1;
            }
            self.render_results_view();
        }
    }

    pub fn selected_item(&self) -> Option<&SearchResult> {
        if self.mode == SearchMode::Results {
            self.results.get(self.selected_result)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.view.blocks.clear();
        self.view.scroll = 0;
        self.results.clear();
        self.selected_result = 0;
        self.active_doc_url = None;
        self.active_doc_title = None;
        self.active_doc_markdown = None;
        self.last_query = None;
        self.mode = SearchMode::Results;
    }
}
