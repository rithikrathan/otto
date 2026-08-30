//! Event model: typed messages from background tasks, plus the input stream.
//!
//! The UI runs on the main thread; crossterm input and Tokio background tasks
//! all feed into a single channel drained by the main loop.

use crate::buffers::BufferId;

/// Every message the app consumes in its event loop.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Raw terminal/keyboard input.
    Input(crossterm::event::KeyEvent),
    /// Mouse wheel scroll delta (positive = down). Scrolls the active buffer.
    MouseScroll {
        delta: i32,
    },

    /// Periodic tick (drives the spinner animation).
    Tick,
    /// Abort running jobs
    Abort,

    /// Streaming chat token appended to a buffer.
    ChatDelta {
        buffer: BufferId,
        delta: String,
    },
    /// Assistant streaming finished.
    ChatDone {
        buffer: BufferId,
    },
    ChatError {
        buffer: BufferId,
        msg: String,
    },

    /// Token accounting updated mid-stream (reading + writing + context %).
    TokenStat {
        /// Tokens sent to the model (reading) so far in this conversation.
        prompt_tokens: u64,
        /// Tokens generated (writing) so far in this conversation.
        eval_tokens: u64,
    },

    /// Documentation search events.
    SearchResultsLoaded {
        query: String,
        results: Vec<crate::search::SearchResult>,
    },
    SearchDocumentLoaded {
        url: String,
        title: String,
        markdown: String,
        from_cache: bool,
    },
    SearchError {
        msg: String,
    },

    /// cht.sh URL plan produced by the model.
    ChtshPlan {
        topic: String,
        query: String,
    },
    ChtshDone {
        text: String,
    },
    ChtshError {
        msg: String,
    },
    ChtshRootLoaded(Vec<String>),
    ChtshTopicLoaded {
        lang: String,
        topics: Vec<String>,
    },

    /// Model list from `/api/tags` or cloud providers.
    ModelsLoaded(Vec<String>),
    ProviderModelsLoaded {
        provider: String,
        models: Vec<String>,
    },


    /// Add/remove a busy-job marker (drives the spinner).
    MarkBusy {
        job: crate::app::JobKind,
        on: bool,
    },
    /// Connection status to the server.
    ConnectionStatus(bool),
}

/// Channel type used for UI events.
pub type EventSender = tokio::sync::mpsc::UnboundedSender<AppEvent>;
pub type EventReceiver = tokio::sync::mpsc::UnboundedReceiver<AppEvent>;

/// Create the app event channel.
pub fn channel() -> (EventSender, EventReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}

/// Task-local alias so background workers can push typed results.
pub type JobTx = EventSender;
