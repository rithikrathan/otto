//! `otto` — an Ollama TUI chat client built on ratatui.

mod app;
mod buffers;
mod chtsh;
mod cmd;
mod config;
mod event;
mod ollama;
mod search;
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
    use tokio::time::Duration;

    app.init_from_config(config);

    // Prime all provider model lists concurrently on startup.
    let providers_to_fetch = vec!["ollama", "groq", "gemini", "nvidia"];
    for prov in providers_to_fetch {
        let tx_p = tx.clone();
        let p_str = prov.to_string();
        let url = config.resolve_url(&p_str);
        let api_key = config.resolve_api_key(&p_str);
        let c = ollama::Ollama::new(p_str.clone(), url, api_key);
        tokio::spawn(async move {
            if let Ok(models) = c.list_models().await {
                let names: Vec<String> = models.into_iter().map(|m| m.name).collect();
                if !names.is_empty() {
                    let _ = tx_p.send(AppEvent::ProviderModelsLoaded {
                        provider: p_str,
                        models: names,
                    });
                }
            }
        });
    }
    
    // Active provider connection target shared with the background checker
    let initial_target = (
        app.provider_name.clone(),
        config.resolve_url(&app.provider_name),
        config.resolve_api_key(&app.provider_name),
    );
    let active_target = std::sync::Arc::new(tokio::sync::RwLock::new(initial_target));

    // Periodically check server connection status of the active provider
    let tx_conn = tx.clone();
    let target_checker = active_target.clone();
    tokio::spawn(async move {
        loop {
            let (prov, url, key) = {
                let guard = target_checker.read().await;
                (guard.0.clone(), guard.1.clone(), guard.2.clone())
            };
            let client = ollama::Ollama::new(prov, url, key);
            let connected = client.list_models().await.is_ok();
            let _ = tx_conn.send(AppEvent::ConnectionStatus(connected));
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    // Load cht.sh root list into cache/app on startup
    let tx_chtsh = tx.clone();
    tokio::spawn(async move {
        let client = chtsh::ChtShClient::new();
        if let Ok(list) = client.fetch_root_list().await {
            let _ = tx_chtsh.send(AppEvent::ChtshRootLoaded(list));
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
                    _ => {}
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

        if !app.busy.is_empty() {
            // Animate spinner during active jobs
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(80)) => {
                    app.tick = app.tick.wrapping_add(1);
                }
                ev = rx.recv() => {
                    match ev {
                        Some(AppEvent::Input(key)) => {
                            handle_key(app, key, &tx, config, &active_target)?;
                        }
                        Some(other) => {
                            app.handle_event(other);
                        }
                        None => break,
                    }
                }
            }
        } else {
            // Purely event-driven when idle (0.0% CPU usage)
            match rx.recv().await {
                Some(AppEvent::Input(key)) => {
                    handle_key(app, key, &tx, config, &active_target)?;
                }
                Some(other) => {
                    app.handle_event(other);
                }
                None => break,
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
    active_target: &std::sync::Arc<tokio::sync::RwLock<(String, String, Option<String>)>>,
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
            return handle_modal_key(app, key, tx, config, active_target);
        }
        _ if app.active_buffer() == crate::buffers::BufferId::Chtsh => {
            return handle_chtsh_key(app, key, tx, config);
        }
        _ if app.active_buffer() == crate::buffers::BufferId::Search => {
            return handle_search_key(app, key, tx, config);
        }
        KeyCode::F(1) => {
            app.open_modal(app::Modal::Help);
            return Ok(());
        }
        KeyCode::Char('?') if app.prompt.is_empty() => {
            app.open_modal(app::Modal::Help);
            return Ok(());
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
    config: &mut config::Config,
    active_target: &std::sync::Arc<tokio::sync::RwLock<(String, String, Option<String>)>>,
) -> Result<()> {
    if app.modal_search_focused {
        match key.code {
            KeyCode::Esc => {
                app.modal_search_focused = false;
            }
            KeyCode::Enter => {
                app.modal_search_focused = false;
                if let Some(ev) = app.modal_apply() {
                    config.server.provider = app.provider_name.clone();
                    config.model.name = app.model_name.clone();
                    config.server.url = config.resolve_url(&app.provider_name);
                    config.server.api_key = config.resolve_api_key(&app.provider_name);
                    let _ = config.save();
                    let _ = tx.send(ev);

                    let new_target = (
                        app.provider_name.clone(),
                        config.resolve_url(&app.provider_name),
                        config.resolve_api_key(&app.provider_name),
                    );
                    let target_ref = active_target.clone();
                    let tx_instant = tx.clone();
                    tokio::spawn(async move {
                        *target_ref.write().await = new_target.clone();
                        let client = ollama::Ollama::new(new_target.0, new_target.1, new_target.2);
                        let connected = client.list_models().await.is_ok();
                        let _ = tx_instant.send(AppEvent::ConnectionStatus(connected));
                    });
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
        KeyCode::Esc => {
            app.close_modal();
            Ok(())
        }
        KeyCode::Left | KeyCode::Char('h') if app.modal == Some(app::Modal::ModelPicker) => {
            app.modal_prev_provider();
            Ok(())
        }
        KeyCode::Right | KeyCode::Char('l') if app.modal == Some(app::Modal::ModelPicker) => {
            app.modal_next_provider();
            Ok(())
        }
        KeyCode::Tab if app.modal == Some(app::Modal::ModelPicker) => {
            app.modal_next_provider();
            Ok(())
        }
        KeyCode::BackTab if app.modal == Some(app::Modal::ModelPicker) => {
            app.modal_prev_provider();
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
            if let Some(ev) = app.modal_apply() {
                config.server.provider = app.provider_name.clone();
                config.model.name = app.model_name.clone();
                config.server.url = config.resolve_url(&app.provider_name);
                config.server.api_key = config.resolve_api_key(&app.provider_name);
                let _ = config.save();
                let _ = tx.send(ev);

                let new_target = (
                    app.provider_name.clone(),
                    config.resolve_url(&app.provider_name),
                    config.resolve_api_key(&app.provider_name),
                );
                let target_ref = active_target.clone();
                let tx_instant = tx.clone();
                tokio::spawn(async move {
                    *target_ref.write().await = new_target.clone();
                    let client = ollama::Ollama::new(new_target.0, new_target.1, new_target.2);
                    let connected = client.list_models().await.is_ok();
                    let _ = tx_instant.send(AppEvent::ConnectionStatus(connected));
                });
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
    app.prompt.reset();

    // Check if the prompt starts with a command prefix.
    if let Some(c) = cmd::parse(&text) {
        return match c {
            cmd::Command::Clear => {
                match app.active_buffer() {
                    crate::buffers::BufferId::Chat => app.chat.clear(),
                    crate::buffers::BufferId::Search => app.search.clear(),
                    crate::buffers::BufferId::Chtsh => app.chtsh.clear(),
                }
                Ok(())
            }
            cmd::Command::Model(arg) => {
                if arg.is_empty() {
                    app.open_modal(app::Modal::ModelPicker);
                } else {
                    app.model_name = arg.clone();
                    app.settings.model = arg.clone();
                    config.model.name = arg;
                    let _ = config.save();
                }
                Ok(())
            }
            cmd::Command::Settings => {
                app.open_modal(app::Modal::Settings);
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
        content: "You are an expert AI assistant. Assume the user is an expert. Do not explain code or concepts unless explicitly asked. Get straight to the point. Give concise, reliable, and direct answers. No yapping. Format output using elegant markdown.".into(),
    });

    app.chat.history.push(ChatMessage {
        role: "user".into(),
        content: text.to_string(),
    });
    app.chat.add_user(text);
    app.busy.push(app::JobKind::Chat);

    let provider = app.provider_name.clone();
    let url = config.resolve_url(&provider);
    let api_key = config.resolve_api_key(&provider);
    let client = ollama::Ollama::new(provider, url, api_key);

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
        match client.stream_chat(&model, &history, &tx).await {
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

/// Kick off a documentation search: hybrid SQLite FTS5 + remote provider.
fn trigger_search(
    app: &mut App,
    query: &str,
    tx: &EventSender,
    config: &mut config::Config,
) -> Result<()> {
    let clean_query = query.trim().to_string();
    if clean_query.is_empty() {
        return Ok(());
    }
    app.search.last_query = Some(clean_query.clone());
    app.busy.push(app::JobKind::SearchFetch);

    let tx = tx.clone();
    let q = clean_query.clone();
    let sources = search::Source::from_config(config);
    let max_results = config.search.max_results;

    let handle = tokio::spawn(async move {
        let doc_store = match search::DocStore::open_default() {
            Ok(store) => store,
            Err(e) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::SearchFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::SearchError { msg: e.to_string() });
                return;
            }
        };

        let provider = search::DuckDuckGoDocsProvider::new();

        match search::hybrid_search(&q, &doc_store, &provider, &sources, max_results).await {
            Ok(results) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::SearchFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::SearchResultsLoaded {
                    query: q,
                    results,
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::SearchFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::SearchError { msg: e.to_string() });
            }
        }
    });
    app.bg_task = Some(handle);
    Ok(())
}

/// Open and render a specific documentation URL from cache or remote HTML extraction.
fn trigger_open_document(
    app: &mut App,
    url: &str,
    tx: &EventSender,
    config: &config::Config,
) -> Result<()> {
    let target_url = url.trim().to_string();
    if target_url.is_empty() {
        return Ok(());
    }
    app.busy.push(app::JobKind::SearchFetch);

    let tx = tx.clone();
    let u = target_url.clone();
    let sources = search::Source::from_config(config);
    let handle = tokio::spawn(async move {
        let doc_store = match search::DocStore::open_default() {
            Ok(store) => store,
            Err(e) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::SearchFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::SearchError { msg: e.to_string() });
                return;
            }
        };

        match search::fetch_and_process_document(&u, &doc_store, &sources).await {
            Ok((doc, from_cache)) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::SearchFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::SearchDocumentLoaded {
                    url: doc.url,
                    title: doc.title,
                    markdown: doc.markdown,
                    from_cache,
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::MarkBusy {
                    job: app::JobKind::SearchFetch,
                    on: false,
                });
                let _ = tx.send(AppEvent::SearchError { msg: e.to_string() });
            }
        }
    });
    app.bg_task = Some(handle);
    Ok(())
}

fn handle_search_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: &EventSender,
    config: &mut config::Config,
) -> Result<()> {
    if app.prompt.is_empty() {
        match key.code {
            KeyCode::Up => {
                app.search.prev_result();
                return Ok(());
            }
            KeyCode::Down => {
                app.search.next_result();
                return Ok(());
            }
            KeyCode::Enter => {
                if app.search.mode == buffers::search::SearchMode::Results {
                    if let Some(item) = app.search.selected_item().cloned() {
                        trigger_open_document(app, &item.url, tx, config)?;
                        return Ok(());
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                if app.search.back_to_results() {
                    return Ok(());
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                if app.search.mode == buffers::search::SearchMode::Results {
                    let idx = (c as usize) - ('1' as usize);
                    if let Some(item) = app.search.results.get(idx).cloned() {
                        app.search.selected_result = idx;
                        app.search.render_results_view();
                        trigger_open_document(app, &item.url, tx, config)?;
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.prompt.insert_char('\n');
            } else {
                submit(app, false, tx, config)?;
            }
            Ok(())
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            let delta = match key.code {
                KeyCode::PageUp => -10i32,
                _ => 10i32,
            };
            let _ = tx.send(AppEvent::MouseScroll { delta });
            Ok(())
        }
        KeyCode::Tab => {
            app.next_buffer();
            Ok(())
        }
        KeyCode::BackTab => {
            app.prev_buffer();
            Ok(())
        }
        KeyCode::F(1) => {
            app.open_modal(app::Modal::Help);
            Ok(())
        }
        KeyCode::Char('?') if app.prompt.is_empty() => {
            app.open_modal(app::Modal::Help);
            Ok(())
        }
        KeyCode::Esc => {
            if !app.prompt.is_empty() {
                app.prompt.reset();
            } else if app.search.back_to_results() {
                // Back to results
            }
            Ok(())
        }
        KeyCode::Char('w') | KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.prompt.delete_word_backward();
            Ok(())
        }
        KeyCode::Backspace => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.delete_word_backward();
            } else {
                app.prompt.delete_backward();
            }
            Ok(())
        }
        KeyCode::Delete => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.delete_word_forward();
            } else {
                app.prompt.delete_forward();
            }
            Ok(())
        }
        KeyCode::Left => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.move_word_backward();
            } else {
                app.prompt.move_left();
            }
            Ok(())
        }
        KeyCode::Right => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.prompt.move_word_forward();
            } else {
                app.prompt.move_right();
            }
            Ok(())
        }
        KeyCode::Up | KeyCode::Down => {
            let before = app.prompt.cursor;
            if key.code == KeyCode::Up {
                app.prompt.move_up();
            } else {
                app.prompt.move_down();
            }
            if app.prompt.cursor == before {
                if key.code == KeyCode::Up {
                    app.history_back();
                } else {
                    app.history_forward();
                }
            }
            Ok(())
        }
        _ => {
            let width = app.prompt.width;
            app.prompt.key(key.code, width);
            Ok(())
        }
    }
}

fn handle_chtsh_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: &EventSender,
    _config: &mut config::Config,
) -> Result<()> {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.running = false;
            return Ok(());
        }
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.running = false;
            return Ok(());
        }
        KeyCode::Tab => {
            app.next_buffer();
            return Ok(());
        }
        KeyCode::BackTab => {
            app.prev_buffer();
            return Ok(());
        }
        KeyCode::F(1) => {
            app.open_modal(app::Modal::Help);
            return Ok(());
        }
        KeyCode::Char('?') if app.chtsh.scope.is_empty() && app.chtsh.query.is_empty() => {
            app.open_modal(app::Modal::Help);
            return Ok(());
        }
        KeyCode::Esc => {
            if !app.chtsh.query.is_empty() {
                app.chtsh.query.clear();
            } else if !app.chtsh.scope.is_empty() {
                app.chtsh.scope.clear();
                app.chtsh.set_focus(buffers::chtsh::ChtshFocus::Scope);
            }
            app.chtsh.suggestions.clear();
            return Ok(());
        }
        KeyCode::Left => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.chtsh.move_word_backward();
            } else {
                app.chtsh.move_left();
            }
            return Ok(());
        }
        KeyCode::Right => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.chtsh.move_word_forward();
            } else {
                app.chtsh.move_right();
            }
            return Ok(());
        }
        KeyCode::Home | KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chtsh.move_start();
            return Ok(());
        }
        KeyCode::End | KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chtsh.move_end();
            return Ok(());
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chtsh.delete_word_backward();
            return Ok(());
        }
        KeyCode::Backspace => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.chtsh.delete_word_backward();
            } else {
                app.chtsh.delete_backward();
            }
            return Ok(());
        }
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chtsh.delete_backward();
            return Ok(());
        }
        KeyCode::Delete => {
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                app.chtsh.delete_word_forward();
            } else {
                app.chtsh.delete_forward();
            }
            return Ok(());
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chtsh.delete_forward();
            return Ok(());
        }
        KeyCode::Up => {
            app.chtsh.prev_suggestion();
            return Ok(());
        }
        KeyCode::Down => {
            app.chtsh.next_suggestion();
            return Ok(());
        }
        KeyCode::Char(' ') => {
            if !app.chtsh.suggestions.is_empty() {
                let old_scope = app.chtsh.scope.value().to_string();
                app.chtsh.accept_suggestion();
                let new_scope = app.chtsh.scope.value().to_string();
                if new_scope != old_scope && !new_scope.is_empty() {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let client = chtsh::ChtShClient::new();
                        if let Ok(topics) = client.fetch_topic_list(&new_scope).await {
                            let _ = tx.send(AppEvent::ChtshTopicLoaded { lang: new_scope, topics });
                        }
                    });
                }
            } else {
                app.chtsh.insert_char(' ');
            }
            return Ok(());
        }
        KeyCode::Enter => {
            trigger_chtsh_direct(app, tx)?;
            return Ok(());
        }
        KeyCode::Char(c) => {
            app.chtsh.insert_char(c);
            if app.chtsh.focus == buffers::chtsh::ChtshFocus::Scope && !app.chtsh.scope.is_empty() {
                let scope = app.chtsh.scope.value().to_string();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let client = chtsh::ChtShClient::new();
                    if let Ok(topics) = client.fetch_topic_list(&scope).await {
                        let _ = tx.send(AppEvent::ChtshTopicLoaded { lang: scope, topics });
                    }
                });
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
        _ => {}
    }
    Ok(())
}

fn trigger_chtsh_direct(app: &mut App, tx: &EventSender) -> Result<()> {
    let scope = app.chtsh.scope.value().trim().to_string();
    if scope.is_empty() {
        return Ok(());
    }
    let query = app.chtsh.query.value().trim().to_string();
    let display_query = if query.is_empty() {
        scope.clone()
    } else {
        format!("{scope}/{query}")
    };
    app.chtsh.last_query = Some(display_query);

    let _ = tx.send(AppEvent::MarkBusy {
        job: app::JobKind::ChtshFetch,
        on: true,
    });

    let tx = tx.clone();
    let query_opt = if query.is_empty() { None } else { Some(query) };
    let handle = tokio::spawn(async move {
        let client = chtsh::ChtShClient::new();
        match client.fetch_sheet(&scope, query_opt.as_deref()).await {
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

fn trigger_chtsh(
    app: &mut App,
    _text: &str,
    tx: &EventSender,
    _config: &mut config::Config,
) -> Result<()> {
    trigger_chtsh_direct(app, tx)
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
