//! Documentation search engine: local SQLite FTS5 + remote provider + HTML-to-Markdown ingestion.
//!
//! Features:
//! - Deterministic local/remote search without AI/LLMs
//! - SQLite FTS5 full-text search with BM25 ranking
//! - Conditional HTTP caching (ETag / Last-Modified)
//! - Clean HTML parsing and Markdown conversion (stripping boilerplate)
//! - URL normalization and hybrid result deduplication

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use rusqlite::{params, Connection};
use scraper::{Html, Selector};
use url::Url;

/// Cache status of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Cached,
    Remote,
}

/// A documentation source definition.
#[derive(Debug, Clone)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub domains: Vec<String>,
    pub url_prefixes: Vec<String>,
    pub priority: u32,
}

impl Source {
    pub fn from_config(config: &crate::config::Config) -> Vec<Source> {
        let mut sources = Vec::new();
        for custom in &config.search.custom_sources {
            sources.push(Source {
                id: custom.id.clone(),
                name: custom.name.clone(),
                domains: custom.domains.clone(),
                url_prefixes: custom.url_prefixes.clone(),
                priority: custom.priority,
            });
        }
        for default_s in Self::default_sources() {
            if !sources.iter().any(|s| s.id == default_s.id) {
                sources.push(default_s);
            }
        }
        sources
    }

