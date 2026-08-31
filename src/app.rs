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
    /// Active provider name (e.g. ollama, openai, groq, gemini, nvidia, openrouter).
    pub provider_name: String,
    /// Ordered provider tabs for model picker.
    pub provider_list: Vec<String>,
    /// Active provider tab index in model picker.
    pub provider_index: usize,
    /// Loaded models per provider.
    pub provider_models: std::collections::HashMap<String, Vec<String>>,
    /// Loaded models from Ollama API
    pub models: Vec<String>,
    /// Currently-open floating window (model picker / settings), if any.
    pub modal: Option<Modal>,
    /// Search query for the model picker.
    pub modal_search: String,
    /// Whether the model picker search is focused.
    pub modal_search_focused: bool,
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
    /// Connection status to the server.
    pub is_connected: bool,
}

/// Editable settings surfaced in the floating settings window.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub model: String,
    pub server_url: String,
    pub search_provider: String,
    pub search_summarize: bool,
}

/// A floating window layered over the main UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    ModelPicker,
    Settings,
    SearchQueryPicker(Vec<String>),
    Help,
}

impl App {
    pub fn new() -> Self {
        let tabs = vec![BufferId::Chat, BufferId::Search, BufferId::Chtsh];
        let provider_list = vec![
            "ollama".to_string(),
            "groq".to_string(),
            "gemini".to_string(),
            "nvidia".to_string(),
        ];
        let mut app = Self {
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
            provider_name: "ollama".to_string(),
            provider_list,
            provider_index: 0,
            provider_models: std::collections::HashMap::new(),
            models: Vec::new(),
            modal: None,
            modal_search: String::new(),
            modal_search_focused: false,
            modal_index: 0,
            settings: SettingsState {
                model: String::new(),
                server_url: String::new(),
                search_provider: "duckduckgo".into(),
                search_summarize: true,
            },
            running: true,
            pending_abort: false,
            bg_task: None,
            is_connected: true,
        };
        app.chtsh.refresh_suggestions();
        app
    }

