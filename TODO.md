# [COMPLETED] cht.sh (cheat.sh) client integration
- [x] Phase 1: Models & API Client (`ChtShClient` with `fetch_root_list`, `fetch_topic_list`, `fetch_sheet` with `?T`)
- [x] Phase 2: Caching & State Management (7-day TTL disk cache, async background fetching, SkimMatcherV2 fuzzy finding)
- [x] Phase 3: 3-Line TUI Component (Line 1: Status/keys, Line 2: `[ Scope ] │ [ Query ]`, Line 3: Horizontal fuzzy suggestions with `[Space]` completion & `[↑/↓]` selection)
- [x] Phase 4: Fetch & Render Pipeline (Direct fetch on Enter, render to Markdown buffer, graceful error handling)

# [COMPLETED] Local & Remote Documentation Search & Rendering
- [x] Phase 1: Core Models & Traits (`Source`, `SearchResult`, `CacheStatus`, `SearchProvider` trait)
- [x] Phase 2: Ingestion & Processing Pipeline (Conditional HTTP GET with `ETag`/`Last-Modified`, HTML boilerplate stripping via `scraper`, semantic Markdown conversion)
- [x] Phase 3: Storage & Local Search (SQLite FTS5 database at `~/.local/share/otto/docs.db` with BM25 full-text indexing)
- [x] Phase 4: Hybrid Search & Ranking (Concurrent FTS5 + scoped provider queries, normalized URL deduplication, deterministic ranking)
- [x] Phase 5: Async TUI Integration (Interactive `SearchBuffer` with list & document views, `↑`/`↓`/number key selection, `Enter` to fetch & view, `Esc`/`b` to return)

