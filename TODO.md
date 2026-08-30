# [COMPLETED] cht.sh (cheat.sh) client integration
- [x] Phase 1: Models & API Client (`ChtShClient` with `fetch_root_list`, `fetch_topic_list`, `fetch_sheet` with `?T`)
- [x] Phase 2: Caching & State Management (7-day TTL disk cache, async background fetching, SkimMatcherV2 fuzzy finding)
- [x] Phase 3: 3-Line TUI Component (Line 1: Status/keys, Line 2: `[ Scope ] │ [ Query ]`, Line 3: Horizontal fuzzy suggestions with `[Space]` completion & `[↑/↓]` selection)
- [x] Phase 4: Fetch & Render Pipeline (Direct fetch on Enter, render to Markdown buffer, graceful error handling)



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
