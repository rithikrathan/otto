//! Global application state: buffers, active buffer, prompt, busy state.

use crate::buffers::{self, BufferId};
use crate::event::AppEvent;

/// Which region currently holds keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Buffer,
    Prompt,
}

/// Top-level application state shared across the UI.
pub struct App {
    /// Ordered buffer tabs; cycling uses this vector.
    pub tabs: Vec<BufferId>,
    /// Index of the active buffer.
    pub active: usize,
    /// Where keyboard input is directed.
    pub focus: Focus,
    /// The shared prompt string (grows up to 5 lines).
    pub prompt: input::Prompt,
    /// In-flight background jobs (drive the spinner).
    pub busy: Vec<JobKind>,
    /// Token accounting shown in the statusline (reading / writing / ctx %).
    pub tokens: TokenStats,
    /// Chat buffer state.
    pub chat: buffers::chat::ChatBuffer,
    /// Search buffer state.
    pub search: buffers::search::SearchBuffer,
    /// cht.sh buffer state.
    pub chtsh: buffers::chtsh::ChtshBuffer,
    /// Manage buffer state.
    pub manage: buffers::manage::ManageBuffer,
    /// Frame ticker (advances the spinner).
    pub tick: u64,
    /// Default model name (from config/manage selection).
    pub model_name: String,
    /// Set to `false` to exit the event loop.
    pub running: bool,
}

impl App {
    pub fn new() -> Self {
        let tabs = vec![
            BufferId::Chat,
            BufferId::Search,
            BufferId::Chtsh,
            BufferId::Manage,
        ];
        Self {
            tabs,
            active: 0,
            focus: Focus::Prompt,
            prompt: input::Prompt::new(),
            busy: Vec::new(),
            tokens: TokenStats::new(),
            chat: buffers::chat::ChatBuffer::default(),
            search: buffers::search::SearchBuffer::default(),
            chtsh: buffers::chtsh::ChtshBuffer::default(),
            manage: buffers::manage::ManageBuffer::default(),
            tick: 0,
            model_name: String::new(),
            running: true,
        }
    }

