//! Wikipedia quick-lookup client (keyless, no API key).
//!
//! Uses two free MediaWiki endpoints:
//! - `action=opensearch` — resolves an arbitrary free-text query (misspellings
//!   included) into a ranked list of matching article titles + URLs.
//! - `action=query&prop=extracts` — fetches the plain-text lead section of the
//!   chosen article (abstract). A better, MathJax-friendly alternative is the
//!   REST summary endpoint; we use the plain-text `exintro` extract here so the
//!   result renders cleanly in the markdown buffer.
//!
//! These complement DuckDuckGo Instant Answer: DDG covers named entities /
//! definitions / calculations, while Wikipedia covers the long tail of general
//! topics with an abstract and deep links.

use anyhow::{Context, Result};
use serde::Deserialize;

const SEARCH_API: &str = "https://en.wikipedia.org/w/api.php";
const CLIENT_UA: &str = concat!("otto/1.0 (TUI chat client; contact: local)");

/// A single candidate article returned by opensearch.
#[derive(Debug, Clone)]
pub struct WikiHit {
    pub title: String,
    pub url: String,
}

/// Lead-section plain-text extract for a resolved article.
#[derive(Debug, Clone)]
pub struct WikiSummary {
    pub title: String,
    pub extract: String,
    pub url: String,
}

/// opensearch returns a JSON array: [query, titles[], descriptions[], urls[]].
#[derive(Debug, Deserialize)]
struct OpenSearch {
    #[allow(dead_code)]
    query: String,
    titles: Vec<String>,
    #[allow(dead_code)]
    descriptions: Vec<String>,
    urls: Vec<String>,
}

/// `action=query&prop=extracts&exintro` response shape.
#[derive(Debug, Deserialize)]
struct Extracts {
    #[allow(dead_code)]
    query: ExtractsQuery,
}

#[derive(Debug, Deserialize)]
struct ExtractsQuery {
    #[serde(rename = "pages")]
    pages: Vec<ExtractPage>,
}

#[derive(Debug, Deserialize)]
struct ExtractPage {
    #[serde(rename = "title")]
    title: String,
    #[serde(rename = "extract", default)]
    extract: String,
}

fn http_client() -> Result<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(CLIENT_UA));
    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(15))
        .build()?)
}

/// Resolve a free-text query to candidate article titles via opensearch.
pub async fn search(query: &str) -> Result<Vec<WikiHit>> {
    let client = http_client()?;
    let resp = client
        .get(SEARCH_API)
        .query(&[
            ("action", "opensearch"),
            ("search", query.trim()),
            ("limit", "8"),
            ("namespace", "0"),
            ("format", "json"),
        ])
        .send()
        .await
        .context("network request to Wikipedia search")?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("Wikipedia search failed (HTTP {status}). Try again in a moment.");
    }
    let os: OpenSearch = serde_json::from_slice(&resp.bytes().await.context("read body")?)
        .context("parse Wikipedia opensearch response")?;
    Ok(os.titles
        .into_iter()
        .zip(os.urls.into_iter())
        .map(|(title, url)| WikiHit { title, url })
        .collect())
}

/// Fetch the plain-text lead-section abstract for an exact title.
pub async fn summary(title: &str) -> Result<WikiSummary> {
    let client = http_client()?;
    let resp = client
        .get(SEARCH_API)
        .query(&[
            ("action", "query"),
            ("prop", "extracts"),
            ("titles", title),
            ("exintro", "1"),
            ("explaintext", "1"),
            ("redirects", "1"),
            ("format", "json"),
            ("formatversion", "2"),
        ])
        .send()
        .await
        .context("network request to Wikipedia extract")?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("Wikipedia extract failed (HTTP {status}). Try again in a moment.");
    }
    let ex: Extracts = serde_json::from_slice(&resp.bytes().await.context("read body")?)
        .context("parse Wikipedia extract response")?;
    let page = match ex.query.pages.iter().find(|p| !p.extract.is_empty()) {
        Some(p) => p,
        None => anyhow::bail!("No Wikipedia article found for '{title}'."),
    };
    let url = format!(
        "https://en.wikipedia.org/wiki/{}",
        page.title.replace(' ', "_")
    );
    Ok(WikiSummary {
        title: page.title.clone(),
        extract: page.extract.clone(),
        url,
    })
}

