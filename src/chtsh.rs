//! cht.sh cheatsheet fetching.
//!
//! Ollama builds a plan `{topic, query}` and the app constructs the URL:
//! `https://cht.sh/<topic>/<query with '+' separators>` (with `?T` for plain text).

use serde::{Deserialize, Serialize};

/// Structured plan the model returns to drive a cht.sh query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChtshPlan {
    pub topic: String,
    #[serde(default)]
    pub query: String,
}

/// Build a cht.sh URL from a topic + query.
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

pub fn build_url(plan: &ChtshPlan) -> String {
    let topic = plan.topic.trim().to_lowercase();
    let joined = urlencoding(&plan.query);
    format!("https://cht.sh/{}/{}?T", urlencoding(&topic), joined)
}

/// Fetch the cheatsheet text for a query and return the raw body.
pub async fn fetch(url: &str) -> anyhow::Result<String> {
    let resp = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "curl/8.5.0")
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("cht.sh returned {} for {url}", status);
    }
    let text = resp.text().await?;
    // cht.sh pages include an HTML header line; strip obvious noise.
    Ok(strip_noise(&text))
}

/// Trim the leading "Type to search ..." link banner cht.sh prepends.
fn strip_noise(text: &str) -> String {
    text.lines()
        .skip_while(|l| l.starts_with("<") || l.contains("Type to search"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_url_with_joined_query() {
        let plan = ChtshPlan {
            topic: "Rust".into(),
            query: "read file lines".into(),
        };
        assert_eq!(build_url(&plan), "https://cht.sh/rust/read+file+lines?T");
    }

    #[test]
    fn strips_search_banner() {
        let noise = "<a ...>Type to search</a>\n\n# real content\nhello";
        let out = strip_noise(noise);
        assert_eq!(out, "# real content\nhello");
    }
}
