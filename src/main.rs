//! `otto` — an Ollama TUI chat client built on ratatui.

mod app;
mod buffers;
mod chtsh;
mod cmd;
mod config;
mod event;
mod ollama;
mod search;
mod stt;
mod ui;

use std::io;

use anyhow::{Context, Result};
use app::App;
use crossterm::event::{KeyCode, KeyModifiers};
use event::{channel, AppEvent, EventSender};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Terminal setup: enter raw mode + alternate screen, run the app, restore.
fn main() -> Result<()> {
    let mut config = config::Config::load()?;
    let (tx, rx) = channel();

    let mut app = App::new();
    app.model_name = config.model.name.clone();
    app.settings.model = config.model.name.clone();
    app.settings.server_url = config.server.url.clone();
    app.settings.stt_enabled = config.stt.enabled;
    app.settings.stt_model_path = config.stt.model_path.clone();
    app.settings.search_provider = config.search.provider.clone();
    app.settings.search_summarize = config.search.summarize;

    crossterm::terminal::enable_raw_mode().context("enable raw mode")?;
    let mut term = Terminal::new(CrosstermBackend::new(io::stdout())).context("create terminal")?;
    crossterm::execute!(
        io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide,
        crossterm::event::EnableMouseCapture
    )?;


    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime")
        .block_on(run(&mut term, &mut app, rx, &mut config, tx.clone()));

    crossterm::execute!(
        io::stdout(),
        crossterm::cursor::Show,
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;

    crossterm::terminal::disable_raw_mode().ok();

    result
}

/// The main event loop.
async fn run(
    term: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    mut rx: event::EventReceiver,
    config: &mut config::Config,
    tx: EventSender,
) -> Result<()> {
    use tokio::time::{interval, Duration};

    let mut tick = interval(Duration::from_millis(80));

    // Prime the model list on startup (background).
    let ollama = ollama::Ollama::new(config.server.url.clone());
    let tx2 = tx.clone();
    tokio::spawn(async move {
        match ollama.list_models().await {
            Ok(models) => {
                let _ = tx2.send(AppEvent::ModelsLoaded(
                    models.into_iter().map(|m| m.name).collect(),
                ));
            }
            Err(e) => {
                let _ = tx2.send(AppEvent::ModelsLoaded(Vec::new()));
                let _ = tx2.send(AppEvent::ChatError {
                    buffer: crate::buffers::BufferId::Chat,
                    msg: format!("ollama: {e}"),
                });
            }
        }
    });

    // Input reader streams key + mouse events into the same channel.
    let tx3 = tx.clone();
    tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut events = crossterm::event::EventStream::new();
        while let Some(Ok(ev)) = events.next().await {
            match ev {
                crossterm::event::Event::Key(key) => {
                    let _ = tx3.send(AppEvent::Input(key));
                }
                crossterm::event::Event::Mouse(m) => match m.kind {
                    crossterm::event::MouseEventKind::ScrollUp => {
                        let _ = tx3.send(AppEvent::MouseScroll { delta: -3 });
                    }
                    crossterm::event::MouseEventKind::ScrollDown => {
                        let _ = tx3.send(AppEvent::MouseScroll { delta: 3 });
                    }
                    _ => {
                        let _ = tx3.send(AppEvent::Mouse(m));
                    }
                },
                _ => {}
            }
        }
    });

    loop {
        if !app.running {
            break;
        }
        term.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            _ = tick.tick() => {
                app.tick = app.tick.wrapping_add(1);
            }
            ev = rx.recv() => {
                match ev {
                    Some(AppEvent::Input(key)) => {
                        handle_key(app, key, &tx, config)?;
                    }
                    Some(AppEvent::Mouse(m)) => {
                        use crossterm::event::{MouseEventKind, MouseButton};
                        match m.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                app.selection_start = Some((m.column, m.row));
                                app.selection_end = Some((m.column, m.row));
                            }
                            MouseEventKind::Drag(MouseButton::Left) => {
                                if app.selection_start.is_some() {
                                    app.selection_end = Some((m.column, m.row));
                                }
                            }
                            MouseEventKind::Up(MouseButton::Left) => {
                                if let (Some(s), Some(_)) = (app.selection_start, app.selection_end) {
                                    let mut start = s;
                                    let mut end = (m.column, m.row);
                                    if start.1 > end.1 || (start.1 == end.1 && start.0 > end.0) {
                                        std::mem::swap(&mut start, &mut end);
                                    }
                                    
                                    let buf = term.current_buffer_mut();
                                    let area = buf.area;
                                    let mut copied = String::new();
                                    
                                    for y in start.1..=end.1 {
                                        if y >= area.height { break; }
                                        let x_start = if y == start.1 { start.0 } else { 0 };
                                        let x_end = if y == end.1 { end.0 } else { area.width.saturating_sub(1) };
                                        
                                        let mut line_str = String::new();
                                        for x in x_start..=x_end {
                                            if x >= area.width { break; }
                                            let pos = ratatui::layout::Position { x, y };
                                            line_str.push_str(buf.cell(pos).map(|c| c.symbol()).unwrap_or(" "));
                                        }
                                        copied.push_str(line_str.trim_end());
                                        if y < end.1 {
                                            copied.push('\n');
                                        }
                                    }
                                    
                                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                        let _ = clipboard.set_text(copied.clone());
                                    }
                                    
                                    // Fallback: emit OSC 52 for SSH / tmux multiplexers
                                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                                    let encoded = STANDARD.encode(&copied);
                                    let osc52 = format!("\x1b]52;c;{}\x07", encoded);
                                    let _ = std::io::Write::write_all(&mut std::io::stdout(), osc52.as_bytes());
                                    let _ = std::io::Write::flush(&mut std::io::stdout());
                                }
                                app.selection_start = None;
                                app.selection_end = None;
                            }
                            _ => {}
                        }
                    }
                    Some(other) => {
                        if let AppEvent::SearchExecute { query } = &other {
                            execute_search(app, query, &tx, config);
                        }
                        app.handle_event(other);
                    }
                    None => break,
                }
            }
        }
    }
    Ok(())
}

