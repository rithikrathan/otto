//! DuckDuckGo Instant Answer client.
//!
//! Uses the official (keyless) Instant Answer API and renders the result as
//! markdown for display in the ddg buffer.

use anyhow::{Context, Result};
use serde::Deserialize;

const API_URL: &str = "https://api.duckduckgo.com/";

/// Raw JSON shape returned by the Instant Answer API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InstantAnswer {
    pub heading: Option<String>,
    #[serde(default)]
    pub answer: Answer,
    #[serde(default)]
    pub answer_type: Option<String>,
    #[serde(default)]
    pub abstract_text: Option<String>,
    #[serde(default, rename = "AbstractURL")]
    pub abstract_url: Option<String>,
    #[serde(default)]
    pub definition: Option<String>,
    #[serde(default, rename = "DefinitionURL")]
    pub definition_url: Option<String>,
    #[serde(default, rename = "Type")]
    pub answer_kind: Option<String>,
    #[serde(default)]
    pub results: Vec<AbstractResult>,
    #[serde(default)]
    pub related_topics: Vec<RelatedTopic>,
}

/// The `Answer` field is a string for direct answers (calc, ip, random) but can
/// be a JSON object for widget-style answers (currency conversion, sqrt, etc.).
/// We only surface string answers; widget answers are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
pub enum Answer {
    Text(String),
    Widget(serde_json::Value),
    #[default]
    None,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AbstractResult {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default, rename = "FirstURL")]
    pub first_url: Option<String>,
}

/// A related topic is either a leaf entry or a nested category with `topics`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RelatedTopic {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default, rename = "FirstURL")]
    pub first_url: Option<String>,
    #[serde(default)]
    pub topics: Vec<RelatedTopic>,
}

fn clean_abstract(text: &str) -> String {
    text.replace("(company)", "").trim().to_string()
}

fn clean_result_text(text: &str) -> String {
    let t = text.trim();
    let lower = t.to_lowercase();
    if lower.starts_with("(disambiguation)") {
        t.trim_start_matches("(disambiguation)").trim().to_string()
    } else {
        t.to_string()
    }
}

/// Render an Instant Answer as markdown.
pub fn render_markdown(query: &str, ia: &InstantAnswer) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {query}\n\n"));

    if let Answer::Text(a) = &ia.answer {
        if !a.trim().is_empty() {
            md.push_str(a.trim());
            md.push_str("\n\n");
        }
    }

    if let Some(text) = &ia.abstract_text {
        let clean = clean_abstract(text);
        if !clean.is_empty() {
            if let Some(url) = &ia.abstract_url {
                md.push_str(&format!("[{clean}]({url})\n\n"));
            } else {
                md.push_str(&format!("{clean}\n\n"));
            }
        }
    }

    if let Some(def) = &ia.definition {
        if !def.trim().is_empty() {
            md.push_str("**Definition:** ");
            md.push_str(def.trim());
            if let Some(url) = &ia.definition_url {
                md.push_str(&format!(" — [{url}]({url})"));
            }
            md.push_str("\n\n");
        }
    }

    if !ia.results.is_empty() {
        md.push_str("## Results\n\n");
        for r in &ia.results {
            if let Some(text) = &r.text {
                let clean = clean_result_text(text);
                if !clean.is_empty() {
                    if let Some(url) = &r.first_url {
                        md.push_str(&format!("- [{clean}]({url})\n"));
                    } else {
                        md.push_str(&format!("- {clean}\n"));
                    }
                }
            }
        }
        md.push('\n');
    }

    let mut flat_topics: Vec<&RelatedTopic> = Vec::new();
    fn collect<'a>(topics: &'a [RelatedTopic], out: &mut Vec<&'a RelatedTopic>) {
        for t in topics {
            if !t.topics.is_empty() {
                collect(&t.topics, out);
            } else if t.text.is_some() {
                out.push(t);
            }
        }
    }
    collect(&ia.related_topics, &mut flat_topics);
    if !flat_topics.is_empty() {
        md.push_str("## Related\n\n");
        for t in flat_topics {
            let clean = clean_result_text(t.text.as_deref().unwrap_or(""));
            if !clean.is_empty() {
                if let Some(url) = &t.first_url {
                    md.push_str(&format!("- [{clean}]({url})\n"));
                } else {
                    md.push_str(&format!("- {clean}\n"));
                }
            }
        }
        md.push('\n');
    }

    if md.trim().is_empty() || md.trim() == format!("# {query}") {
        md = format!(
            "# {query}\n\n\
             **No instant answer found.**\n\n\
             DuckDuckGo's Instant Answer only covers named topics (people, places, \
             things), definitions, and calculations — not general web search.\n\n\
             Try a specific name or topic, e.g. `Nikola Tesla`, `photosynthesis`, \
             `rust programming language`, or a calculation like `1 mile in km`.\n"
        );
    }

    md
}