    /// Move to the next buffer (Tab).
    pub fn next_buffer(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// Move to the previous buffer (Shift+Tab).
    pub fn prev_buffer(&mut self) {
        self.active = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
    }

    pub fn active_buffer(&self) -> BufferId {
        self.tabs[self.active]
    }

    /// Scroll offset for the active buffer (used by the buffer renderer).
    pub fn active_scroll(&self) -> u16 {
        match self.active_buffer() {
            BufferId::Chat => self.chat.view.scroll,
            BufferId::Search => self.search.view.scroll,
            BufferId::Chtsh => self.chtsh.view.scroll,
            BufferId::Manage => self.manage.view.scroll,
        }
        .min(1_000_000) as u16
    }

    /// Handle an event from the channel: route background results to buffers.
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Tick => {}
            AppEvent::Input(_) => {}
            AppEvent::MouseScroll { delta } => {
                let view = match self.active_buffer() {
                    BufferId::Chat => &mut self.chat.view,
                    BufferId::Search => &mut self.search.view,
                    BufferId::Chtsh => &mut self.chtsh.view,
                    BufferId::Manage => &mut self.manage.view,
                };
                view.scroll = (view.scroll as i32 + delta).max(0) as usize;
            }
            AppEvent::TokenStat { prompt_tokens, eval_tokens } => {
                // Ollama streams report per-request counts on the done chunk;
                // accumulate into the running totals.
                self.tokens.accumulate(prompt_tokens, eval_tokens, None);
            }
            AppEvent::ChatDelta { buffer, delta } => {
                if buffer == BufferId::Chat {
                    if !self.chat.streaming {
                        self.chat.begin_assistant();
                    }
                    self.chat.push_assistant(&delta);
                }
            }
            AppEvent::ChatDone { buffer } => {
                if buffer == BufferId::Chat {
                    self.chat.finish_assistant();
                }
                self.remove_job(JobKind::Chat);
            }
            AppEvent::ChatError { buffer, msg } => {
                if buffer == BufferId::Chat {
                    self.chat.finish_assistant();
                    self.chat.view.blocks.push(crate::buffers::Block {
                        kind: "error",
                        markdown: format!("*{msg}*"),
                    });
                }
                self.remove_job(JobKind::Chat);
            }
            AppEvent::ModelsLoaded(models) => {
                self.manage.models = models;
                if self.manage.model_index >= self.manage.models.len() {
                    self.manage.model_index = 0;
                }
                if self.model_name.is_empty() {
                    if let Some(first) = self.manage.models.first() {
                        self.model_name = first.clone();
                    }
                }
                self.remove_job(JobKind::Models);
            }
            AppEvent::SearchDone { markdown } => {
                let q = self.search.last_query.clone().unwrap_or_default();
                self.search.add_result(&q, &markdown);
                self.remove_job(JobKind::SearchFetch);
                self.remove_job(JobKind::SearchPlan);
            }
            AppEvent::SearchError { msg } => {
                self.search.view.blocks.push(crate::buffers::Block {
                    kind: "error",
                    markdown: format!("*{msg}*"),
                });
                self.remove_job(JobKind::SearchFetch);
                self.remove_job(JobKind::SearchPlan);
            }
            AppEvent::ChtshDone { text } => {
                let q = self.chtsh.last_query.clone().unwrap_or_default();
                self.chtsh.add_result(&q, &text);
                self.remove_job(JobKind::ChtshFetch);
                self.remove_job(JobKind::ChtshPlan);
            }
            AppEvent::ChtshError { msg } => {
                self.chtsh.view.blocks.push(crate::buffers::Block {
                    kind: "error",
                    markdown: format!("*{msg}*"),
                });
                self.remove_job(JobKind::ChtshFetch);
                self.remove_job(JobKind::ChtshPlan);
            }
            AppEvent::SttPartial { .. } => {}
            AppEvent::SttFinal { text } => {
                self.prompt.text.push_str(&text);
                self.prompt.cursor = self.prompt.text.len();
                self.remove_job(JobKind::Stt);
            }
            AppEvent::SttError { msg } => {
                self.busy.retain(|j| *j != JobKind::Stt);
                self.chat.view.blocks.push(crate::buffers::Block {
                    kind: "error",
                    markdown: format!("*stt: {msg}*"),
                });
            }
            AppEvent::SearchPlan { .. } | AppEvent::ChtshPlan { .. } => {}
            AppEvent::MarkBusy { job, on } => {
                if on {
                    if !self.busy.contains(&job) {
                        self.busy.push(job);
                    }
                } else {
                    self.busy.retain(|j| *j != job);
                }
            }
        }
    }

    /// Remove one in-flight job marker.
    pub fn remove_job(&mut self, job: JobKind) {
        self.busy.retain(|j| *j != job);
    }
}

/// Type of in-flight background work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Chat,
    SearchPlan,
    SearchFetch,
    ChtshPlan,
    ChtshFetch,
    Stt,
    Models,
}

/// Live token accounting for the statusline.
///
/// - `prompt_tokens`: tokens read in (sent to the model) for the conversation.
/// - `eval_tokens`:   tokens written out (generated) for the conversation.
/// - `num_ctx`:       the model's context window size (tokens); used to show
///                    how full the context is. Falls back to a default when
///                    unknown.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenStats {
    pub prompt_tokens: u64,
    pub eval_tokens: u64,
    pub num_ctx: u64,
}

/// Fallback context window used when the model doesn't report `num_ctx`.
const DEFAULT_NUM_CTX: u64 = 4096;

impl TokenStats {
    pub fn new() -> Self {
        Self {
            prompt_tokens: 0,
            eval_tokens: 0,
            num_ctx: DEFAULT_NUM_CTX,
        }
    }

    /// Accumulate fresh delta counts from a stream response.
    pub fn accumulate(&mut self, prompt_delta: u64, eval_delta: u64, num_ctx: Option<u64>) {
        self.prompt_tokens += prompt_delta;
        self.eval_tokens += eval_delta;
        if let Some(n) = num_ctx {
            self.num_ctx = n.max(1);
        }
    }

    /// Context window usage as a percentage (`0..=100`).
    pub fn context_percent(&self) -> u8 {
        let used = self.prompt_tokens.min(self.num_ctx);
        ((used as f64 / self.num_ctx as f64) * 100.0).round() as u8
    }

