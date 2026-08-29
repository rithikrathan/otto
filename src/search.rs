//! Web search: a provider default of DuckDuckGo (free, keyless).
//!
//! Flow: user prompt -> Ollama plans a query -> provider fetches -> (optional)
//! Ollama summarizes into concise markdown. Designed so additional providers
//! (Brave/Tavily) can be added by extending this module.

use serde::{Deserialize, Serialize};

/// A single search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Structured plan the model returns to drive the search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPlan {
    pub query: String,
    #[serde(default)]
    pub provider: String,
}

/// Run a search against the configured provider (only DuckDuckGo for now).
///
/// Parses DuckDuckGo's HTML "lite" results page:
/// - titles: `<a rel="nofollow" class="result__a" href="//.../uddg=ENCODED">Title</a>`
/// - snippets: `<a class="result__snippet">...</a>`
pub async fn search(query: &str) -> anyhow::Result<Vec<SearchResult>> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));
    let body = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 otc/0.1")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let mut results = Vec::new();
    // Split result blocks on titled anchors.
    for block in body
        .split("<a rel=\"nofollow\" class=\"result__a\" href=\"")
        .skip(1)
    {
        let Some((href, rest)) = block.split_once('"') else {
            continue;
        };
        let Some(url) = decode_link(href) else {
            continue;
        };
        let title = strip_tags(rest.split("</a>").next().unwrap_or(""));
        let snippet = extract_snippet(rest);
        if title.is_empty() && url.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title,
            url,
            snippet,
        });
        if results.len() >= 8 {
            break;
        }
    }
    Ok(results)
}

/// Decode a DDG result href (a redirect to `uddg=<percent-encoded>`).
fn decode_link(href: &str) -> Option<String> {
    let h = html_unescape(href);
    if let Some(idx) = h.find("uddg=") {
        let enc = &h[idx + 5..];
        // strip trailing redirect params
        let enc = enc.split('&').next().unwrap_or(enc);
        Some(percent_decode(enc))
    } else if h.starts_with("http://") || h.starts_with("https://") {
        Some(h)
    } else {
        None
    }
}

/// Minimal percent-decoding (single-byte, sufficient for URLs).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(hx) = hex {
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

/// Format search results as a concise markdown list.
pub fn results_to_markdown(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "*No results.*".to_string();
    }
    let mut md = String::new();
    for (i, r) in results.iter().enumerate() {
        md.push_str(&format!(
            "{}. [{}]({})\n   {}\n\n",
            i + 1,
            if r.title.is_empty() {
                r.url.clone()
            } else {
                r.title.clone()
            },
            r.url,
            r.snippet
        ));
    }
    md
}

fn urlencoding(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join("+")
}

fn extract_snippet(rest: &str) -> String {
    // rest starts at `>Title</a>...`. Everything after the title's closing
    // `</a>` contains the URL extras div and the `<a class="result__snippet">`.
    // Use split_once so we keep the whole tail, not just up to the next `</a>`.
    let after = rest.split_once("</a>").map(|(_, r)| r).unwrap_or(rest);
    let snippet = after
        .split("result__snippet")
        .nth(1)
        .and_then(|s| s.split('>').nth(1))
        .and_then(|s| s.split("</a>").next())
        .unwrap_or("");
    let clean = strip_tags(snippet);
    if clean.is_empty() {
        strip_tags(after)
    } else {
        clean
    }
}

fn strip_tags(s: &str) -> String {
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
    html_unescape(&out).trim().to_string()
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_format_as_numbered_markdown() {
        let r = vec![SearchResult {
            title: "Example".into(),
            url: "https://example.com".into(),
            snippet: "A snippet".into(),
        }];
        let md = results_to_markdown(&r);
        assert!(md.contains("1. [Example](https://example.com)"));
        assert!(md.contains("A snippet"));
    }

    #[test]
    fn empty_results_message() {
        assert!(results_to_markdown(&[]).contains("No results"));
    }

    #[test]
    fn decodes_uddg_link() {
        let href =
            "//duckduckgo.com/l/?uddg=https%3A%2F%2Fdoc.rust%2Dlang.org%2Fcargo%2F&amp;rut=12";
        assert_eq!(
            decode_link(href).as_deref(),
            Some("https://doc.rust-lang.org/cargo/")
        );
    }
}