    pub fn default_sources() -> Vec<Source> {
        vec![
            Source {
                id: "rust".into(),
                name: "Rust Docs & Forum".into(),
                domains: vec![
                    "docs.rs".into(),
                    "doc.rust-lang.org".into(),
                    "users.rust-lang.org".into(),
                    "internals.rust-lang.org".into(),
                    "rust-lang.github.io".into(),
                    "crates.io".into(),
                ],
                url_prefixes: vec![
                    "https://docs.rs".into(),
                    "https://doc.rust-lang.org".into(),
                    "https://users.rust-lang.org".into(),
                ],
                priority: 100,
            },
            Source {
                id: "stackoverflow".into(),
                name: "Stack Overflow".into(),
                domains: vec![
                    "stackoverflow.com".into(),
                    "stackexchange.com".into(),
                    "serverfault.com".into(),
                    "superuser.com".into(),
                    "askubuntu.com".into(),
                ],
                url_prefixes: vec![
                    "https://stackoverflow.com".into(),
                    "https://superuser.com".into(),
                    "https://serverfault.com".into(),
                    "https://askubuntu.com".into(),
                ],
                priority: 98,
            },
            Source {
                id: "mdn".into(),
                name: "MDN Web Docs".into(),
                domains: vec!["developer.mozilla.org".into()],
                url_prefixes: vec!["https://developer.mozilla.org".into()],
                priority: 96,
            },
            Source {
                id: "typescript".into(),
                name: "TypeScript & JS Docs".into(),
                domains: vec![
                    "typescriptlang.org".into(),
                    "nodejs.org".into(),
                    "deno.land".into(),
                    "bun.sh".into(),
                    "npmjs.com".into(),
                    "tc39.es".into(),
                ],
                url_prefixes: vec![
                    "https://www.typescriptlang.org".into(),
                    "https://nodejs.org".into(),
                ],
                priority: 95,
            },
            Source {
                id: "react".into(),
                name: "React & React Native".into(),
                domains: vec![
                    "react.dev".into(),
                    "reactjs.org".into(),
                    "reactnative.dev".into(),
                ],
                url_prefixes: vec![
                    "https://react.dev".into(),
                    "https://reactjs.org".into(),
                ],
                priority: 95,
            },
            Source {
                id: "python".into(),
                name: "Python Docs & Forum".into(),
                domains: vec![
                    "docs.python.org".into(),
                    "pypi.org".into(),
                    "discourse.python.org".into(),
                    "realpython.com".into(),
                    "readthedocs.io".into(),
                ],
                url_prefixes: vec!["https://docs.python.org".into(), "https://pypi.org".into()],
                priority: 94,
            },
            Source {
                id: "go".into(),
                name: "Go Package & Docs".into(),
                domains: vec![
                    "pkg.go.dev".into(),
                    "go.dev".into(),
                    "golang.org".into(),
                    "discuss.golang.org".into(),
                ],
                url_prefixes: vec!["https://pkg.go.dev".into(), "https://go.dev".into()],
                priority: 93,
            },
            Source {
                id: "cpp".into(),
                name: "C/C++ Reference".into(),
                domains: vec![
                    "cppreference.com".into(),
                    "en.cppreference.com".into(),
                    "cplusplus.com".into(),
                    "discuss.cplusplus.com".into(),
                ],
                url_prefixes: vec!["https://en.cppreference.com".into()],
                priority: 92,
            },
            Source {
                id: "nim".into(),
                name: "Nim Docs & Forum".into(),
                domains: vec![
                    "nim-lang.org".into(),
                    "nim-lang.github.io".into(),
                    "forum.nim-lang.org".into(),
                ],
                url_prefixes: vec![
                    "https://nim-lang.org/docs/".into(),
                    "https://nim-lang.org".into(),
                ],
                priority: 92,
            },
            Source {
                id: "lua".into(),
                name: "Lua Manual & Docs".into(),
                domains: vec![
                    "lua.org".into(),
                    "pgl.yoyo.org".into(),
                    "luau-lang.org".into(),
                    "devforum.roblox.com".into(),
                ],
                url_prefixes: vec![
                    "https://www.lua.org/manual/".into(),
                    "https://www.lua.org/pil/".into(),
                ],
                priority: 92,
            },
            Source {
                id: "godot".into(),
                name: "Godot & GDScript".into(),
                domains: vec![
                    "docs.godotengine.org".into(),
                    "godotengine.org".into(),
                    "godotshaders.com".into(),
                    "forum.godotengine.org".into(),
                ],
                url_prefixes: vec!["https://docs.godotengine.org".into()],
                priority: 92,
            },
            Source {
                id: "processing".into(),
                name: "Processing & p5.js".into(),
                domains: vec![
                    "p5js.org".into(),
                    "processing.org".into(),
                    "discourse.processing.org".into(),
                ],
                url_prefixes: vec![
                    "https://p5js.org/reference/".into(),
                    "https://processing.org/reference/".into(),
                ],
                priority: 92,
            },
            Source {
                id: "p5py".into(),
                name: "p5py Documentation".into(),
                domains: vec!["p5py.org".into(), "p5.readthedocs.io".into()],
                url_prefixes: vec!["https://p5py.org".into(), "https://p5.readthedocs.io".into()],
                priority: 92,
            },
            Source {
                id: "manim".into(),
                name: "Manim Docs".into(),
                domains: vec![
                    "docs.manim.community".into(),
                    "manim.community".into(),
                    "3b1b.github.io".into(),
                ],
                url_prefixes: vec![
                    "https://docs.manim.community".into(),
                    "https://3b1b.github.io/manim/".into(),
                ],
                priority: 92,
            },
            Source {
                id: "motioncanvas".into(),
                name: "Motion Canvas TS".into(),
                domains: vec!["motioncanvas.io".into(), "docs.motioncanvas.io".into()],
                url_prefixes: vec!["https://motioncanvas.io/docs/".into()],
                priority: 92,
            },
            Source {
                id: "linux".into(),
                name: "Linux Manual & ArchWiki".into(),
                domains: vec![
                    "man7.org".into(),
                    "wiki.archlinux.org".into(),
                    "archlinux.org".into(),
                    "linux.die.net".into(),
                    "kernel.org".into(),
                ],
                url_prefixes: vec![
                    "https://man7.org/linux/man-pages/".into(),
                    "https://wiki.archlinux.org".into(),
                ],
                priority: 90,
            },
            Source {
                id: "github".into(),
                name: "GitHub".into(),
                domains: vec![
                    "github.com".into(),
                    "raw.githubusercontent.com".into(),
                    "gist.github.com".into(),
                ],
                url_prefixes: vec!["https://github.com".into()],
                priority: 88,
            },
            Source {
                id: "reddit".into(),
                name: "Reddit".into(),
                domains: vec!["reddit.com".into(), "old.reddit.com".into()],
                url_prefixes: vec!["https://www.reddit.com".into(), "https://reddit.com".into()],
                priority: 85,
            },
            Source {
                id: "community".into(),
                name: "Dev Community".into(),
                domains: vec![
                    "dev.to".into(),
                    "news.ycombinator.com".into(),
                    "hashnode.dev".into(),
                    "medium.com".into(),
                ],
                url_prefixes: vec!["https://dev.to".into(), "https://news.ycombinator.com".into()],
                priority: 80,
            },
            Source {
                id: "general".into(),
                name: "Web".into(),
                domains: vec![],
                url_prefixes: vec![],
                priority: 50,
            },
        ]
    }