    /// Reset counters (called on `/clear` to reflect the reset context).
    pub fn reset(&mut self) {
        self.prompt_tokens = 0;
        self.eval_tokens = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_accumulate_and_percent() {
        let mut t = TokenStats::new();
        t.accumulate(2048, 120, Some(4096));
        assert_eq!(t.prompt_tokens, 2048);
        assert_eq!(t.eval_tokens, 120);
        assert_eq!(t.context_percent(), 50);
        t.accumulate(2048, 0, None);
        assert_eq!(t.prompt_tokens, 4096);
        assert_eq!(t.context_percent(), 100);
    }

    #[test]
    fn reset_clears_counts() {
        let mut t = TokenStats::new();
        t.accumulate(500, 30, Some(4096));
        t.reset();
        assert_eq!(t.prompt_tokens, 0);
        assert_eq!(t.eval_tokens, 0);
    }

    #[test]
    fn buffer_cycling() {
        let mut app = App::new();
        assert_eq!(app.active_buffer(), BufferId::Chat);
        app.next_buffer();
        assert_eq!(app.active_buffer(), BufferId::Search);
        app.next_buffer();
        app.next_buffer();
        assert_eq!(app.active_buffer(), BufferId::Manage);
        app.next_buffer();
        assert_eq!(app.active_buffer(), BufferId::Chat); // wraps
        app.prev_buffer();
        assert_eq!(app.active_buffer(), BufferId::Manage); // wraps back
    }

    #[test]
    fn mouse_scroll_only_moves_active_buffer_and_stays_positive() {
        let mut app = App::new();
        assert_eq!(app.active_buffer(), BufferId::Chat);
        app.chat.view.scroll = 0;
        app.handle_event(AppEvent::MouseScroll { delta: -5 });
        assert_eq!(app.chat.view.scroll, 0); // clamped at zero
        app.handle_event(AppEvent::MouseScroll { delta: 3 });
        assert_eq!(app.chat.view.scroll, 3);
        app.next_buffer(); // Search
        app.handle_event(AppEvent::MouseScroll { delta: 2 });
        assert_eq!(app.search.view.scroll, 2);
        assert_eq!(app.chat.view.scroll, 3); // chat untouched
    }
}

pub mod input {
    //! The shared prompt box: a string + char cursor, expandable to 5 lines.
    //!
    //! Text wraps at `width` columns. The box shows at most `MAX_LINES` visual
    //! rows; once the content exceeds that it scrolls internally (`scroll`).

    use crossterm::event::KeyCode;

    pub const MAX_LINES: usize = 5;

    pub struct Prompt {
        pub text: String,
        /// Cursor position in bytes.
        pub cursor: usize,
        /// Which visual row index is the first visible one (when scrolling).
        pub scroll: usize,
        /// The width (columns) the prompt was last laid out at — used to
        /// recompute wrapping for insert/delete.
        pub width: usize,
    }

    impl Prompt {
        pub fn new() -> Self {
            Self {
                text: String::new(),
                cursor: 0,
                scroll: 0,
                width: 40,
            }
        }

        pub fn reset(&mut self) {
            self.text.clear();
            self.cursor = 0;
            self.scroll = 0;
        }

        pub fn is_empty(&self) -> bool {
            self.text.is_empty()
        }

        /// Content of the prompt (used as the submit payload).
        pub fn value(&self) -> &str {
            &self.text
        }

        fn set_width(&mut self, width: usize) {
            if self.width != width {
                self.width = width.max(1);
                self.clamp_scroll();
            }
        }

        /// Insert `c` at the cursor and advance.
        pub fn insert_char(&mut self, c: char) {
            self.text.insert(self.cursor, c);
            self.cursor += c.len_utf8();
            self.clamp_scroll();
        }

        /// Wrap should expose the substring of text before cursor.
        pub fn before(&self) -> &str {
            &self.text[..self.cursor.min(self.text.len())]
        }

        pub fn after(&self) -> &str {
            &self.text[self.cursor.min(self.text.len())..]
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
            self.clamp_scroll();
        }

