Role & Objective
You are an expert Rust developer. Implement a cht.sh (cheat.sh) client integration within the existing Rust TUI application. The feature will use a 3-line horizontal prompt space for query construction and the existing buffer/Markdown renderer to display the cheat sheets.

Strict Architectural Constraints

UI Reuse: Use ONLY the allocated 3-line prompt space at the bottom/top of the screen and the main Buffer. Do NOT introduce floating windows, popups, or new buffers.

Renderer Reuse: Feed the cht.sh output into the application's existing Markdown renderer. cht.sh returns ANSI-colored text by default; you MUST append ?T to the URL to strip ANSI and return plain text (e.g., curl cht.sh/rust/read+file?T), which can then be parsed as Markdown.

No Async Blocking: Network requests for fetching suggestions or cheat sheets must not block the TUI event loop.

Phase 1: Models & API Client
Define the structures to interface with the cheat.sh API.

Implement a ChtShClient with async methods:

fetch_root_list() -> Result<Vec<String>>: Fetches [https://cht.sh/:list?T](https://cht.sh/:list?T)

fetch_topic_list(lang: &str) -> Result<Vec<String>>: Fetches [https://cht.sh/](https://cht.sh/){lang}/:list?T

fetch_sheet(lang_or_cmd: &str, query: Option<&str>) -> Result<String>: Fetches [https://cht.sh/](https://cht.sh/){lang_or_cmd}/{query}?T (ensure spaces in the query are replaced with +).

Phase 2: Caching & State Management
Implement local caching so the TUI remains perfectly responsive.

On initialization, check if the root list (languages and commands) is in the local SQLite cache. If missing or stale (older than 7 days), fetch and store it asynchronously.

When a user locks in a language, check the SQLite cache for its topic list. Fetch asynchronously if missing.

Implement a fast, in-memory fuzzy finder (using a crate like nucleo, skim, or simple substring matching) that operates against these cached lists based on the user's current input.

Phase 3: The 3-Line TUI Component
Implement a custom input component conforming strictly to a 3-line height constraint.

Line 1 (Status): Render static keybindings and current state (e.g., [Tab] Switch fields | [Enter] Fetch).

Line 2 (Inputs): Render two distinct input blocks side-by-side: [ Scope ] / [ Query ].

Implement internal focus state: the user is either typing in the Scope block or the Query block.

Bind Tab and Left/Right arrow keys to toggle focus between the two blocks.

Line 3 (Fuzzy Suggestions): Listen to input changes in the active block. Run the fuzzy matcher against the cached list. Render the top 3 to 5 matches horizontally, separated by a visual divider (e.g., |).

Bind Up/Down arrow keys to cycle through the horizontal suggestions on Line 3.

Bind Space to auto-complete the active block with the highlighted suggestion.

Phase 4: Fetch & Render Pipeline
Connect the UI to the buffer.

Bind the Enter key. When pressed:

Clear the 3-line prompt or collapse it to its inactive state.

Display a "Fetching from cht.sh..." status in the existing statusline.

Trigger the async fetch_sheet method using the values from Scope and Query.

On success, pass the plain-text result to the application's existing Markdown renderer and load it into the main buffer.

On failure, report the HTTP/Network error in the statusline without crashing the app.



Role & Objective
You are an expert Rust developer. Implement a local/remote documentation search and rendering feature within this existing Rust TUI application.

Strict Architectural Constraints

UI Reuse: Use ONLY the existing Prompt Box (input), Buffer (rendering), and Statusline (transient states). Do NOT create sidebars, popups, or new buffers.

Renderer Reuse: Use the application's existing Markdown renderer. Do NOT introduce a second rendering engine or format.

No AI/LLMs: Do NOT use embeddings, vector search, RAG, or LLM summaries. Rely purely on deterministic FTS5, exact matching, and BM25 scoring.

No Web Crawling: Do NOT download pages blindly. Fetch HTML only when a user explicitly opens a specific search result or requests a refresh.

No Generic Web Search: Restrict all remote search queries strictly to configured documentation domains.

Phase 1: Core Models & Traits
Define the foundational data structures and the search abstraction.

Implement the Source struct containing id, name, domains (allowed URLs), url_prefixes, and priority.

Implement the SearchResult struct containing title, url (normalized), source_id, snippet (optional), score, and cache_status.

Implement a SearchProvider trait with an async search(query: &str, sources: &[Source]) -> Result<Vec<SearchResult>> method. This abstraction ensures the TUI does not depend on a specific search API.

Phase 2: Ingestion & Processing Pipeline
Create the pipeline that converts remote documentation into compatible Markdown.

Fetch: Implement conditional HTTP GET requests utilizing ETag and Last-Modified headers.

Extract: Use standard Rust HTML parsing crates (do NOT use regex for HTML parsing) to extract main content. Strip boilerplate: nav, headers, footers, sidebars, and ads. Retain: headings, paragraphs, code blocks, lists, and tables.

Convert: Translate the cleaned HTML into Markdown compatible with the existing renderer.

Phase 3: Storage & Local Search
Implement persistent local caching using SQLite.

Initialize a SQLite database in the app's standard data directory.

Store successfully parsed documents. Schema requirements: URL (primary key/normalized), source ID, title, processed Markdown, fetch timestamp, ETag.

Implement SQLite FTS5 for local full-text search against the cached Markdown and titles.

Phase 4: Hybrid Search & Ranking
Combine local and remote search results seamlessly.

When a search is triggered, execute the local FTS5 search and the remote SearchProvider query concurrently.

Merge the results. Deduplicate strictly by normalized URL. If a document exists locally and remotely, merge them into a single SearchResult prioritizing local cache metadata.

Rank the final list deterministically using a combination of FTS/BM25 score, heading/title exact matches, and source priority.

Phase 5: Async TUI Integration
Wire the feature into the existing event loop without blocking the main thread.

Input Binding: Bind prompt submission to trigger the search and clear the prompt box immediately.

Navigation: Bind up/down arrow keys to select items in the search results buffer. Bind the existing "action/enter" key to fetch and render the selected document.

Buffer State: The existing buffer must alternate cleanly between SEARCH_RESULTS (list view) and DOCUMENT (rendered Markdown view).

Async Events: Route background tasks to the existing event loop using standard messages (e.g., SearchStarted, SearchCompleted, DocumentLoaded, SearchFailed).

Statusline Updates: Map the async states directly to the existing statusline (e.g., "Searching...", "Extracting document...", "Loaded from cache"). Handle HTTP/extraction errors gracefully via the statusline without crashing the TUI or clearing the previous buffer state.