    pub fn init_from_config(&mut self, config: &crate::config::Config) {
        self.provider_name = config.server.provider.clone();
        if let Some(pos) = self.provider_list.iter().position(|p| p == &self.provider_name) {
            self.provider_index = pos;
        }

        self.provider_models.insert("ollama".into(), config.providers.ollama.models.clone());
        self.provider_models.insert("openai".into(), config.providers.openai.models.clone());
        self.provider_models.insert("groq".into(), config.providers.groq.models.clone());
        self.provider_models.insert("gemini".into(), config.providers.gemini.models.clone());
        self.provider_models.insert("nvidia".into(), config.providers.nvidia.models.clone());
        self.provider_models.insert("openrouter".into(), config.providers.openrouter.models.clone());
        self.provider_models.insert("custom".into(), config.providers.custom.models.clone());

        self.models = self.provider_models.get(&self.provider_name).cloned().unwrap_or_default();
        if self.model_name.is_empty() {
            self.model_name = config.model.name.clone();
        }
        self.settings.model = self.model_name.clone();
        self.settings.server_url = config.resolve_url(&self.provider_name);
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
                    kind: "info".to_string(),
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
                let max_scroll = view.last_max_scroll.get();
                let mut current_scroll = if view.auto_scroll {
                    max_scroll as i32
                } else {
                    view.scroll as i32
                };

                current_scroll = (current_scroll + delta).max(0);

                if current_scroll >= max_scroll as i32 {
                    view.auto_scroll = true;
                    view.scroll = max_scroll;
                } else {
                    view.auto_scroll = false;
                    view.scroll = current_scroll as usize;
                }
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
                        let m = self.model_name.clone();
                        self.chat.begin_assistant(&m);
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
                        kind: "error".to_string(),
                        markdown: format!("*{msg}*"),
                    });
                }
                self.remove_job(JobKind::Chat);
            }
            AppEvent::ModelsLoaded(models) => {
                if !models.is_empty() {
                    self.provider_models.insert(self.provider_name.clone(), models.clone());
                    self.models = models;
                }
                if self.model_name.is_empty() {
                    if let Some(first) = self.models.first() {
                        self.model_name = first.clone();
                    }
                }
                self.remove_job(JobKind::Models);
            }
            AppEvent::ProviderModelsLoaded { provider, models } => {
                if !models.is_empty() {
                    self.provider_models.insert(provider.clone(), models.clone());
                    if self.active_provider_tab() == provider || self.provider_name == provider {
                        self.models = self.provider_models.get(&self.provider_name).cloned().unwrap_or(models);
                    }
                }
            }
            AppEvent::SearchResultsLoaded { query, results } => {
                self.search.set_results(&query, results);
                self.remove_job(JobKind::SearchFetch);
                self.remove_job(JobKind::SearchPlan);
            }
            AppEvent::SearchDocumentLoaded { url, title, markdown, .. } => {
                self.search.set_document(&url, &title, &markdown);
                self.remove_job(JobKind::SearchFetch);
                self.remove_job(JobKind::SearchPlan);
            }
            AppEvent::SearchError { msg } => {
                self.search.view.blocks.push(crate::buffers::Block {
                    kind: "error".to_string(),
                    markdown: format!("*Search Error: {msg}*"),
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
                    kind: "error".to_string(),
                    markdown: format!("*{msg}*"),
                });
                self.remove_job(JobKind::ChtshFetch);
                self.remove_job(JobKind::ChtshPlan);
            }
            AppEvent::ChtshRootLoaded(list) => {
                self.chtsh.root_list = list;
                self.chtsh.refresh_suggestions();
            }
            AppEvent::ChtshTopicLoaded { lang, topics } => {
                if self.chtsh.scope.value().eq_ignore_ascii_case(&lang) {
                    self.chtsh.topic_list = topics;
                    self.chtsh.last_topic_scope = Some(lang);
                    self.chtsh.refresh_suggestions();
                }
            }
            AppEvent::ChtshPlan { .. } => {}
            AppEvent::MarkBusy { job, on } => {
                if on {
                    if !self.busy.contains(&job) {
                        self.busy.push(job);
                    }
                } else {
                    self.busy.retain(|j| *j != job);
                }
            }
            AppEvent::ConnectionStatus(status) => {
                self.is_connected = status;
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
        self.modal_search.clear();
        self.modal_search_focused = false;
        self.modal_index = 0;
    }

    /// Close the currently-open floating window.
    pub fn close_modal(&mut self) {
        self.modal = None;
        self.modal_search.clear();
        self.modal_search_focused = false;
        self.modal_index = 0;
    }

    pub fn active_provider_tab(&self) -> &str {
        self.provider_list.get(self.provider_index).map(|s| s.as_str()).unwrap_or("ollama")
    }

    pub fn modal_next_provider(&mut self) {
        if !self.provider_list.is_empty() {
            self.provider_index = (self.provider_index + 1) % self.provider_list.len();
            self.modal_index = 0;
            let p = self.active_provider_tab().to_string();
            self.models = self.provider_models.get(&p).cloned().unwrap_or_default();
        }
    }

    pub fn modal_prev_provider(&mut self) {
        if !self.provider_list.is_empty() {
            if self.provider_index == 0 {
                self.provider_index = self.provider_list.len() - 1;
            } else {
                self.provider_index -= 1;
            }
            self.modal_index = 0;
            let p = self.active_provider_tab().to_string();
            self.models = self.provider_models.get(&p).cloned().unwrap_or_default();
        }
    }

    /// Return models with their provider: (provider_id, model_name)
    pub fn filtered_models_with_provider(&self) -> Vec<(String, String)> {
        if self.modal_search.is_empty() {
            let p = self.active_provider_tab().to_string();
            let list = self.provider_models.get(&p).cloned().unwrap_or_default();
            list.into_iter().map(|m| (p.clone(), m)).collect()
        } else {
            let lower = self.modal_search.to_lowercase();
            let mut matches = Vec::new();
            for p in &self.provider_list {
                if let Some(list) = self.provider_models.get(p) {
                    for m in list {
                        if m.to_lowercase().contains(&lower) || p.to_lowercase().contains(&lower) {
                            matches.push((p.clone(), m.clone()));
                        }
                    }
                }
            }
            matches
        }
    }

    /// Move the modal selection up/down; clamps to the modal's item count.
    pub fn filtered_models(&self) -> Vec<String> {
        self.filtered_models_with_provider()
            .into_iter()
            .map(|(_, m)| m)
            .collect()
    }

    pub fn modal_move(&mut self, up: bool) {
        let len = match &self.modal {
            Some(Modal::ModelPicker) => self.filtered_models_with_provider().len(),
            Some(Modal::Settings) => settings_rows(),
            Some(Modal::SearchQueryPicker(opts)) => opts.len(),
            Some(Modal::Help) => 16,
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
    }

    /// Current modal selection label (for rendering/activation).
    pub fn modal_selection(&self) -> Option<String> {
        match &self.modal {
            Some(Modal::ModelPicker) => self.filtered_models().get(self.modal_index).cloned(),
            Some(Modal::Settings) => settings_row_label(self.modal_index).map(|s| s.to_string()),
            Some(Modal::SearchQueryPicker(opts)) => opts.get(self.modal_index).cloned(),
            Some(Modal::Help) => None,
            None => None,
        }
    }

    /// Render the modal as a list of `(label, value, selected)` rows.
    pub fn modal_rows(&self) -> Vec<(String, String, bool)> {
        let mut out = Vec::new();
        match &self.modal {
            Some(Modal::ModelPicker) => {
                let items = self.filtered_models_with_provider();
                for (i, (prov, m)) in items.iter().enumerate() {
                    let is_active = self.provider_name == *prov && self.model_name == *m;
                    let label = if self.modal_search.is_empty() {
                        m.clone()
                    } else {
                        format!("[{prov}] {m}")
                    };
                    let value = if is_active { "✓ (active)".to_string() } else { "".to_string() };
                    out.push((label, value, i == self.modal_index));
                }
            }
            Some(Modal::Settings) => {
                let sel = self.modal_index;
                let values = [
                    self.settings.model.clone(),
                    self.settings.server_url.clone(),
                    self.settings.search_provider.clone(),
                    self.settings.search_summarize.to_string(),
                ];
                for (i, label) in SETTINGS_ROWS.iter().enumerate() {
                    out.push(((*label).to_string(), values[i].clone(), i == sel));
                }
            }
            Some(Modal::SearchQueryPicker(opts)) => {
                for (i, opt) in opts.iter().enumerate() {
                    out.push((opt.clone(), "".to_string(), i == self.modal_index));
                }
            }
            Some(Modal::Help) => {
                let help_entries = [
                    ("Tab / BackTab", "Switch buffer (Chat / Search / Chtsh)"),
                    ("? / /help", "Toggle this help window"),
                    ("Enter", "Submit / Fetch / Select"),
                    ("Shift+Enter", "Insert newline in prompt"),
                    ("Ctrl+K", "Clear chat context"),
                    ("/settings", "Open settings"),
                    ("/model", "Open model picker"),
                    ("Left / Right (in Model Picker)", "Switch provider category"),
                    ("Ctrl+Left / Right", "Move word backward / forward"),
                    ("Ctrl+W / Ctrl+Bksp", "Delete word backward"),
                    ("Ctrl+Delete", "Delete word forward"),
                    ("Up / Down", "History / suggestions / modal nav"),
                    ("Space (in cht.sh)", "Accept highlighted suggestion"),
                    ("Left/Right (in cht.sh)", "Navigate & switch Scope/Query"),
                    ("PageUp / PageDown", "Scroll buffer"),
                    ("Esc", "Close modal / cancel running task"),
                    ("Ctrl+Q / Ctrl+C", "Quit application"),
                ];
                for (i, (key, desc)) in help_entries.iter().enumerate() {
                    out.push((key.to_string(), desc.to_string(), i == self.modal_index));
                }
            }
            None => {}
        }
        out
    }

    /// Apply the current modal selection (Enter). Returns an optional event to dispatch.
    pub fn modal_apply(&mut self) -> Option<AppEvent> {
        match &self.modal {
            Some(Modal::ModelPicker) => {
                let items = self.filtered_models_with_provider();
                if let Some((p, m)) = items.get(self.modal_index).cloned() {
                    self.provider_name = p.clone();
                    if let Some(pos) = self.provider_list.iter().position(|prov| prov == &p) {
                        self.provider_index = pos;
                    }
                    self.model_name = m.clone();
                    self.settings.model = self.model_name.clone();
                }
                self.close_modal();
                Some(AppEvent::Tick)
            }
            Some(Modal::Settings) => {
                match settings_row_label(self.modal_index) {
                    Some("model") => {
                        // Jump to the model picker.
                        self.open_modal(Modal::ModelPicker);
                    }
                    Some("search-summarize") => {
                        self.settings.search_summarize = !self.settings.search_summarize;
                    }
                    _ => {}
                }
                Some(AppEvent::Tick)
            }
            Some(Modal::SearchQueryPicker(_)) => {
                self.close_modal();
                Some(AppEvent::Tick)
            }
            Some(Modal::Help) => {
                self.close_modal();
                Some(AppEvent::Tick)
            }
            None => None,
        }
    }
}