/// Render resolved search hits + abstract as markdown for the wiki buffer.
pub fn render_markdown(query: &str, hits: &[WikiHit], summary: Option<&WikiSummary>) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {query}\n\n"));

    if let Some(s) = summary {
        if !s.title.is_empty() {
            // Prominent, clickable title link (opens the full article).
            md.push_str(&format!("[{t}]({u})\n\n", t = s.title, u = s.url));
        }
        let extract = s.extract.trim();
        if !extract.is_empty() {
            md.push_str(extract);
            md.push_str(&format!(
                "\n\nSource: [{t}]({u})\n\n",
                t = s.title,
                u = s.url
            ));
        }
    }

    if !hits.is_empty() {
        md.push_str("## More articles\n\n");
        for h in hits {
            md.push_str(&format!("- [{t}]({u})\n", t = h.title, u = h.url));
        }
        md.push('\n');
    }

    if md.trim() == format!("# {query}") {
        md = format!(
            "# {query}\n\n\
             **No Wikipedia article found.**\n\n\
             Try a different spelling or a more specific topic, e.g. \
             `navier strokes`, `quantum computing`, or `rust programming language`.\n"
        );
    }

    md
}

/// Full lookup: resolve the query to top hits, fetch the abstract, render.
pub async fn lookup(query: &str) -> Result<String> {
    let hits = search(query).await?;
    if hits.is_empty() {
        return Ok(render_markdown(query, &[], None));
    }
    // Fetch the abstract for the top hit (the best fuzzy match).
    let mut result: Option<WikiSummary> = None;
    for hit in &hits {
        if let Ok(s) = summary(&hit.title).await {
            if !s.extract.trim().is_empty() {
                result = Some(s);
                break;
            }
        }
    }
    Ok(render_markdown(query, &hits, result.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opensearch_shape() {
        let raw = r#"[
          "navier strokes",
          ["Navier–Stokes equations","Navier-Stokes equations/Derivation"],
          ["",""],
          ["https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations","https://en.wikipedia.org/wiki/Navier-Stokes_equations/Derivation"]
        ]"#;
        let os: OpenSearch = serde_json::from_str(raw).expect("parse");
        assert_eq!(os.titles.len(), 2);
        assert!(os.titles[0].contains("Navier"));
    }

    #[test]
    fn parses_extract_shape() {
        let raw = r#"{
          "query": {
            "pages": [
              {
                "pageid": 1,
                "ns": 0,
                "title": "Navier–Stokes equations",
                "extract": "The Navier–Stokes equations describe the motion of viscous fluids."
              }
            ]
          }
        }"#;
        let ex: Extracts = serde_json::from_str(raw).expect("parse");
        assert_eq!(ex.query.pages.len(), 1);
        assert!(ex.query.pages[0].extract.contains("motion"));
    }

    #[test]
    fn renders_hit_list_with_abstract() {
        let hits = vec![WikiHit {
            title: "Navier–Stokes equations".into(),
            url: "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations".into(),
        }];
        let s = WikiSummary {
            title: "Navier–Stokes equations".into(),
            extract: "The Navier–Stokes equations describe the motion of viscous fluids.".into(),
            url: "https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations".into(),
        };
        let md = render_markdown("navier strokes", &hits, Some(&s));
        assert!(md.contains("# navier strokes"));
        assert!(md.contains("navier strokes"));
        assert!(md.contains("motion of viscous fluids"));
        assert!(md.contains("More articles"));
        assert!(md.contains("https://en.wikipedia.org/wiki/Navier"));
    }

    #[test]
    fn renders_empty_state_when_no_abstract() {
        let hits = vec![WikiHit {
            title: "Something".into(),
            url: "https://en.wikipedia.org/wiki/Something".into(),
        }];
        let md = render_markdown("zzz nothing", &hits, None);
        assert!(md.contains("More articles"));
        assert!(md.contains("Something"));
        assert!(!md.contains("No Wikipedia article found"));
    }

    #[test]
    fn renders_helpful_empty_state_for_no_hits() {
        let md = render_markdown("qqqqxzz", &[], None);
        assert!(md.contains("No Wikipedia article found"));
        assert!(md.contains("navier strokes"));
        assert!(md.contains("quantum computing"));
    }

    #[tokio::test]
    #[ignore = "hits the live Wikipedia API"]
    async fn live_navier_stokes_resolves() {
        let md = lookup("navier strokes").await.expect("lookup");
        assert!(md.contains("More articles"));
        assert!(md.contains("https://en.wikipedia.org/wiki/"));
    }

    #[test]
    fn summary_url_uses_underscores() {
        let s = WikiSummary {
            title: "Quantum Computing".into(),
            extract: "x".into(),
            url: "https://en.wikipedia.org/wiki/Quantum_Computing".into(),
        };
        let md = render_markdown("q", &[], Some(&s));
        assert!(md.contains("Quantum_Computing"));
    }
}