/// Dispatch a keyboard event.
fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: &EventSender,
    config: &mut config::Config,
) -> Result<()> {
    if key.kind != crossterm::event::KeyEventKind::Press {
        return Ok(());
    }
    if key.code != KeyCode::Esc {
        app.pending_abort = false;
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+C quits.
            app.running = false;
            return Ok(());
        }
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+Q quits.
            app.running = false;
            return Ok(());
        }
        // A floating window has focus: route keys to it.
        _ if app.modal.is_some() => {
            return handle_modal_key(app, key, tx, config);
        }
        KeyCode::Esc => {
            if !app.prompt.is_empty() {
                app.prompt.reset();
                app.pending_abort = false;
            } else if !app.busy.is_empty() {
                if app.pending_abort {
                    let _ = tx.send(AppEvent::Abort);
                    app.pending_abort = false;
                } else {
                    app.pending_abort = true;
                }
            } else {
                app.pending_abort = false;
            }
            return Ok(());
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+K clears the context.
            app.chat.clear();
            app.tokens.reset();
            return Ok(());
        }
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+M toggles speech-to-text (requires the `stt` feature + libvosk).
            if !stt::ENABLED {
                app.chat.view.blocks.push(crate::buffers::Block {
                    kind: "info".to_string(),
                    markdown: "STT not compiled (build with `--features stt` and install libvosk)."
                        .into(),
                });
                return Ok(());
            }
            if app.busy.contains(&app::JobKind::Stt) {
                app.remove_job(app::JobKind::Stt);
            } else {
                app.busy.push(app::JobKind::Stt);
                let tx = tx.clone();
                let path = config.stt.model_path.clone();
                tokio::spawn(async move {
                    stt::start_recording(&path, &tx).await;
                });
            }
            return Ok(());
        }
        KeyCode::Tab => {
            if let Some(comp) = crate::cmd::autocomplete(&app.prompt.text) {
                if app.prompt.cursor == app.prompt.text.len() {
                    let comp_str = comp.to_string();
                    for c in comp_str.chars() {
                        app.prompt.insert_char(c);
                    }
                    return Ok(());
                }
            }
            app.next_buffer();
            return Ok(());
        }
        KeyCode::Right => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.move_word_forward();
                return Ok(());
            }
            if let Some(comp) = crate::cmd::autocomplete(&app.prompt.text) {
                if app.prompt.cursor == app.prompt.text.len() {
                    let comp_str = comp.to_string();
                    for c in comp_str.chars() {
                        app.prompt.insert_char(c);
                    }
                    return Ok(());
                }
            }
            app.prompt.move_right();
            return Ok(());
        }
        KeyCode::BackTab => {
            app.prev_buffer();
            return Ok(());
        }
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.prompt.insert_char('\n');
            } else {
                submit(app, false, tx, config)?;
            }
            return Ok(());
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            let delta = match key.code {
                KeyCode::PageUp => -10i32,
                _ => 10i32,
            };
            let _ = tx.send(AppEvent::MouseScroll { delta });
            return Ok(());
        }
        KeyCode::Up | KeyCode::Down => {
            // Prompt: move the caret up/down a line; at the top/bottom edge,
            // navigate history (Up = previous, Down = next).
            let before = app.prompt.cursor;
            if key.code == KeyCode::Up {
                app.prompt.move_up();
            } else {
                app.prompt.move_down();
            }
            if app.prompt.cursor == before {
                // Caret couldn't move (edge of the prompt): browse history.
                if key.code == KeyCode::Up {
                    app.history_back();
                } else {
                    app.history_forward();
                }
            }
            return Ok(());
        }
        KeyCode::Char('w') | KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.prompt.delete_word_backward();
            return Ok(());
        }
        KeyCode::Backspace => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.delete_word_backward();
            } else {
                app.prompt.delete_backward();
            }
            return Ok(());
        }
        KeyCode::Delete => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.delete_word_forward();
            } else {
                app.prompt.delete_forward();
            }
            return Ok(());
        }
        KeyCode::Left => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.move_word_backward();
                return Ok(());
            }
        }
        _ => {}
    }

    // Default: edit the prompt box at the last known width.
    let width = app.prompt.width;
    app.prompt.key(key.code, width);
    Ok(())
}

