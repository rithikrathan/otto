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
    /// Submitted prompt history (most recent last), for up/down navigation.
    pub history: Vec<String>,
    /// Current history cursor: `None` = not browsing (editing a fresh prompt).
    pub history_index: Option<usize>,
    /// Saved draft while browsing history, restored when reaching the end.
    pub history_draft: String,
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
    /// Frame ticker (advances the spinner).
    pub tick: u64,
    /// Default model name (from config/manage selection).
    pub model_name: String,
    /// Loaded models from Ollama API
    pub models: Vec<String>,
    /// Currently-open floating window (model picker / settings), if any.
    pub modal: Option<Modal>,
    /// Selection index inside the open modal.
    pub modal_index: usize,
    /// Editable settings shown/toggled in the settings window.
    pub settings: SettingsState,
    /// Set to `false` to exit the event loop.
    pub running: bool,
    /// Wait for a second Esc to abort generating
    pub pending_abort: bool,
    /// Handle to the current background task (to allow abortion).
    pub bg_task: Option<tokio::task::JoinHandle<()>>,
}

/// Editable settings surfaced in the floating settings window.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub model: String,
    pub server_url: String,
    pub stt_enabled: bool,
    pub stt_model_path: String,
    pub search_provider: String,
    pub search_summarize: bool,
}

/// A floating window layered over the main UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    ModelPicker,
    Settings,
}

impl App {
    pub fn new() -> Self {
        let tabs = vec![BufferId::Chat, BufferId::Search, BufferId::Chtsh];
        Self {
            tabs,
            active: 0,
            focus: Focus::Prompt,
            prompt: input::Prompt::new(),
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            busy: Vec::new(),
            tokens: TokenStats::new(),
            chat: buffers::chat::ChatBuffer::default(),
            search: buffers::search::SearchBuffer::default(),
            chtsh: buffers::chtsh::ChtshBuffer::default(),
            tick: 0,
            model_name: String::new(),
            models: Vec::new(),
            modal: None,
            modal_index: 0,
            settings: SettingsState {
                model: String::new(),
                server_url: String::new(),
                stt_enabled: false,
                stt_model_path: String::new(),
                search_provider: "duckduckgo".into(),
                search_summarize: true,
            },
            running: true,
            pending_abort: false,
            bg_task: None,
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
        }
        .min(1_000_000) as u16
    }