/// Editable rows in the settings window.
const SETTINGS_ROWS: [&str; 4] = [
    "model",
    "server-url",
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
    fn test_tab_switching() {
        let mut app = App::new();
        assert_eq!(app.active_buffer(), BufferId::Chat);
        app.next_buffer();
        assert_eq!(app.active_buffer(), BufferId::Search);
        app.next_buffer();
        assert_eq!(app.active_buffer(), BufferId::Chtsh);
        app.next_buffer();
        assert_eq!(app.active_buffer(), BufferId::Chat); // wraps
        app.prev_buffer();
        assert_eq!(app.active_buffer(), BufferId::Chtsh); // wraps back
    }

    #[test]
    fn mouse_scroll_only_moves_active_buffer_and_stays_positive() {
        let mut app = App::new();
        assert_eq!(app.active_buffer(), BufferId::Chat);
        app.chat.view.last_max_scroll.set(100);
        app.chat.view.auto_scroll = false;
        app.chat.view.scroll = 0;

        app.search.view.last_max_scroll.set(100);
        app.search.view.auto_scroll = false;
        app.search.view.scroll = 0;

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
        app.provider_models.insert("ollama".into(), vec!["a".into(), "b".into(), "c".into()]);
        app.models = vec!["a".into(), "b".into(), "c".into()];
        app.open_modal(Modal::ModelPicker);
        assert_eq!(app.modal, Some(Modal::ModelPicker));
        app.modal_move(false); // index 1
        app.modal_move(false); // index 2
        assert!(app.modal_apply().is_some());
        assert_eq!(app.model_name, "c");
        assert_eq!(app.settings.model, "c");
        assert_eq!(app.modal, None);
    }

    #[test]
    fn modal_move_clamps_and_close_resets() {
        let mut app = App::new();
        app.provider_models.insert("ollama".into(), vec!["x".into()]);
        app.models = vec!["x".into()];
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
    fn test_multi_provider_modal_switching() {
        let mut app = App::new();
        app.provider_list = vec!["ollama".into(), "groq".into(), "gemini".into(), "nvidia".into()];
        app.provider_index = 0;
        app.provider_name = "ollama".into();
        app.provider_models.insert("ollama".into(), vec!["qwen2.5-coder-1.5b:latest".into()]);
        app.provider_models.insert("groq".into(), vec!["llama-3.3-70b-versatile".into()]);

        app.open_modal(Modal::ModelPicker);
        assert_eq!(app.active_provider_tab(), "ollama");

        app.modal_next_provider();
        assert_eq!(app.active_provider_tab(), "groq");

        // Apply first model of groq
        app.modal_apply();
        assert_eq!(app.provider_name, "groq");
        assert_eq!(app.model_name, "llama-3.3-70b-versatile");
    }

    #[test]
    fn test_global_model_search() {
        let mut app = App::new();
        app.provider_models.insert("ollama".into(), vec!["qwen2.5-coder-7b:latest".into()]);
        app.provider_models.insert("nvidia".into(), vec!["nvidia/llama-3.1-nemotron-70b-instruct".into()]);

        app.modal_search = "nemotron".into();
        let matches = app.filtered_models_with_provider();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "nvidia");
        assert_eq!(matches[0].1, "nvidia/llama-3.1-nemotron-70b-instruct");
    }

    #[test]
    fn settings_toggle_flips_boolean_row() {
        let mut app = App::new();
        app.settings.search_summarize = false;
        app.open_modal(Modal::Settings);
        // navigate to the "search-summarize" row (index 3)
        app.modal_index = 3;
        app.modal_apply();
        assert_eq!(app.settings.search_summarize, true);
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

        pub fn move_word_backward(&mut self) {
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
            self.cursor = new_cursor;
            self.clamp_scroll();
        }

        pub fn move_word_forward(&mut self) {
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
            self.cursor = end;
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