        pub fn delete_forward(&mut self) {
            if self.cursor >= self.text.chars().count() {
                return;
            }
            let len = self.text[self.cursor..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
            self.text.replace_range(self.cursor..self.cursor + len, "");
            self.clamp_scroll();
        }

        pub fn move_left(&mut self) {
            if self.cursor > 0 {
                let prev = self.text[..self.cursor].char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
                self.cursor = prev;
            }
            self.clamp_scroll();
        }

        pub fn move_right(&mut self) {
            if self.cursor < self.text.len() {
                let next = self.text[self.cursor..].chars().next().map(|c| self.cursor + c.len_utf8()).unwrap_or(self.cursor);
                self.cursor = next;
            }
            self.clamp_scroll();
        }

        pub fn move_home(&mut self) {
            self.cursor = 0;
            self.clamp_scroll();
        }

        pub fn move_end(&mut self) {
            self.cursor = self.text.len();
            self.clamp_scroll();
        }

        pub fn move_line_start(&mut self) {
            // Move to start of the current logical line.
            if let Some(pos) = self.before().rfind('\n') {
                self.cursor = pos + 1;
            } else {
                self.cursor = 0;
            }
            self.clamp_scroll();
        }

        pub fn move_line_end(&mut self) {
            if let Some(pos) = self.after().find('\n') {
                self.cursor = self.text.len() - self.after().len() + pos;
            } else {
                self.cursor = self.text.len();
            }
            self.clamp_scroll();
        }

        /// Number of wrapped rows the content occupies at the given width.
        fn wrapped_len(&self, text: &str, width: usize) -> usize {
            if text.is_empty() {
                return 1;
            }
            // Guard against a zero width (no layout yet); avoid divide-by-zero.
            let width = width.max(1);
            let mut rows = 0usize;
            for line in text.split('\n') {
                if line.is_empty() {
                    rows += 1;
                } else {
                    let chars = line.chars().count();
                    rows += (chars + width - 1) / width;
                }
            }
            rows.max(1)
        }

        /// Row index (0-based) where the caret sits. TODO(stub): exact caret row.
        pub fn caret_row(&self) -> usize {
            let w = self.width;
            self.wrapped_len(self.before(), w).saturating_sub(1)
        }

        /// Total wrapped rows at the current width.
        pub fn wrapped_line_count(&self) -> usize {
            self.wrapped_len(&self.text, self.width)
        }

        /// Number of visible rows (capped at MAX_LINES).
        pub fn visible_lines(&self) -> usize {
            self.wrapped_line_count().min(MAX_LINES)
        }

        fn clamp_scroll(&mut self) {
            let total = self.wrapped_line_count();
            let max_scroll = total.saturating_sub(MAX_LINES);
            self.scroll = self.scroll.min(max_scroll);
            // Keep the caret visible.
            let caret = self.caret_row();
            if caret < self.scroll {
                self.scroll = caret;
            } else if caret >= self.scroll + MAX_LINES {
                self.scroll = caret.saturating_sub(MAX_LINES - 1);
            }
        }

        pub fn scroll_up(&mut self) {
            self.scroll = self.scroll.saturating_sub(1);
        }

        pub fn scroll_down(&mut self) {
            self.scroll = (self.scroll + 1).min(self.wrapped_line_count().saturating_sub(MAX_LINES));
        }

        /// Handle a key relevant to editing (returns true if consumed).
        pub fn key(&mut self, key: KeyCode, width: usize) -> bool {
            self.set_width(width);
            match key {
                KeyCode::Char(c) => {
                    self.insert_char(c);
                    true
                }
                KeyCode::Backspace => {
                    self.delete_backward();
                    true
                }
                KeyCode::Delete => {
                    self.delete_forward();
                    true
                }
                KeyCode::Left => {
                    self.move_left();
                    true
                }
                KeyCode::Right => {
                    self.move_right();
                    true
                }
                KeyCode::Home => {
                    self.move_home();
                    true
                }
                KeyCode::End => {
                    self.move_end();
                    true
                }
                KeyCode::Enter => {
                    // Enter submits; Alt+Enter inserts a newline. Per design,
                    // we insert a newline here and let the caller decide.
                    self.insert_char('\n');
                    true
                }
                _ => false,
            }
        }
    }
}