/// Handle keys while a floating window (modal) has focus.
fn handle_modal_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: &EventSender,
    _config: &mut config::Config,
) -> Result<()> {
    if app.modal_search_focused {
        match key.code {
            KeyCode::Esc => {
                app.modal_search_focused = false;
            }
            KeyCode::Enter => {
                app.modal_search_focused = false;
                if let Some(ev) = app.modal_apply() {
                    let _ = tx.send(ev);
                }
            }
            KeyCode::Backspace => {
                app.modal_search.pop();
                app.modal_index = 0;
            }
            KeyCode::Char(c) => {
                app.modal_search.push(c);
                app.modal_index = 0;
            }
            KeyCode::Up => {
                app.modal_move(true);
            }
            KeyCode::Down => {
                app.modal_move(false);
            }
            _ => {}
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
            app.close_modal();
            Ok(())
        }
        KeyCode::Char('.') if app.modal == Some(app::Modal::ModelPicker) => {
            app.modal_search_focused = true;
            Ok(())
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.modal_move(true);
            Ok(())
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.modal_move(false);
            Ok(())
        }
        KeyCode::Enter => {
            // Applying an async job (e.g. switching models) is sync here.
            if let Some(ev) = app.modal_apply() {
                let _ = tx.send(ev);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Handle a submitted prompt: dispatch commands or the active buffer's action.
fn submit(app: &mut App, shift: bool, tx: &EventSender, config: &mut config::Config) -> Result<()> {
    // Alt+Enter (shift+enter here approximated) inserts a newline instead.
    if shift {
        app.prompt.insert_char('\n');
        return Ok(());
    }

    let text = app.prompt.value().to_string();
    if text.trim().is_empty() {
        return Ok(());
    }
    app.history_push(&text);

    if let Some(cmd) = cmd::parse(&text) {
        return match cmd {
            cmd::Command::Clear => {
                app.chat.clear();
                app.tokens.reset();
                Ok(())
            }
            cmd::Command::Model(m) if !m.is_empty() => {
                app.model_name = m;
                Ok(())
            }
            cmd::Command::Model(_) => {
                // `/model` with no arg: open the floating model picker.
                app.open_modal(app::Modal::ModelPicker);
                Ok(())
            }
            cmd::Command::Settings => {
                app.open_modal(app::Modal::Settings);
                Ok(())
            }
            cmd::Command::Help => {
                let help_text = "### Available Commands\n\n* `/clear`: Clear chat history\n* `/model [name]`: Change model (empty to open picker)\n* `/settings`: Open settings\n* `/export <path>`: Export chat to file\n* `/google`, `/bing`, `/ddg`: Change search provider\n* `/help`: Show this help\n* `/exit`, `:q`: Quit\n\n_Tip: Use Tab or Right Arrow to autocomplete slash commands._";
                app.chat.view.blocks.push(crate::buffers::Block {
                    kind: "markdown".to_string(),
                    markdown: help_text.into(),
                });
                app.chat.view.scroll = 9999;
                Ok(())
            }
            cmd::Command::SearchProvider(provider) => {
                app.settings.search_provider = provider;
                Ok(())
            }
            cmd::Command::Quit => {
                app.running = false;
                Ok(())
            }
            cmd::Command::Endpoint(url) if !url.is_empty() => {
                app.settings.server_url = url.clone();
                config.server.url = url;
                let _ = config.save();
                app.chat.view.blocks.push(crate::buffers::Block {
                    kind: "markdown".to_string(),
                    markdown: format!("**System:** Endpoint updated to `{}`", config.server.url),
                });
                app.chat.view.scroll = 9999;
                Ok(())
            }
            cmd::Command::Export(path) if !path.is_empty() => export_chat(app, &path),
            _ => Ok(()),
        };
    }

    match app.active_buffer() {
        crate::buffers::BufferId::Chat => run_chat(app, &text, tx, config),
        crate::buffers::BufferId::Search => trigger_search(app, &text, tx, config),
        crate::buffers::BufferId::Chtsh => trigger_chtsh(app, &text, tx, config),
    }
}

/// Send the prompt to the selected model and stream the reply.
fn run_chat(app: &mut App, text: &str, tx: &EventSender, config: &mut config::Config) -> Result<()> {
    use crate::buffers::chat::ChatMessage;

    // Clear history to start fresh as requested, and add a system prompt.
    app.chat.history.clear();
    app.chat.history.push(ChatMessage {
        role: "system".into(),
        content: "You are a highly capable AI assistant. Answer the user's questions clearly and concisely. Format your output using elegant markdown, especially for code blocks.".into(),
    });

    app.chat.history.push(ChatMessage {
        role: "user".into(),
        content: text.to_string(),
    });
    app.chat.add_user(text);
    app.busy.push(app::JobKind::Chat);

    let ollama = ollama::Ollama::new(config.server.url.clone());
    let model = app.model_name.clone();
    let history: Vec<ollama::ChatMessage> = app
        .chat
        .history
        .iter()
        .map(|m| ollama::ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();
    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        match ollama.stream_chat(&model, &history, &tx).await {
            Ok(()) => {
                let _ = tx.send(AppEvent::ChatDone {
                    buffer: crate::buffers::BufferId::Chat,
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::ChatError {
                    buffer: crate::buffers::BufferId::Chat,
                    msg: e.to_string(),
                });
            }
        }
    });
    app.bg_task = Some(handle);
    Ok(())
}

/// Kick off a search: ask the model for a query plan, and show a picker modal.
fn trigger_search(
    app: &mut App,
    text: &str,
    tx: &EventSender,
    config: &mut config::Config,
) -> Result<()> {
    app.search.last_query = Some(text.to_string());
    app.open_modal(app::Modal::SearchQueryPicker(vec![text.to_string()]));
    app.busy.push(app::JobKind::SearchPlan);

    let ollama = ollama::Ollama::new(config.server.url.clone());
    let model = app.model_name.clone();
    let prompt = text.to_string();
    let tx = tx.clone();
    
    let handle = tokio::spawn(async move {
        // Phase 1: model produces the actual search query.
        let plan = plan_search(&ollama, &model, &prompt).await;
        match plan {
            Ok(plan_val) => {
                let mut query = plan_val
                    .get("query")
                    .and_then(|q| q.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| prompt.clone());
                if query == prompt {
                    query = format!("{} (AI)", query);
                } else {
                    query = format!("✨ {}", query);
                }
                let _ = tx.send(AppEvent::SearchRefinedQuery { query });
            }
            Err(e) => {
                let query = format!("{} (AI Failed: {})", prompt, e);
                let _ = tx.send(AppEvent::SearchRefinedQuery { query });
            }
        }
        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::SearchPlan,
            on: false,
        });
    });
    app.bg_task = Some(handle);
    Ok(())
}

fn execute_search(
    app: &mut App,
    query: &str,
    tx: &EventSender,
    config: &mut config::Config,
) {
    app.busy.push(app::JobKind::SearchFetch);
    let ollama = ollama::Ollama::new(config.server.url.clone());
    let model = app.model_name.clone();
    let summarize = config.search.summarize;
    let provider = app.settings.search_provider.clone();
    let tx = tx.clone();
    let query_str = query.to_string();

    let handle = tokio::spawn(async move {
        // Phase 2: run the provider query.
        let results = search::search(&provider, &query_str).await;
        let (results, err) = match results {
            Ok(r) => (r, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };

        // Phase 3: summarize (optional) into concise markdown.
        let mut md = search::results_to_markdown(&results);
        if summarize && !results.is_empty() {
            if let Ok(sum) = summarize_results(&ollama, &model, &query_str, &md).await {
                md = sum;
            }
        }

        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::SearchFetch,
            on: false,
        });
        if let Some(err) = err {
            let _ = tx.send(AppEvent::SearchError { msg: err });
        } else {
            let final_md = format!("**Search:** `{query_str}` (Provider: {provider})\n\n{md}");
            let _ = tx.send(AppEvent::SearchDone { markdown: final_md });
        }
    });
    app.bg_task = Some(handle);
}

/// Kick off a cht.sh query: ask the model for the URL, then fetch.
fn trigger_chtsh(
    app: &mut App,
    text: &str,
    tx: &EventSender,
    config: &mut config::Config,
) -> Result<()> {
    app.chtsh.last_query = Some(text.to_string());
    app.busy.push(app::JobKind::ChtshPlan);

    let ollama = ollama::Ollama::new(config.server.url.clone());
    let model = app.model_name.clone();
    let prompt = text.to_string();
    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        let plan = plan_chtsh(&ollama, &model, &prompt).await;
        let planval = match plan {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::ChtshPlan,
                    on: false,
                });
                let _ = tx.send(AppEvent::ChtshError { msg: e.to_string() });
                return;
            }
        };
        let chtsh_plan = chtsh::ChtshPlan {
            topic: planval
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            query: planval
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or(&prompt)
                .to_string(),
        };
        let url = chtsh::build_url(&chtsh_plan);
        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::ChtshPlan,
            on: false,
        });
        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::ChtshFetch,
            on: true,
        });
        match chtsh::fetch(&url).await {
            Ok(text) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::ChtshFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::ChtshDone { text });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::ChtshFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::ChtshError { msg: e.to_string() });
            }
        }
    });
    app.bg_task = Some(handle);
    Ok(())
}

