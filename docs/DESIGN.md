# otc — Ollama TUI Chat — Design Document

## 1. Overview

`otc` is a terminal user interface (TUI) chat client for **Ollama** models, built in
**Rust** with the **ratatui** library. It provides a persistent, buffer-based chat
experience in the terminal with three auxiliary features:

1. **Speech-to-text** (STT) input.
2. **Web / knowledge search** whose queries and endpoints are constructed by an
   Ollama model.
3. **Settings / management** with the ability to clear context on command.

The whole app is modal and buffer-oriented ("like a React/Svelte style swap of
buffers"), designed to run in a **narrow vertical split** beside an editor, so the
UI styling is intentionally minimal to avoid visual interference with the editor.

## 2. Goals & Non-Goals

### Goals
- Chat with an Ollama model, with streaming assistant responses rendered as markdown.
- Multiple switchable, per-mode buffers sharing one prompt box.
- Lightweight, offline speech-to-text that runs acceptably on an older machine.
- A web search mode where the model decides the query and the appropriate endpoint,
  with concise, procedurally summarized results.
- A cht.sh mode where the model builds the correct URL for a query.
- A management buffer for model selection, chat management, and export.
- Clear conversation context on command.
- Minimal, editor-friendly styling for a narrow split.

### Non-Goals (v1)
- Full async streaming / mermaid diagram rendering.
- Voice-activity-triggered always-listening STT.
- Multi-turn tool calling loops (each feature is an explicit user-initiated mode).
- Rich mouse support.

## 3. Technology Stack

| Concern          | Choice                                            | Rationale                                                              |
|------------------|---------------------------------------------------|------------------------------------------------------------------------|
| TUI framework    | `ratatui` 0.29 + `crossterm`                      | User-specified; active, well-documented.                               |
| Runtime          | `tokio`                                           | Async IO for ollama/search/cht.sh without blocking the UI loop.        |
| HTTP             | `reqwest`                                         | Streaming responses from Ollama, simple fetches.                       |
| Markdown render  | `tui-markdown` (pulldown-cmark)                   | Converts markdown to `ratatui::Text` with syntax highlighting.         |
| Audio capture    | `cpal`                                            | Cross-platform mic capture.                                            |
| Speech-to-text   | `vosk` (+ `libvosk` via `vosk-sys`)               | Lightweight, offline, CPU-first (runs on Raspberry Pi-class hardware). 16kHz mono streaming. |
| Serialization    | `serde`, `serde_json`                             | Ollama JSON APIs, tokenize/parse model output.                         |
| Config           | `dirs` + TOML file                                | Local config for server URL, model, STT model path, providers.         |

## 4. Architecture / Event Model

`socketbox`-style single-threaded UI with background work on Tokio.

```
                    ┌────────────────────────────┐
   keyboard/input   │          Event Loop         │
  ────────────────► │  app.rs : handle_event()    │
                    │  ┌──────────────────────┐   │
                    │  │    App state         │   │
                    │  │  buffers, input,     │   │
                    │  │  models, history     │   │
                    │  └──────────┬───────────┘   │
                    └─────────────┼───────────────┘
                                  │  Tokio tasks dispatch work
                                  ▼
                    ┌────────────────────────────┐
                    │  Background Tokio tasks    │
                    │  ollama / search / cht.sh /│
                    │  stt                        │
                    └────────────┬───────────────┘
                                 │  send AppEvent back over channel
                                 ▼
                       ┌──────────────────┐
                       │   Event channel  │   (crossbeam / tokio mpsc)
                       └──────────────────┘
```

- The main loop alternates between reading the input stream and draining the
  event channel.
- Long-running work (streaming a chat, performing a search, fetching cht.sh,
  transcribing audio) runs on Tokio tasks and reports progress back via typed
  `AppEvent`s (e.g. `ChatDelta`, `SearchDone`, `ChtshDone`, `SttPartial`, `SttFinal`).

### AppEvent (channel message) type sketch
```rust
enum AppEvent {
    Input(KeyEvent),
    Tick,
    ChatDelta { buffer: BufferId, delta: String },
    ChatDone { buffer: BufferId },
    ChatError { buffer: BufferId, msg: String },
    ModelsLoaded { models: Vec<ModelInfo> },
    SearchDone { buffer: BufferId, markdown: String },
    SearchError { msg: String },
    ChtshDone { buffer: BufferId, text: String },
    SttPartial { text: String },
    SttFinal { text: String },
    ...
}
```

## 5. UI / Layout

```
┌───────────────────────────────────────────┐
│  Buffer Tabs: [Chat] [Search] [cht.sh] [Manage] │  ← Tab / Shift+Tab
│                                             │
│   (scrollable buffer area:                  │
│     per-buffer markdown history,            │
│     scroll with PgUp/PgDn / scroll keys)    │
├───────────────────────────────────────────┤
│  nvim-style statusline                     │  ← buffer · model · spinner · pos
├───────────────────────────────────────────┤
│  > prompt box (wraps)                     │  ← expands up to 5 lines, then scrolls
│                                             │
├───────────────────────────────────────────┤
│  nvim-style bottom statusline              │  ← mode / keyhints · [mic] STT state
└───────────────────────────────────────────┘
```

- **Top row**: buffer tabs; `Tab` advances, `Shift+Tab` goes back.
- **Buffer area**: the active buffer renders its full scrollable history, each
  message a markdown block (assistant / user / search / cht.sh result).
- **Statusline (separator)**: nvim-like, e.g.
  `CHAT · llama3.2 · spinning… · tok in 1200 · tok out 340 · ctx 61%`.
  Includes the spinner while work is in progress, plus live token accounting:
  - **reading (tok in)** — tokens sent to the model (`prompt_eval_count`).
  - **writing (tok out)** — tokens generated (`eval_count`).
  - **ctx %** — `prompt_eval_count / num_ctx * 100`, i.e. how full the model's
    context window is; this makes it obvious when it's time to `/clear`.
- **Prompt box**: single input shared across buffers. Text wraps; the box grows to
  a max of **5 lines**, then scrolls internally. `Enter` submits (unless in
  multi-line `/` mode).
- **Bottom statusline**: shows mode/hints and STT recording state (`[● REC]`).

### Styling / Interference-free
- Deliberately uses minimal borders/chrome and a restrained color palette.
- Uses `crossterm` alternate screen; colors chosen to be neutral so the TUI does
  not clash with an adjacent editor split.

## 6. Buffers

A `BufferId` enum drives tabs; each buffer keeps its own history and scroll pos.

```rust
enum BufferId { Chat, Search, Chtsh, Manage }
```

- **Chat** — sends prompt to the selected model (streaming), appends user +
  assistant markdown blocks.
- **Search** — the prompt is sent to an Ollama model which returns a JSON plan
  like `{"query":"...","provider":"duckduckgo"}`; the app then calls the search
  provider; an optional Ollama summarization produces a concise markdown result.
- **cht.sh** — the prompt is sent to Ollama which returns a URL plan like
  `{"topic":"rust","query":"read+file+lines"}`; the app builds
  `https://cht.sh/<topic>/<query+...>?T`, fetches it, and renders the text.
- **Manage** — model selection (from `/api/tags`), chat management
  (list/switch/delete), export current chat to a file, context controls.

## 7. Feature Details

### 7.1 Speech-to-Text (STT)
- Mic captured via `cpal` at 16 kHz mono 16-bit (VOSK requirement).
- Hotkey (default `Ctrl+M`) toggles recording; live partial text is echoed into a
  status/overlay; on stop, the recognized text is inserted into the prompt box.
- `vosk` model path configurable; default `~/.local/share/otc/vosk-model-small-en-us`.
- Because VOSK runs fully on CPU, this meets the "old system" constraint.

### 7.2 Web Search
- Default provider: **DuckDuckGo** (free, no API key), behind a `SearchProvider`
  trait so Brave/Tavily/etc. can be added later.
- Flow: user prompt → Ollama composes `{query, provider}` → provider query →
  (optional) Ollama summarizes → concise markdown appended to Search buffer.
- Spinner shown while the model composes and while the provider runs.

### 7.3 cht.sh
- Flow: user prompt → Ollama composes `{topic, query}` → build URL →
  `reqwest` GET → text rendered in cht.sh buffer. Spinner while composing/fetching.

### 7.4 Context clearing
- Ollama keeps context by us passing the full message history on each call.
- `:clear` / default `Ctrl+K` wipes the in-memory history and clears the Chat
  buffer, which resets the model context.
- The statusline's **ctx %** counter (see §5) is reset along with the history,
  giving immediate feedback that the model context was cleared.

### 7.6 Token accounting
- Consumers: `ChatDelta`, `ChatDone`, search/cht.sh plan+summary calls.
- Ollama's streaming `/api/chat` returns `prompt_eval_count` (reading) and
  `eval_count` (writing) — accumulated into the app-level `TokenStats` and
  rendered in the statusline.

### 7.5 Export
- From the Manage buffer, export the current conversation as Markdown to a
  user-chosen path.

## 8. Config

`~/.config/otc/config.toml` (or `dirs::config_dir`):
```toml
[server]
url = "http://localhost:11434"

[model]
name = "llama3.2"

[stt]
mode = "vosk"
model_path = "~/.local/share/otc/vosk-model-small-en-us"

[search]
provider = "duckduckgo"
summarize = true
```

## 9. Project Structure

```
otc/
  docs/DESIGN.md
  src/
    main.rs          — terminal init, alternate screen, event loop bootstrap
    app.rs           — App state, buffer switching, input/prompt handling
    event.rs         — Tokio event channel + event loop
    ui/
      mod.rs         — overall frame render
      layout.rs      — split layout, tabs, statuslines
      prompt.rs      — expandable prompt box (<= 5 lines, scrollable)
      buffer.rs      — scrollable markdown buffer rendering
      spinner.rs     — spinner animation
    buffers/
      mod.rs
      chat.rs
      search.rs
      chtsh.rs
      manage.rs
    ollama.rs        — tags, streaming chat, generate, compose JSON plans
    stt.rs           — cpal capture + vosk recognition task
    search.rs        — SearchProvider trait + DuckDuckGo + summarizer
    chtsh.rs         — URL builder + fetcher
    cmd.rs           — /commands (clear, export, model, ...)
    config.rs        — config load/save
```

## 10. Keybindings (defaults)

| Action                     | Key                 |
|----------------------------|---------------------|
| Next buffer                | `Tab`               |
| Previous buffer            | `Shift+Tab`         |
| Submit prompt              | `Enter`             |
| Newline (in `{ }` mode)    | `Alt+Enter`         |
| Record/stop STT            | `Ctrl+M`            |
| Clear context (Chat)       | `Ctrl+K` / `/clear` |
| Scroll buffer up/down      | `PgUp` / `PgDn`, arrow keys |
| Quit                       | `Ctrl+C`, `Ctrl+Q`  |

## 11. Milestones

1. Scaffold project and confirm build (ratatui shell with layout + tabs + prompt).
2. Ollama integration: list models, stream chat into Chat buffer, `:clear`.
3. Scrollable buffers, spinner, statuslines, expandable prompt box.
4. cht.sh buffer with Ollama-built URLs.
5. Search buffer with provider + summarization.
6. Manage buffer: model select, chat manage, export.
7. STT: cpal + VOSK push-to-talk.
8. Polish, tests, docs.

## 12. Open Items (resolved during build)
- ~~Search provider~~ → DuckDuckGo default (pluggable).
- STT hotkey default `Ctrl+M`; clear default `Ctrl+K`.
- Prompt box max expansion: 5 lines (specified).