    /// Handle an event from the channel: route background results to buffers.
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Abort => {
                if let Some(task) = self.bg_task.take() {
                    task.abort();
                }
                self.busy.clear();
                self.chat.finish_assistant();
                self.chat.view.blocks.push(crate::buffers::Block {
                    kind: "info",
                    markdown: "*Aborted*".into(),
                });
            }
            AppEvent::Tick => {}
            AppEvent::Input(_) => {}
            AppEvent::MouseScroll { delta } => {
                let view = match self.active_buffer() {
                    BufferId::Chat => &mut self.chat.view,
                    BufferId::Search => &mut self.search.view,
                    BufferId::Chtsh => &mut self.chtsh.view,
                };
                view.scroll = (view.scroll as i32 + delta).max(0) as usize;
            }
            AppEvent::TokenStat {
                prompt_tokens,
                eval_tokens,
            } => {
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
                self.models = models;
                if self.model_name.is_empty() {
                    if let Some(first) = self.models.first() {
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

    /// Record a submitted prompt into history (no consecutive duplicates).
    pub fn history_push(&mut self, text: &str) {
        let t = text.trim().to_string();
        if t.is_empty() {
            return;
        }
        if self.history.last().map(String::as_str) != Some(t.as_str()) {
            self.history.push(t);
        }
        // Reset the browser to a fresh prompt.
        self.history_index = None;
        self.prompt.reset();
    }

    /// Go to the previous history entry (Up arrow at the first prompt line).
    pub fn history_back(&mut self) {
        if self.history.is_empty() {
            return;
        }
        // On first entry, stash the current draft so we can restore it later.
        if self.history_index.is_none() {
            self.history_draft = self.prompt.value().to_string();
            self.history_index = Some(self.history.len() - 1);
            self.load_history(self.history.len() - 1);
            return;
        }
        let idx = self.history_index.unwrap();
        if idx > 0 {
            self.history_index = Some(idx - 1);
            self.load_history(idx - 1);
        }
    }

    /// Go to the next history entry (Down arrow at the last prompt line).
    pub fn history_forward(&mut self) {
        let Some(idx) = self.history_index else {
            return;
        };
        let next = idx + 1;
        if next < self.history.len() {
            self.history_index = Some(next);
            self.load_history(next);
        } else {
            // Reached the end: restore the original draft.
            self.history_index = None;
            self.prompt.set_text(&self.history_draft);
        }
    }

    fn load_history(&mut self, idx: usize) {
        if let Some(text) = self.history.get(idx) {
            self.prompt.set_text(text);
        }
    }

    /// Open a floating window, resetting its selection.
    pub fn open_modal(&mut self, modal: Modal) {
        self.modal = Some(modal);
        self.modal_index = 0;
    }

    /// Close the currently-open floating window.
    pub fn close_modal(&mut self) {
        self.modal = None;
        self.modal_index = 0;
    }

    /// Move the modal selection up/down; clamps to the modal's item count.
    pub fn modal_move(&mut self, up: bool) {
        let len = match self.modal {
            Some(Modal::ModelPicker) => self.models.len(),
            Some(Modal::Settings) => settings_rows(),
            None => 0,
        };
        if len == 0 {
            return;
        }
        if up {
            self.modal_index = self.modal_index.saturating_sub(1);
        } else {
            self.modal_index = (self.modal_index + 1).min(len - 1);
        }

        if let Some(Modal::ModelPicker) = self.modal {
            if let Some(m) = self.models.get(self.modal_index).cloned() {
                self.model_name = m.clone();
                self.settings.model = m;
            }
        }
    }

    /// Current modal selection label (for rendering/activation).
    pub fn modal_selection(&self) -> Option<String> {
        match self.modal {
            Some(Modal::ModelPicker) => self.models.get(self.modal_index).cloned(),
            Some(Modal::Settings) => settings_row_label(self.modal_index).map(|s| s.to_string()),
            None => None,
        }
    }

    /// Render the modal as a list of `(label, value, selected)` rows.
    pub fn modal_rows(&self) -> Vec<(String, String, bool)> {
        let mut out = Vec::new();
        match self.modal {
            Some(Modal::ModelPicker) => {
                let models = self.models.clone();
                for (i, m) in models.iter().enumerate() {
                    let sel = self.model_name == *m;
                    out.push((m.clone(), "".to_string(), sel || i == self.modal_index));
                }
            }
            Some(Modal::Settings) => {
                let sel = self.modal_index;
                let values = [
                    self.settings.model.clone(),
                    self.settings.server_url.clone(),
                    self.settings.stt_enabled.to_string(),
                    self.settings.stt_model_path.clone(),
                    self.settings.search_provider.clone(),
                    self.settings.search_summarize.to_string(),
                ];
                for (i, label) in SETTINGS_ROWS.iter().enumerate() {
                    out.push(((*label).to_string(), values[i].clone(), i == sel));
                }
            }
            None => {}
        }
        out
    }

    /// Apply the current modal selection (Enter). Returns whether it consumed.
    pub fn modal_apply(&mut self) -> bool {
        match self.modal {
            Some(Modal::ModelPicker) => {
                if let Some(m) = self.models.get(self.modal_index).cloned() {
                    self.model_name = m;
                    self.settings.model = self.model_name.clone();
                }
                self.close_modal();
                true
            }
            Some(Modal::Settings) => {
                match settings_row_label(self.modal_index) {
                    Some("model") => {
                        // Jump to the model picker.
                        self.open_modal(Modal::ModelPicker);
                    }
                    Some("stt-enabled") => {
                        self.settings.stt_enabled = !self.settings.stt_enabled;
                    }
                    Some("search-summarize") => {
                        self.settings.search_summarize = !self.settings.search_summarize;
                    }
                    _ => {}
                }
                true
            }
            None => false,
        }
    }
}

/// Editable rows in the settings window.
const SETTINGS_ROWS: [&str; 6] = [
    "model",
    "server-url",
    "stt-enabled",
    "stt-model-path",
    "search-provider",
    "search-summarize",
];

/// Number of settings rows shown in the settings window.
pub fn settings_rows() -> usize {
    SETTINGS_ROWS.len()
}

/// Label for a settings row index.
pub fn settings_row_label(i: usize) -> Option<&'static str> {
    SETTINGS_ROWS.get(i).copied()
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

    #[test]
    fn history_push_dedupes_and_resets_prompt() {
        let mut app = App::new();
        app.prompt.set_text("hello");
        app.history_push("hello");
        assert_eq!(app.history, vec!["hello".to_string()]);
        assert!(app.prompt.is_empty());
        // duplicate is not added again
        app.history_push("hello");
        assert_eq!(app.history.len(), 1);
    }

    #[test]
    fn history_back_forward_restores_draft() {
        let mut app = App::new();
        app.history_push("one");
        app.history_push("two");
        app.prompt.set_text("draft "); // user started typing

        app.history_back();
        assert_eq!(app.prompt.value(), "two");
        app.history_back();
        assert_eq!(app.prompt.value(), "one");
        app.history_back(); // already at oldest: no change
        assert_eq!(app.prompt.value(), "one");

        app.history_forward();
        assert_eq!(app.prompt.value(), "two");
        app.history_forward(); // past the end: restore draft
        assert_eq!(app.prompt.value(), "draft ");
    }

    #[test]
    fn prompt_up_down_moves_between_lines_and_stops_at_edges() {
        let mut p = input::Prompt::new();
        p.set_text("line one\nline two\nline three");
        p.cursor = 0; // start of "line one"
        p.move_up(); // already at first line: stays
        assert_eq!(p.cursor, 0);
        p.move_down();
        assert_eq!(&p.value()[..p.cursor], "line one\n"); // start of line two
        p.move_down();
        assert_eq!(&p.value()[..p.cursor], "line one\nline two\n"); // start of line three
        p.move_down(); // already at last line: stays
        assert_eq!(&p.value()[..p.cursor], "line one\nline two\n");
    }

    #[test]
    fn model_picker_apply_sets_model_and_closes() {
        let mut app = App::new();
        app.manage.models = vec!["a".into(), "b".into(), "c".into()];
        app.open_modal(Modal::ModelPicker);
        assert_eq!(app.modal, Some(Modal::ModelPicker));
        app.modal_move(false); // index 1
        app.modal_move(false); // index 2
        assert!(app.modal_apply());
        assert_eq!(app.model_name, "c");
        assert_eq!(app.settings.model, "c");
        assert_eq!(app.modal, None);
    }

    #[test]
    fn modal_move_clamps_and_close_resets() {
        let mut app = App::new();
        app.manage.models = vec!["x".into()];
        app.open_modal(Modal::ModelPicker);
        app.modal_move(true); // stays at 0
        assert_eq!(app.modal_index, 0);
        app.modal_move(false); // clamped at 0 (only one)
        assert_eq!(app.modal_index, 0);
        app.close_modal();
        assert_eq!(app.modal, None);
        assert_eq!(app.modal_index, 0);
    }

    #[test]
    fn settings_toggle_flips_boolean_row() {
        let mut app = App::new();
        app.settings.stt_enabled = false;
        app.open_modal(Modal::Settings);
        // navigate to the "stt-enabled" row (index 2)
        app.modal_index = 2;
        app.modal_apply();
        assert_eq!(app.settings.stt_enabled, true);
        assert_eq!(app.modal, Some(Modal::Settings));
        // "model" row jumps to the picker
        app.modal_index = 0;
        app.modal_apply();
        assert_eq!(app.modal, Some(Modal::ModelPicker));
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

        /// Replace the whole prompt (history load), caret to the end.
        pub fn set_text(&mut self, text: &str) {
            self.text = text.to_string();
            self.cursor = self.text.len();
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
            let len = self.text[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.text.replace_range(self.cursor..self.cursor + len, "");
            self.clamp_scroll();
        }

        pub fn delete_word_backward(&mut self) {
            if self.cursor == 0 { return; }
            let before = &self.text[..self.cursor];
            let mut iter = before.char_indices().rev().peekable();
            while let Some(&(_, c)) = iter.peek() {
                if c.is_whitespace() { iter.next(); } else { break; }
            }
            while let Some(&(_, c)) = iter.peek() {
                if !c.is_whitespace() { iter.next(); } else { break; }
            }
            let new_cursor = iter.peek().map(|&(i, c)| i + c.len_utf8()).unwrap_or(0);
            self.text.replace_range(new_cursor..self.cursor, "");
            self.cursor = new_cursor;
            self.clamp_scroll();
        }

        pub fn delete_word_forward(&mut self) {
            if self.cursor >= self.text.len() { return; }
            let after = &self.text[self.cursor..];
            let mut iter = after.char_indices().peekable();
            while let Some(&(_, c)) = iter.peek() {
                if !c.is_whitespace() { iter.next(); } else { break; }
            }
            while let Some(&(_, c)) = iter.peek() {
                if c.is_whitespace() { iter.next(); } else { break; }
            }
            let end = self.cursor + iter.peek().map(|&(i, _)| i).unwrap_or(after.len());
            self.text.replace_range(self.cursor..end, "");
            self.clamp_scroll();
        }

        pub fn move_left(&mut self) {
            if self.cursor > 0 {
                let prev = self.text[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                self.cursor = prev;
            }
            self.clamp_scroll();
        }

        pub fn move_right(&mut self) {
            if self.cursor < self.text.len() {
                let next = self.text[self.cursor..]
                    .chars()
                    .next()
                    .map(|c| self.cursor + c.len_utf8())
                    .unwrap_or(self.cursor);
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
            self.scroll =
                (self.scroll + 1).min(self.wrapped_line_count().saturating_sub(MAX_LINES));
        }

        /// Move the caret up one logical line, preserving the column.
        pub fn move_up(&mut self) {
            let col = self.cursor_column();
            let before = &self.text[..self.cursor.min(self.text.len())];
            if let Some(prev_nl) = before.rfind('\n') {
                let line_start = &self.text[..prev_nl];
                let line_col = current_line_column(line_start);
                self.cursor = prev_nl + col.min(line_col);
            } else {
                self.cursor = 0;
            }
            self.clamp_scroll();
        }

        /// Move the caret down one logical line, preserving the column.
        pub fn move_down(&mut self) {
            let col = self.cursor_column();
            let after = &self.text[self.cursor.min(self.text.len())..];
            // Only move down if there is another line below the caret.
            if let Some(rel) = after.find('\n') {
                let line_start = self.cursor + rel + 1;
                let line = &self.text[line_start..];
                self.cursor = line_start + col.min(current_line_column(line));
                self.clamp_scroll();
            }
        }

        /// Column of the caret within its current logical line (in chars).
        fn cursor_column(&self) -> usize {
            let before = &self.text[..self.cursor.min(self.text.len())];
            match before.rfind('\n') {
                Some(pos) => before[pos + 1..].chars().count(),
                None => before.chars().count(),
            }
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

    /// Number of characters in a single line (used for column clamping).
    fn current_line_column(line: &str) -> usize {
        line.chars().count()
    }
}