    pub fn matches_url(&self, raw_url: &str) -> bool {
        if self.domains.is_empty() && self.url_prefixes.is_empty() {
            return true;
        }
        if let Ok(parsed) = Url::parse(raw_url) {
            if let Some(host) = parsed.host_str() {
                for d in &self.domains {
                    if host == d || host.ends_with(&format!(".{d}")) {
                        return true;
                    }
                }
            }
        }
        for prefix in &self.url_prefixes {
            if raw_url.starts_with(prefix) {
                return true;
            }
        }
        false
    }
}

/// Identifies source ID for any URL (falling back to hostname for arbitrary websites).
pub fn identify_source_id(raw_url: &str, sources: &[Source]) -> (String, f32) {
    for s in sources {
        if s.matches_url(raw_url) && s.id != "general" {
            return (s.id.clone(), s.priority as f32);
        }
    }
    if let Ok(parsed) = Url::parse(raw_url) {
        if let Some(host) = parsed.host_str() {
            let clean_host = host.strip_prefix("www.").unwrap_or(host);
            return (clean_host.to_string(), 50.0);
        }
    }
    ("web".to_string(), 50.0)
}

/// A search result from local cache or remote provider.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub source_id: String,
    pub snippet: Option<String>,
    pub score: f32,
    pub cache_status: CacheStatus,
}

/// Cached documentation entry.
#[derive(Debug, Clone)]
pub struct CachedDocument {
    pub url: String,
    pub source_id: String,
    pub title: String,
    pub markdown: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fetched_at: u64,
}

/// Search provider abstraction.
pub trait SearchProvider: Send + Sync {
    fn search(&self, query: &str, sources: &[Source]) -> impl std::future::Future<Output = Result<Vec<SearchResult>>> + Send;
}

/// DuckDuckGo-backed documentation and community search provider.
#[derive(Default)]
pub struct DuckDuckGoDocsProvider {
    client: reqwest::Client,
}

impl DuckDuckGoDocsProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl SearchProvider for DuckDuckGoDocsProvider {
    async fn search(&self, query: &str, sources: &[Source]) -> Result<Vec<SearchResult>> {
        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding(query)
        );

        let res = self
            .client
            .get(&search_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            )
            .send()
            .await?;

        if !res.status().is_success() {
            return Ok(Vec::new());
        }

        let html_body = res.text().await?;
        let document = Html::parse_document(&html_body);

        let result_sel = Selector::parse(".result").unwrap();
        let title_sel = Selector::parse(".result__a").unwrap();
        let snippet_sel = Selector::parse(".result__snippet").unwrap();

        let mut results = Vec::new();

        for element in document.select(&result_sel) {
            let Some(title_elem) = element.select(&title_sel).next() else {
                continue;
            };
            let title = title_elem.text().collect::<String>().trim().to_string();
            let Some(href) = title_elem.value().attr("href") else {
                continue;
            };

            let Some(target_url) = decode_ddg_url(href) else {
                continue;
            };
            let normalized_url = normalize_url(&target_url);

            let snippet = element
                .select(&snippet_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());

            // Identify source ID (matches defined docs/forums, or extracts clean hostname)
            let (source_id, priority_boost) = identify_source_id(&normalized_url, sources);
            let score = 50.0 + priority_boost + title_match_score(&title, query);

            results.push(SearchResult {
                title,
                url: normalized_url,
                source_id,
                snippet,
                score,
                cache_status: CacheStatus::Remote,
            });

            if results.len() >= 50 {
                break;
            }
        }