/// Fetch and render the Instant Answer for `query`.
pub async fn answer(query: &str) -> Result<String> {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, REFERER, USER_AGENT};
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        ),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://duckduckgo.com/"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client
        .get(API_URL)
        .query(&[
            ("q", query.trim()),
            ("format", "json"),
            ("no_html", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await
        .context("network request to DuckDuckGo")?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!(
            "DuckDuckGo blocked the request (HTTP {status}). Try again in a moment."
        );
    }
    let ia: InstantAnswer =
        serde_json::from_slice(&resp.bytes().await.context("read response body")?)
            .context("parse DuckDuckGo response")?;
    Ok(render_markdown(query, &ia))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_related_topics() {
        let raw = r#"{
            "Heading": "Docker",
            "AbstractText": "",
            "AbstractURL": "https://en.wikipedia.org/wiki/Docker",
            "Results": [],
            "RelatedTopics": [
                {
                    "FirstURL": "https://duckduckgo.com/Docker_(software)",
                    "Text": "Docker (software) A set of products that uses operating system-level virtualization",
                    "Result": "<a>Docker (software)</a>A set of products"
                },
                {
                    "Name": "Brands",
                    "Topics": [
                        {
                            "FirstURL": "https://duckduckgo.com/Dockers_(brand)",
                            "Text": "Dockers (brand) An American brand of garments"
                        }
                    ]
                }
            ]
        }"#;
        let ia: InstantAnswer = serde_json::from_str(raw).expect("parse");
        assert_eq!(ia.heading.as_deref(), Some("Docker"));
        let md = render_markdown("docker", &ia);
        assert!(md.contains("# docker"));
        assert!(md.contains("Docker (software)"));
        assert!(md.contains("Related"));
        assert!(md.contains("Dockers (brand)"));
    }

    #[test]
    fn renders_definition_and_results() {
        let raw = r#"{
            "Heading": "RFC",
            "AbstractText": "",
            "Definition": "Request for Comments",
            "DefinitionURL": "https://duckduckgo.com/RFC",
            "Results": [
                {"FirstURL": "https://www.rfc-editor.org/", "Text": "RFC Editor official site"}
            ],
            "RelatedTopics": []
        }"#;
        let ia: InstantAnswer = serde_json::from_str(raw).expect("parse");
        let md = render_markdown("rfc", &ia);
        assert!(md.contains("Request for Comments"));
        assert!(md.contains("RFC Editor"));
        assert!(md.contains("https://www.rfc-editor.org/"));
    }

    #[test]
    fn parses_real_docker_response_structure() {
        let raw = r#"{
            "Heading": "Docker",
            "AbstractText": "",
            "AbstractURL": "https://en.wikipedia.org/wiki/Docker",
            "Answer": "",
            "AnswerType": "",
            "Definition": "",
            "DefinitionURL": "",
            "Results": [],
            "RelatedTopics": [
                {
                    "FirstURL": "https://duckduckgo.com/Docker_(software)",
                    "Icon": {"URL": "/i/d8d7d296.png"},
                    "Result": "<a href=\"https://duckduckgo.com/Docker_(software)\">Docker (software)</a>A set of products",
                    "Text": "Docker (software) A set of products that uses operating system-level virtualization"
                },
                {
                    "Name": "Places",
                    "Topics": [
                        {
                            "FirstURL": "https://duckduckgo.com/Docker%2C_Victoria",
                            "Text": "Docker, Victoria A town in Victoria, Australia"
                        }
                    ]
                }
            ]
        }"#;
        let ia: InstantAnswer = serde_json::from_str(raw).expect("parse real-shape JSON");
        assert_eq!(ia.heading.as_deref(), Some("Docker"));
        assert_eq!(ia.abstract_url.as_deref(), Some("https://en.wikipedia.org/wiki/Docker"));
        let md = render_markdown("docker", &ia);
        // Both top-level and nested-group topics are rendered in Related.
        assert!(md.contains("Docker (software)"));
        assert!(md.contains("Docker, Victoria"));
    }

    #[test]
    fn tolerates_widget_answer_and_renders_empty_state() {
        // `Answer` can be a JSON object for calculator/converter widgets.
        let raw = r#"{
            "Heading": "",
            "AbstractText": "",
            "Answer": {"from": "calculator", "id": "calculator", "result": ""},
            "AnswerType": "calculator",
            "RelatedTopics": [],
            "Results": []
        }"#;
        let ia: InstantAnswer = serde_json::from_str(raw).expect("widget Answer must not fail parse");
        assert!(matches!(ia.answer, Answer::Widget(_)));
        let md = render_markdown("1 mile in km", &ia);
        // No body content -> helpful empty state mentioning scope + examples.
        assert!(md.contains("No instant answer found"));
        assert!(md.contains("Nikola Tesla"));
        assert!(md.contains("1 mile in km"));
    }

    #[tokio::test]
    #[ignore = "hits the live Du"]
    async fn live_endpoint_returns_renderable_markdown() {
        // Not run in the default suite; verifies live network behaviour on demand.
        match answer("Nikola Tesla").await {
            Ok(md) => assert!(!md.trim().is_empty()),
            Err(e) => {
                // Rate-limited environments may block; that's a tolerated outcome.
                assert!(e.to_string().contains("blocked"));
            }
        }
    }
}