fn clean_json(text: &str) -> String {
    let mut s = text.trim();
    if s.starts_with("```json") {
        s = &s[7..];
    } else if s.starts_with("```") {
        s = &s[3..];
    }
    if s.ends_with("```") {
        s = &s[..s.len() - 3];
    }
    s.trim().to_string()
}

async fn plan_search(
    ollama: &ollama::Ollama,
    model: &str,
    prompt: &str,
) -> Result<serde_json::Value> {
    let sys = "You are a search query refinement engine. Convert the user's intent into a highly optimized search query string. Do not answer the question; only produce the search query. Output JSON ONLY: {\"query\":\"...\"}".into();
    let resp = ollama
        .complete(
            model,
            vec![
                ollama::ChatMessage {
                    role: "system".into(),
                    content: sys,
                },
                ollama::ChatMessage {
                    role: "user".into(),
                    content: prompt.to_string(),
                },
            ],
        )
        .await?;
    let cleaned = clean_json(&resp.message.map(|m| m.content).unwrap_or_default());
    let json: serde_json::Value = serde_json::from_str(&cleaned)?;
    Ok(json)
}

fn extract_json(s: &str) -> &str {
    let s = s.trim();
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            return &s[start..=end];
        }
    }
    s
}

async fn plan_chtsh(
    ollama: &ollama::Ollama,
    model: &str,
    prompt: &str,
) -> Result<serde_json::Value> {
    let sys = "You are a cht.sh query engine. Parse the user's intent into a `topic` (the language or tool, e.g. 'rust', 'python', 'docker') and a `query` (the question or command). Output JSON ONLY: {\"topic\":\"...\", \"query\":\"...\"}".into();
    let resp = ollama
        .complete(
            model,
            vec![
                ollama::ChatMessage {
                    role: "system".into(),
                    content: sys,
                },
                ollama::ChatMessage {
                    role: "user".into(),
                    content: prompt.to_string(),
                },
            ],
        )
        .await?;
    let cleaned = clean_json(&resp.message.map(|m| m.content).unwrap_or_default());
    let json: serde_json::Value = serde_json::from_str(&cleaned)?;
    Ok(json)
}

/// Summarize a raw result list into a concise, procedural markdown answer.
async fn summarize_results(
    ollama: &ollama::Ollama,
    model: &str,
    query: &str,
    raw: &str,
) -> Result<String> {
    let sys = "You summarize web search results into a concise procedural markdown \
help. Answer the user's question only using the supplied results. Cite sources as \
markdown links. Be terse and structured."
        .into();
    let resp = ollama
        .complete(
            model,
            vec![
                ollama::ChatMessage {
                    role: "system".into(),
                    content: sys,
                },
                ollama::ChatMessage {
                    role: "user".into(),
                    content: format!("Search: {query}\n\nResults:\n{raw}"),
                },
            ],
        )
        .await?;
    Ok(resp
        .message
        .map(|m| m.content)
        .unwrap_or_else(|| raw.to_string()))
}

/// Export the current chat conversation as markdown.
fn export_chat(app: &App, path: &str) -> Result<()> {
    let mut md = String::new();
    for msg in &app.chat.history {
        md.push_str(&format!("**{}:**\n\n{}\n\n---\n\n", msg.role, msg.content));
    }
    std::fs::write(path, md).with_context(|| format!("write export to {path}"))?;
    Ok(())
}