        Ok(results)
    }
}

/// SQLite FTS5 Document Database.
pub struct DocStore {
    conn: Arc<Mutex<Connection>>,
}

impl DocStore {
    pub fn open_default() -> Result<Self> {
        let base_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("otto");
        std::fs::create_dir_all(&base_dir).ok();
        let db_path = base_dir.join("docs.db");
        Self::open(db_path)
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS documents (
                url TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                title TEXT NOT NULL,
                markdown TEXT NOT NULL,
                etag TEXT,
                last_modified TEXT,
                fetched_at INTEGER NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
                url UNINDEXED,
                title,
                markdown,
                tokenize='porter unicode61'
            );
            ",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn get_document(&self, raw_url: &str) -> Result<Option<CachedDocument>> {
        let normalized = normalize_url(raw_url);
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT url, source_id, title, markdown, etag, last_modified, fetched_at 
             FROM documents WHERE url = ?1",
        )?;
        let mut rows = stmt.query(params![normalized])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CachedDocument {
                url: row.get(0)?,
                source_id: row.get(1)?,
                title: row.get(2)?,
                markdown: row.get(3)?,
                etag: row.get(4)?,
                last_modified: row.get(5)?,
                fetched_at: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn store_document(&self, doc: &CachedDocument) -> Result<()> {
        let normalized = normalize_url(&doc.url);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO documents (url, source_id, title, markdown, etag, last_modified, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                normalized,
                doc.source_id,
                doc.title,
                doc.markdown,
                doc.etag,
                doc.last_modified,
                doc.fetched_at
            ],
        )?;

        // Update FTS index
        conn.execute("DELETE FROM documents_fts WHERE url = ?1", params![normalized])?;
        conn.execute(
            "INSERT INTO documents_fts (url, title, markdown) VALUES (?1, ?2, ?3)",
            params![normalized, doc.title, doc.markdown],
        )?;

        Ok(())
    }

    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let sanitized = sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT d.url, d.source_id, d.title, d.markdown, bm25(documents_fts) AS rank
             FROM documents_fts f
             JOIN documents d ON d.url = f.url
             WHERE documents_fts MATCH ?1
             ORDER BY rank ASC
             LIMIT ?2",
        )?;

        let mut rows = stmt.query(params![sanitized, limit as i64])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            let url: String = row.get(0)?;
            let source_id: String = row.get(1)?;
            let title: String = row.get(2)?;
            let markdown: String = row.get(3)?;
            let bm25_rank: f64 = row.get(4)?;

            let snippet = create_snippet(&markdown, query);
            let score = 100.0 - (bm25_rank as f32 * 10.0).clamp(0.0, 50.0);

            results.push(SearchResult {
                title,
                url,
                source_id,
                snippet: Some(snippet),
                score,
                cache_status: CacheStatus::Cached,
            });
        }

        Ok(results)
    }
}

/// Hybrid search engine combining SQLite FTS5 and remote provider.
pub async fn hybrid_search<P: SearchProvider>(
    query: &str,
    doc_store: &DocStore,
    provider: &P,
    sources: &[Source],
    max_results: usize,
) -> Result<Vec<SearchResult>> {
    let local_results = doc_store.search_fts(query, max_results.min(25)).unwrap_or_default();
    let remote_results = provider.search(query, sources).await.unwrap_or_default();

    let mut merged: Vec<SearchResult> = Vec::new();

    // Add local results
    for local in local_results {
        merged.push(local);
    }

    // Merge remote results with deduplication
    for remote in remote_results {
        if let Some(existing) = merged.iter_mut().find(|r| urls_match(&r.url, &remote.url)) {
            // Keep local cached status, update snippet or score if remote has more info
            if existing.snippet.is_none() {
                existing.snippet = remote.snippet;
            }
            existing.score = existing.score.max(remote.score);
        } else {
            // Check if document exists in DB even if FTS didn't match
            let is_cached = doc_store
                .get_document(&remote.url)
                .ok()
                .flatten()
                .is_some();
            let mut item = remote;
            if is_cached {
                item.cache_status = CacheStatus::Cached;
                item.score += 20.0;
            }
            merged.push(item);
        }
    }

    // Sort deterministically descending by score
    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(max_results);

    Ok(merged)
}

