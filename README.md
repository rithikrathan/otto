# Otto 🚀

> **Fast, beautiful, keyboard-driven Terminal Assistant & Coding Companion.**  
> Built with Rust, Ratatui, and Tokio.

---

## 📑 Table of Contents
- [Overview](#overview)
- [Key Features](#key-features)
- [Multi-Provider LLM Setup](#multi-provider-llm-setup)
  - [1. Ollama (Local)](#1-ollama-local)
  - [2. OpenAI](#2-openai)
  - [3. Groq (Ultra-fast Inference)](#3-groq-ultra-fast-inference)
  - [4. Google Gemini](#4-google-gemini)
  - [5. NVIDIA NIM](#5-nvidia-nim)
  - [6. OpenRouter](#6-openrouter)
  - [7. Custom OpenAI-Compatible Endpoints (vLLM, LiteLLM, LM Studio)](#7-custom-openai-compatible-endpoints)
- [Interactive Categorized Model Picker](#interactive-categorized-model-picker)
- [Configuration Reference (`config.toml`)](#configuration-reference-configtoml)
- [Documentation & Web Search Engine](#documentation--web-search-engine)
- [Interactive Cheat Sheet (`cht.sh`)](#interactive-cheat-sheet-chtsh)
- [Keyboard Shortcuts](#keyboard-shortcuts)

---

## 🌟 Overview

Otto combines three developer superpowers into a unified TUI:
1. **Multi-Provider AI Chat**: Instant streaming responses across local models (Ollama) and top cloud APIs (OpenAI, Groq, Google Gemini, NVIDIA NIM, OpenRouter).
2. **Deterministic Documentation Engine**: SQLite FTS5 local cache + BM25 ranking + HTML-to-Markdown reader for programming languages, frameworks, and forums.
3. **Interactive Cheat Sheets**: Fast querying and fuzzy autocompletion powered by [cheat.sh](https://cheat.sh).

---

## 🔌 Multi-Provider LLM Setup

Otto supports both local LLMs and external cloud providers via standard OpenAI Chat Completions SSE and native Ollama protocols.

### 🔑 Setting API Keys

#### Option A: Via Shell Environment (Recommended)
Add your API keys to your `~/.bashrc` or `~/.zshrc`:

```bash
# Otto LLM Provider API Keys
export OPENAI_API_KEY="sk-proj-..."
export GROQ_API_KEY="gsk_..."
export GEMINI_API_KEY="AIzaSy..."          # or GOOGLE_API_KEY
export NVIDIA_API_KEY="nvapi-..."          # or NIM_API_KEY
export OPENROUTER_API_KEY="sk-or-v1-..."
```

Then reload your shell:
```bash
source ~/.bashrc    # or source ~/.zshrc
```
Otto will automatically read these variables at runtime whenever you select a model from that provider!

#### Option B: In `~/.config/otto/config.toml`
Alternatively, configure `api_key` under each provider in your config:
```toml
[providers.groq]
url = "https://api.groq.com/openai/v1"
api_key = "gsk_..."
```

---

### 🎯 How to Choose Other Models

You can choose or add any model from any provider in two ways:

#### 1. Quick Switch via Command Prompt
In the Otto chat prompt, type `/model` followed by any valid model ID supported by your active provider:
```text
/model gpt-4o
/model deepseek-ai/deepseek-r1
/model llama-3.3-70b-versatile
/model qwen2.5-coder:7b
```
Otto switches the active model immediately and saves your preference.

#### 2. Add Models to the Categorized Model Picker List
Add any custom model names into `~/.config/otto/config.toml` under the provider's `models` array:
```toml
[providers.nvidia]
url = "https://integrate.api.nvidia.com/v1"
models = [
  "meta/llama-3.1-405b-instruct",
  "meta/llama-3.1-70b-instruct",
  "deepseek-ai/deepseek-r1",
  "nvidia/nemotron-4-340b-instruct",
  "mistralai/mistral-large-2-instruct"  # <- Added custom model
]
```
These will permanently appear inside the `/model` floating window under that provider's category tab.

---

### 1. Ollama (Local)
Run completely offline with zero API keys.
- **Official Website**: [ollama.com](https://ollama.com)
- **Default Endpoint**: `http://localhost:11434`
- **Supported Models**: `qwen2.5-coder`, `deepseek-r1`, `llama3.3`, `mistral`, `codellama`, etc.
- **Quickstart**:
  ```bash
  ollama run qwen2.5-coder:1.5b
  ```

---

### 2. OpenAI
- **API Portal**: [platform.openai.com](https://platform.openai.com/api-keys)
- **Base URL**: `https://api.openai.com/v1`
- **Environment Variable**: `OPENAI_API_KEY`
- **Supported Models**: `gpt-4o`, `gpt-4o-mini`, `o3-mini`, `o1`, `gpt-4-turbo`
- **Setup**:
  ```bash
  export OPENAI_API_KEY="sk-..."
  ```

---

### 3. Groq (Ultra-fast Inference)
- **API Portal**: [console.groq.com](https://console.groq.com/keys)
- **Base URL**: `https://api.groq.com/openai/v1`
- **Environment Variable**: `GROQ_API_KEY`
- **Supported Models**: `llama-3.3-70b-versatile`, `deepseek-r1-distill-llama-70b`, `llama-3.1-8b-instant`, `mixtral-8x7b-32768`
- **Setup**:
  ```bash
  export GROQ_API_KEY="gsk_..."
  ```

---

### 4. Google Gemini
- **API Portal**: [Google AI Studio](https://aistudio.google.com/app/apikey)
- **Base URL**: `https://generativelanguage.googleapis.com/v1beta/openai`
- **Environment Variable**: `GEMINI_API_KEY` (or `GOOGLE_API_KEY`)
- **Supported Models**: `gemini-2.0-flash`, `gemini-1.5-pro`, `gemini-1.5-flash`, `gemini-2.0-flash-thinking-exp`
- **Setup**:
  ```bash
  export GEMINI_API_KEY="AIzaSy..."
  ```

---

### 5. NVIDIA NIM
- **API Portal**: [build.nvidia.com](https://build.nvidia.com)
- **Base URL**: `https://integrate.api.nvidia.com/v1`
- **Environment Variable**: `NVIDIA_API_KEY` (or `NIM_API_KEY`)
- **Supported Models**: `meta/llama-3.1-405b-instruct`, `meta/llama-3.1-70b-instruct`, `deepseek-ai/deepseek-r1`, `nvidia/nemotron-4-340b-instruct`
- **Setup**:
  ```bash
  export NVIDIA_API_KEY="nvapi-..."
  ```

---

### 6. OpenRouter
- **API Portal**: [openrouter.ai](https://openrouter.ai/keys)
- **Base URL**: `https://openrouter.ai/api/v1`
- **Environment Variable**: `OPENROUTER_API_KEY`
- **Supported Models**: `anthropic/claude-3.5-sonnet`, `deepseek/deepseek-r1`, `google/gemini-2.0-flash-001`, `openai/gpt-4o`
- **Setup**:
  ```bash
  export OPENROUTER_API_KEY="sk-or-v1-..."
  ```

---

### 7. Custom OpenAI-Compatible Endpoints
Compatible with **vLLM**, **LiteLLM**, **LocalAI**, **LM Studio**, and **Text Generation WebUI**.
- **Base URL**: e.g., `http://localhost:8000/v1`
- Configure under `[providers.custom]` in `config.toml`.

---

## 🪟 Interactive Categorized Model Picker

Press `/model` (or open from settings) to display the new floating categorized Model Picker:

```text
┌─ model picker (providers & models) ────────────────────────┐
│ search (press '.' to focus)                                │
│ ...                                                        │
├─ provider categories (←/→ switch) ─────────────────────────┤
│  ◄  [ollama]   [openai]  ▶[groq]◀  [gemini]  [nvidia]  ►   │
├─ models ───────────────────────────────────────────────────┤
│   ▶ llama-3.3-70b-versatile  ✓ (active)                    │
│     deepseek-r1-distill-llama-70b                          │
│     llama-3.1-8b-instant                                   │
│     mixtral-8x7b-32768                                     │
│     gemma2-9b-it                                           │
└─ . search   ←/→ provider   ↑/↓ model   Enter select   Esc ─┘
```

- **Switch Provider Tabs**: Press `Left` / `Right` (or `Tab` / `Shift+Tab`) to change the active provider category.
- **Select Model**: Press `Up` / `Down` to navigate models and hit `Enter` to switch providers and models instantly.
- **Global Search**: Press `.` to filter models and providers simultaneously.

---

## ⚙️ Configuration Reference (`config.toml`)

The config file is located at `~/.config/otto/config.toml`:

```toml
[server]
provider = "groq"                                      # Active provider: "ollama", "openai", "groq", "gemini", "nvidia", "openrouter", "custom"
url = "https://api.groq.com/openai/v1"                 # Active URL (automatically resolved if omitted)
# api_key = "gsk_..."                                  # Optional if set in ENV

[model]
name = "llama-3.3-70b-versatile"

[search]
provider = "duckduckgo"
summarize = true
max_results = 40                                       # Maximum retrieved links

# Custom documentation sources
[[search.custom_sources]]
id = "my-docs"
name = "Custom Framework Documentation"
domains = ["mydocs.internal.com"]
priority = 90

[providers.ollama]
url = "http://localhost:11434"
default_model = "qwen2.5-coder-1.5b:latest"
models = ["qwen2.5-coder-1.5b:latest", "deepseek-r1:8b", "llama3.3:70b"]

[providers.openai]
url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"
models = ["gpt-4o", "gpt-4o-mini", "o3-mini", "o1"]

[providers.groq]
url = "https://api.groq.com/openai/v1"
default_model = "llama-3.3-70b-versatile"
models = ["llama-3.3-70b-versatile", "deepseek-r1-distill-llama-70b", "llama-3.1-8b-instant"]

[providers.gemini]
url = "https://generativelanguage.googleapis.com/v1beta/openai"
default_model = "gemini-2.0-flash"
models = ["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"]

[providers.nvidia]
url = "https://integrate.api.nvidia.com/v1"
default_model = "meta/llama-3.1-405b-instruct"
models = ["meta/llama-3.1-405b-instruct", "deepseek-ai/deepseek-r1"]

[providers.openrouter]
url = "https://openrouter.ai/api/v1"
default_model = "anthropic/claude-3.5-sonnet"
models = ["anthropic/claude-3.5-sonnet", "deepseek/deepseek-r1", "google/gemini-2.0-flash-001"]
```

---

## 🔍 Documentation & Web Search Engine

Switch to the **Search Buffer** (`Tab`) to query docs and web links:
- **Built-in Sources**: Rust, Nim, Lua/Luau, Godot/GDScript, Processing, p5.js, p5py, Manim, Motion Canvas TS, TypeScript, React, Node.js, Python, Go, C/C++, Stack Overflow, GitHub, Reddit, ArchWiki, and more.
- **Arbitrary Web Links**: Any domain retrieved from DuckDuckGo is displayed with its clean host tag (e.g. `(medium.com)`).
- **Offline SQLite FTS5 Cache**: Visited documentation pages are stored locally at `~/.local/share/otto/docs.db` and BM25 indexed for instantaneous offline lookup.
- **Direct Reading**: Press `Enter` or numbers `1..=9` on any search result to extract clean Markdown directly in the terminal without opening a browser.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Description |
| :--- | :--- |
| `Tab` / `Shift+Tab` | Switch active buffer (`Chat` ⇄ `Search` ⇄ `Chtsh`) |
| `?` / `F1` / `/help` | Open floating Help modal |
| `/model` | Open Categorized Model & Provider Picker |
| `/settings` | Open Settings modal |
| `Left` / `Right` | Switch provider category in Model Picker / Switch scope in `cht.sh` |
| `.` | Focus search filter in Model Picker |
| `Up` / `Down` | Browse prompt history / Navigate results / Select modal options |
| `Enter` | Submit prompt / Select model / Open doc link |
| `Shift+Enter` | Insert newline in multi-line prompt |
| `Ctrl+Left` / `Ctrl+Right` | Move caret word backward / forward |
| `Ctrl+Backspace` / `Ctrl+W` | Delete word backward |
| `Ctrl+Delete` | Delete word forward |
| `Ctrl+K` | Clear chat context & history |
| `PageUp` / `PageDown` | Scroll active buffer view |
| `Esc` | Close modal / Cancel active streaming task |
| `Ctrl+C` / `Ctrl+Q` | Exit Otto |

---

## 🛠️ Building and Running

```bash
# Clone the repository
git clone https://github.com/rithikrathan/otto.git
cd otto

# Build and run
cargo run --release
```
