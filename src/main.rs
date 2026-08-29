//! `otc` — an Ollama TUI chat client built on ratatui.

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
use event::{AppEvent, EventSender, channel};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Terminal setup: enter raw mode + alternate screen, run the app, restore.
fn main() -> Result<()> {
    let config = config::Config::load()?;
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
    crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    crossterm::execute!(io::stdout(), crossterm::cursor::Hide)?;
    crossterm::execute!(io::stdout(), crossterm::event::EnableMouseCapture)?;

    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime")
        .block_on(run(&mut term, &mut app, rx, &config, tx.clone()));

    crossterm::execute!(io::stdout(), crossterm::cursor::Show)?;
    crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    crossterm::execute!(io::stdout(), crossterm::event::DisableMouseCapture)?;
    crossterm::terminal::disable_raw_mode().ok();

    result
}

/// The main event loop.
async fn run(
    term: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    mut rx: event::EventReceiver,
    config: &config::Config,
    tx: EventSender,
) -> Result<()> {
    use tokio::time::{Duration, interval};

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

        tokio::select! {
            _ = tick.tick() => {
                app.tick = app.tick.wrapping_add(1);
            }
            ev = rx.recv() => {
                match ev {
                    Some(AppEvent::Input(key)) => {
                        handle_key(app, key, &tx, config)?;
                    }
                    Some(other) => app.handle_event(other),
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
    config: &config::Config,
) -> Result<()> {
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
            // Esc clears the prompt (and can cancel focus).
            if !app.prompt.is_empty() {
                app.prompt.reset();
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
                    kind: "info",
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
            app.next_buffer();
            return Ok(());
        }
        KeyCode::BackTab => {
            app.prev_buffer();
            return Ok(());
        }
        KeyCode::Enter if app.active_buffer() == crate::buffers::BufferId::Manage => {
            // Apply the selected model from the Manage buffer's model list.
            if let Some(m) = app.manage.selected_model() {
                app.model_name = m.to_string();
            }
            return Ok(());
        }
        KeyCode::Enter => {
            submit(app, key.modifiers.contains(KeyModifiers::SHIFT), tx, config)?;
            return Ok(());
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            let (active, delta) = match key.code {
                KeyCode::PageUp => (app.active_buffer(), -1i32),
                _ => (app.active_buffer(), 1i32),
            };
            let view = match active {
                crate::buffers::BufferId::Chat => &mut app.chat.view,
                crate::buffers::BufferId::Search => &mut app.search.view,
                crate::buffers::BufferId::Chtsh => &mut app.chtsh.view,
                crate::buffers::BufferId::Manage => &mut app.manage.view,
            };
            view.scroll = (view.scroll as i32 + delta * 10).max(0) as usize;
            return Ok(());
        }
        KeyCode::Up | KeyCode::Down if app.active_buffer() == crate::buffers::BufferId::Manage => {
            let len = app.manage.models.len();
            if len > 0 {
                if key.code == KeyCode::Up {
                    app.manage.model_index = app.manage.model_index.saturating_sub(1);
                } else {
                    app.manage.model_index = (app.manage.model_index + 1).min(len - 1);
                }
            }
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
    _config: &config::Config,
) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
            app.close_modal();
            Ok(())
        }
        KeyCode::Up => {
            app.modal_move(true);
            Ok(())
        }
        KeyCode::Down => {
            app.modal_move(false);
            Ok(())
        }
        KeyCode::Enter => {
            // Applying an async job (e.g. switching models) is sync here.
            app.modal_apply();
            let _ = tx;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Handle a submitted prompt: dispatch commands or the active buffer's action.
fn submit(
    app: &mut App,
    shift: bool,
    tx: &EventSender,
    config: &config::Config,
) -> Result<()> {
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
            cmd::Command::Quit => {
                app.running = false;
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
        crate::buffers::BufferId::Manage => Ok(()),
    }
}

/// Send the prompt to the selected model and stream the reply.
fn run_chat(
    app: &mut App,
    text: &str,
    tx: &EventSender,
    config: &config::Config,
) -> Result<()> {
    use crate::buffers::chat::ChatMessage;

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
    tokio::spawn(async move {
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
    Ok(())
}

/// Kick off a search: ask the model for a query plan, then fetch + render.
fn trigger_search(
    app: &mut App,
    text: &str,
    tx: &EventSender,
    config: &config::Config,
) -> Result<()> {
    app.search.last_query = Some(text.to_string());
    app.busy.push(app::JobKind::SearchPlan);

    let ollama = ollama::Ollama::new(config.server.url.clone());
    let model = app.model_name.clone();
    let prompt = text.to_string();
    let tx = tx.clone();
    let summarize = config.search.summarize;
    tokio::spawn(async move {
        // Phase 1: model produces the actual search query.
        let plan = plan_search(&ollama, &model, &prompt).await;
        let query = plan
            .as_ref()
            .ok()
            .and_then(|p| p.get("query").and_then(|q| q.as_str()).map(String::from))
            .unwrap_or_else(|| prompt.clone());
        if let Ok(p) = plan {
            let _ = tx.send(AppEvent::SearchPlan {
                query: vec![query.clone()],
                provider: p
                    .get("provider")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| "duckduckgo".to_string()),
            });
        }
        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::SearchFetch,
            on: true,
        });

        // Phase 2: run the provider query.
        let results = search::search(&query).await;
        let (results, err) = match results {
            Ok(r) => (r, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };

        // Phase 3: summarize (optional) into concise markdown.
        let mut md = search::results_to_markdown(&results);
        if summarize && !results.is_empty() {
            if let Ok(sum) = summarize_results(&ollama, &model, &query, &md).await {
                md = sum;
            }
        }

        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::SearchPlan,
            on: false,
        });
        let _ = tx.send(AppEvent::MarkBusy {
            job: app::JobKind::SearchFetch,
            on: false,
        });
        if let Some(err) = err {
            let _ = tx.send(AppEvent::SearchError { msg: err });
        } else {
            let _ = tx.send(AppEvent::SearchDone { markdown: md });
        }
    });
    Ok(())
}

/// Kick off a cht.sh query: ask the model for the URL, then fetch.
fn trigger_chtsh(
    app: &mut App,
    text: &str,
    tx: &EventSender,
    config: &config::Config,
) -> Result<()> {
    app.chtsh.last_query = Some(text.to_string());
    app.busy.push(app::JobKind::ChtshPlan);

    let ollama = ollama::Ollama::new(config.server.url.clone());
    let model = app.model_name.clone();
    let prompt = text.to_string();
    let tx = tx.clone();
    tokio::spawn(async move {
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
    Ok(())
}

async fn plan_search(
    ollama: &ollama::Ollama,
    model: &str,
    prompt: &str,
) -> Result<serde_json::Value> {
    let sys = "You are a search-query planner. Given the user's request, produce JSON ONLY: {\"query\":\"...\"}".into();
    let resp = ollama
        .complete(
            model,
            vec![
                ollama::ChatMessage { role: "system".into(), content: sys },
                ollama::ChatMessage { role: "user".into(), content: prompt.to_string() },
            ],
        )
        .await?;
    let out = resp.message.map(|m| m.content).unwrap_or_default();
    Ok(serde_json::from_str(&out).unwrap_or(serde_json::json!({ "query": prompt })))
}

async fn plan_chtsh(
    ollama: &ollama::Ollama,
    model: &str,
    prompt: &str,
) -> Result<serde_json::Value> {
    let sys = "You build cht.sh URLs. Given a request, produce JSON ONLY: {\"topic\":\"...\",\"query\":\"...\"}".into();
    let resp = ollama
        .complete(
            model,
            vec![
                ollama::ChatMessage { role: "system".into(), content: sys },
                ollama::ChatMessage { role: "user".into(), content: prompt.to_string() },
            ],
        )
        .await?;
    let out = resp.message.map(|m| m.content).unwrap_or_default();
    Ok(serde_json::from_str(&out).unwrap_or(serde_json::json!({})))
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
                ollama::ChatMessage { role: "system".into(), content: sys },
                ollama::ChatMessage {
                    role: "user".into(),
                    content: format!("Search: {query}\n\nResults:\n{raw}"),
                },
            ],
        )
        .await?;
    Ok(resp.message.map(|m| m.content).unwrap_or_else(|| raw.to_string()))
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