/// Fetch a document conditionally and parse HTML into clean Markdown.
pub async fn fetch_and_process_document(
    url: &str,
    doc_store: &DocStore,
    sources: &[Source],
) -> Result<(CachedDocument, bool)> {
    let normalized = normalize_url(url);

    // Check local database for conditional headers
    let cached = doc_store.get_document(&normalized)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()?;

    let mut req = client.get(&normalized).header(
        "User-Agent",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    );

    if let Some(ref c) = cached {
        if let Some(ref etag) = c.etag {
            req = req.header("If-None-Match", etag);
        }
        if let Some(ref lm) = c.last_modified {
            req = req.header("If-Modified-Since", lm);
        }
    }

    let response = req.send().await?;
    let status = response.status();

    if status == reqwest::StatusCode::NOT_MODIFIED {
        if let Some(c) = cached {
            return Ok((c, true));
        }
    }

    if !status.is_success() {
        if let Some(c) = cached {
            return Ok((c, true));
        }
        anyhow::bail!("Failed to fetch document: HTTP {}", status);
    }

    let etag = response
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let last_modified = response
        .headers()
        .get("Last-Modified")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let html_content = response.text().await?;

    let (title, markdown) = html_to_markdown(&html_content, &normalized);

    let mut source_id = "general".to_string();
    for s in sources {
        if s.matches_url(&normalized) {
            source_id = s.id.clone();
            break;
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let doc = CachedDocument {
        url: normalized,
        source_id,
        title,
        markdown,
        etag,
        last_modified,
        fetched_at: now,
    };

    // Store in SQLite database
    let _ = doc_store.store_document(&doc);

    Ok((doc, false))
}

/// Convert HTML document to clean Markdown for the application renderer.
pub fn html_to_markdown(html_str: &str, base_url: &str) -> (String, String) {
    let document = Html::parse_document(html_str);

    // Title extraction
    let title_sel = Selector::parse("title").unwrap();
    let h1_sel = Selector::parse("h1").unwrap();

    let title = document
        .select(&h1_sel)
        .next()
        .map(|e| e.text().collect::<String>())
        .or_else(|| {
            document
                .select(&title_sel)
                .next()
                .map(|e| e.text().collect::<String>())
        })
        .map(|t| t.trim().to_string())
        .unwrap_or_else(|| "Documentation".to_string());

    // Main content selection
    let content_selectors = [
        "main",
        "article",
        "#content",
        "#main-content",
        ".document",
        ".markdown-body",
        ".docblock",
        ".content",
        "body",
    ];

    let mut main_element = None;
    for sel_str in &content_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(elem) = document.select(&sel).next() {
                main_element = Some(elem);
                break;
            }
        }
    }

    let base_parsed = Url::parse(base_url).ok();

    let mut markdown = String::new();

    if let Some(elem) = main_element {
        render_element_to_markdown(&elem, &mut markdown, &base_parsed, 0);
    } else {
        markdown = strip_html_tags(html_str);
    }

    let cleaned_md = clean_markdown_output(&markdown);
    (title, cleaned_md)
}

fn render_element_to_markdown(
    elem: &scraper::ElementRef,
    out: &mut String,
    base_url: &Option<Url>,
    depth: usize,
) {
    let name = elem.value().name();

    // Skip unwanted tags
    if matches!(
        name,
        "script" | "style" | "noscript" | "svg" | "nav" | "header" | "footer" | "aside" | "iframe"
    ) {
        return;
    }

    // Skip sidebar / ad classes
    if let Some(class) = elem.value().attr("class") {
        if class.contains("sidebar")
            || class.contains("nav")
            || class.contains("menu")
            || class.contains("ad-")
            || class.contains("cookie")
        {
            return;
        }
    }

    match name {
        "h1" => {
            out.push_str("\n\n# ");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "h2" => {
            out.push_str("\n\n## ");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "h3" => {
            out.push_str("\n\n### ");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "h4" | "h5" | "h6" => {
            out.push_str("\n\n#### ");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "p" => {
            out.push_str("\n\n");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "pre" => {
            let code_child = elem.children().find_map(|n| {
                if let Some(el) = n.value().as_element() {
                    if el.name() == "code" {
                        return scraper::ElementRef::wrap(n);
                    }
                }
                None
            });

            let lang = code_child
                .as_ref()
                .and_then(|c| c.value().attr("class"))
                .and_then(|cls| {
                    cls.split_whitespace()
                        .find(|s| s.starts_with("language-") || s.starts_with("lang-"))
                        .map(|s| s.strip_prefix("language-").unwrap_or(s.strip_prefix("lang-").unwrap_or("")))
                })
                .unwrap_or("");

            out.push_str(&format!("\n\n```{lang}\n"));
            let text = elem.text().collect::<String>();
            out.push_str(text.trim());
            out.push_str("\n```\n\n");
        }
        "code" => {
            let text = elem.text().collect::<String>();
            if !text.contains('\n') && !text.trim().is_empty() {
                out.push_str(&format!(" `{}` ", text.trim()));
            } else {
                out.push_str(&text);
            }
        }
        "li" => {
            out.push_str("\n* ");
            render_children(elem, out, base_url, depth + 1);
        }
        "blockquote" => {
            out.push_str("\n\n> ");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "a" => {
            let text = elem.text().collect::<String>().trim().to_string();
            let href = elem.value().attr("href").unwrap_or("");
            if !text.is_empty() && !href.is_empty() && !href.starts_with('#') {
                let full_href = if let Some(base) = base_url {
                    base.join(href).map(|u| u.to_string()).unwrap_or_else(|_| href.to_string())
                } else {
                    href.to_string()
                };
                out.push_str(&format!(" [{text}]({full_href}) "));
            } else {
                render_children(elem, out, base_url, depth);
            }
        }
        "strong" | "b" => {
            out.push_str(" **");
            render_children(elem, out, base_url, depth);
            out.push_str("** ");
        }
        "em" | "i" => {
            out.push_str(" *");
            render_children(elem, out, base_url, depth);
            out.push_str("* ");
        }
        "hr" => {
            out.push_str("\n\n---\n\n");
        }
        "table" => {
            out.push_str("\n\n");
            render_children(elem, out, base_url, depth);
            out.push_str("\n\n");
        }
        "tr" => {
            out.push_str("\n| ");
            render_children(elem, out, base_url, depth);
        }
        "th" | "td" => {
            render_children(elem, out, base_url, depth);
            out.push_str(" | ");
        }
        _ => {
            render_children(elem, out, base_url, depth);
        }
    }
}

fn render_children(
    elem: &scraper::ElementRef,
    out: &mut String,
    base_url: &Option<Url>,
    depth: usize,
) {
    for child in elem.children() {
        if let Some(text) = child.value().as_text() {
            let unescaped = html_escape::decode_html_entities(&text.text);
            out.push_str(&unescaped);
        } else if let Some(el) = scraper::ElementRef::wrap(child) {
            render_element_to_markdown(&el, out, base_url, depth);
        }
    }
}

fn clean_markdown_output(md: &str) -> String {
    let mut cleaned = String::new();
    let mut last_was_empty = false;

    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !last_was_empty {
                cleaned.push('\n');
                last_was_empty = true;
            }
        } else {
            cleaned.push_str(trimmed);
            cleaned.push('\n');
            last_was_empty = false;
        }
    }
    cleaned.trim().to_string()
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    html_escape::decode_html_entities(&out).to_string()
}

/// Normalize URL for cache keying and deduplication.
pub fn normalize_url(raw: &str) -> String {
    if let Ok(mut parsed) = Url::parse(raw) {
        parsed.set_fragment(None);
        let mut clean = parsed.to_string();
        if clean.ends_with('/') {
            clean.pop();
        }
        clean
    } else {
        raw.trim_end_matches('/').to_string()
    }
}

fn urls_match(u1: &str, u2: &str) -> bool {
    normalize_url(u1) == normalize_url(u2)
}

fn decode_ddg_url(href: &str) -> Option<String> {
    if let Some(idx) = href.find("uddg=") {
        let enc = &href[idx + 5..];
        let enc = enc.split('&').next().unwrap_or(enc);
        let decoded = urlencoding_decode(enc);
        Some(decoded)
    } else if href.starts_with("http://") || href.starts_with("https://") {
        Some(href.to_string())
    } else {
        None
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            ' ' => out.push('+'),
            _ => {
                let mut buf = [0; 4];
                for &b in c.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

fn urlencoding_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(hx) = std::str::from_utf8(&b[i + 1..i + 3]) {
                if let Ok(v) = u8::from_str_radix(hx, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn sanitize_fts_query(raw: &str) -> String {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .filter_map(|w| {
            let clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
            if clean.is_empty() {
                None
            } else {
                Some(format!("\"{}\"*", clean))
            }
        })
        .collect();
    tokens.join(" ")
}

fn title_match_score(title: &str, query: &str) -> f32 {
    let title_lower = title.to_lowercase();
    let mut score = 0.0;
    for token in query.to_lowercase().split_whitespace() {
        if title_lower.contains(token) {
            score += 15.0;
        }
    }
    score
}

fn create_snippet(markdown: &str, query: &str) -> String {
    let query_words: Vec<&str> = query.split_whitespace().collect();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with("```") || trimmed.is_empty() {
            continue;
        }
        for word in &query_words {
            if trimmed.to_lowercase().contains(&word.to_lowercase()) {
                return if trimmed.len() > 160 {
                    format!("{}...", &trimmed[..160])
                } else {
                    trimmed.to_string()
                };
            }
        }
    }
    markdown
        .lines()
        .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}

/// Format search results into interactive Markdown for the Search buffer.
pub fn format_search_results_markdown(
    query: &str,
    results: &[SearchResult],
    selected_idx: usize,
) -> String {
    if results.is_empty() {
        return format!("### Documentation Search » `{query}`\n\n*No results found.*");
    }

    let mut md = format!("### Documentation Search » `{query}`\n\n");
    md.push_str("_Use `↑`/`↓` to select a document and press `Enter` to read. Press `Esc` or `b` to go back._\n\n---\n\n");

    for (i, r) in results.iter().enumerate() {
        let is_selected = i == selected_idx;
        let prefix = if is_selected { "▶ " } else { "  " };
        let tag = match r.cache_status {
            CacheStatus::Cached => "`[cached]`",
            CacheStatus::Remote => "`[remote]`",
        };

        let title = if r.title.is_empty() {
            &r.url
        } else {
            &r.title
        };

        md.push_str(&format!(
            "{}**[{}]** **{}** {} *({})*\n",
            prefix,
            i + 1,
            title,
            tag,
            r.source_id
        ));
        md.push_str(&format!("    `{}`\n", r.url));
        if let Some(ref snip) = r.snippet {
            md.push_str(&format!("    > {}\n", snip));
        }
        md.push('\n');
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_normalization() {
        assert_eq!(
            normalize_url("https://docs.rs/tokio/latest/tokio/#overview"),
            "https://docs.rs/tokio/latest/tokio"
        );
        assert_eq!(
            normalize_url("https://docs.rs/tokio/latest/tokio/"),
            "https://docs.rs/tokio/latest/tokio"
        );
    }

    #[test]
    fn test_html_to_markdown() {
        let html = "<html><body><main><h1>Test Heading</h1><p>Hello <b>World</b></p><pre><code>let x = 1;</code></pre></main></body></html>";
        let (title, md) = html_to_markdown(html, "https://docs.rs");
        assert_eq!(title, "Test Heading");
        assert!(md.contains("# Test Heading"));
        assert!(md.contains("**World**"));
        assert!(md.contains("```"));
    }
}

