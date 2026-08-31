//! cht.sh (cheat.sh) API client with persistent disk caching.
//!
//! Provides async methods to fetch root lists (languages/commands),
//! per-topic lists, and cheat sheet documents with automatic `?T` ANSI stripping.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

const CACHE_TTL_DAYS: u64 = 7;
const USER_AGENT: &str = "curl/8.5.0";

/// Structured query representation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChtshPlan {
    pub topic: String,
    #[serde(default)]
    pub query: String,
}

/// Disk cache manager for cht.sh topic and root lists.
#[derive(Debug, Clone)]
pub struct ChtshCache {
    cache_dir: PathBuf,
}

impl ChtshCache {
    pub fn new() -> Self {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("otto")
            .join("chtsh_cache");
        let _ = fs::create_dir_all(&dir);
        Self { cache_dir: dir }
    }

    fn root_cache_path(&self) -> PathBuf {
        self.cache_dir.join("root_list.txt")
    }

    fn topic_cache_path(&self, lang: &str) -> PathBuf {
        let clean = lang.replace(['/', '\\', ':', '.'], "_");
        self.cache_dir.join(format!("topic_{clean}.txt"))
    }

    pub fn get_root_list(&self) -> Option<Vec<String>> {
        let path = self.root_cache_path();
        self.read_valid_cache(&path)
    }

    pub fn save_root_list(&self, list: &[String]) -> Result<()> {
        let path = self.root_cache_path();
        fs::write(&path, list.join("\n"))?;
        Ok(())
    }

    pub fn get_topic_list(&self, lang: &str) -> Option<Vec<String>> {
        let path = self.topic_cache_path(lang);
        self.read_valid_cache(&path)
    }

    pub fn save_topic_list(&self, lang: &str, list: &[String]) -> Result<()> {
        let path = self.topic_cache_path(lang);
        fs::write(&path, list.join("\n"))?;
        Ok(())
    }

    fn read_valid_cache(&self, path: &PathBuf) -> Option<Vec<String>> {
        if !path.exists() {
            return None;
        }
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                    if elapsed > Duration::from_secs(CACHE_TTL_DAYS * 24 * 3600) {
                        return None;
                    }
                }
            }
        }
        let content = fs::read_to_string(path).ok()?;
        let lines: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with(':'))
            .collect();
        if lines.is_empty() {
            None
        } else {
            Some(lines)
        }
    }
}

/// Client to communicate with cht.sh API.
#[derive(Debug, Clone)]
pub struct ChtShClient {
    client: reqwest::Client,
    cache: ChtshCache,
}

impl Default for ChtShClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ChtShClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_default(),
            cache: ChtshCache::new(),
        }
    }

    pub fn cache(&self) -> &ChtshCache {
        &self.cache
    }

    /// Fetch root list (languages and commands) with caching.
    pub async fn fetch_root_list(&self) -> Result<Vec<String>> {
        if let Some(cached) = self.cache.get_root_list() {
            return Ok(cached);
        }

        let url = "https://cht.sh/:list?T";
        let resp = self.client.get(url).send().await.context("fetch root list")?;
        let text = resp.text().await?;
        let list: Vec<String> = text
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with(':'))
            .collect();

        if !list.is_empty() {
            let _ = self.cache.save_root_list(&list);
        }
        Ok(list)
    }

    /// Fetch topic list for a given language/command with caching.
    pub async fn fetch_topic_list(&self, lang: &str) -> Result<Vec<String>> {
        if let Some(cached) = self.cache.get_topic_list(lang) {
            return Ok(cached);
        }

        let url = format!("https://cht.sh/{lang}/:list?T");
        let resp = self.client.get(&url).send().await.context("fetch topic list")?;
        let text = resp.text().await?;
        let list: Vec<String> = text
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with(':'))
            .collect();

        if !list.is_empty() {
            let _ = self.cache.save_topic_list(lang, &list);
        }
        Ok(list)
    }

    /// Fetch a cheat sheet document from cht.sh.
    pub async fn fetch_sheet(&self, lang_or_cmd: &str, query: Option<&str>) -> Result<String> {
        let topic = lang_or_cmd.trim();
        if topic.is_empty() {
            anyhow::bail!("Topic/Scope cannot be empty");
        }

        let url = match query {
            Some(q) if !q.trim().is_empty() => {
                let encoded_q = q.trim().replace(' ', "+");
                format!("https://cht.sh/{topic}/{encoded_q}?T")
            }
            _ => format!("https://cht.sh/{topic}?T"),
        };

        let resp = self.client.get(&url).send().await.context("network request")?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("cht.sh error (HTTP {status}) for {url}");
        }

        let text = resp.text().await?;
        Ok(strip_noise(&text))
    }
}

/// Fuzzy match query against a candidate list and return top N results.
///
/// Uses a process-wide cached `SkimMatcherV2` so we don't rebuild the (expensive)
/// matcher on every keystroke — this was a source of input lag in the cht.sh buffer.
pub fn fuzzy_suggest(candidates: &[String], input: &str, limit: usize) -> Vec<String> {
    if input.trim().is_empty() {
        return candidates.iter().take(limit).cloned().collect();
    }

    static MATCHER: std::sync::OnceLock<SkimMatcherV2> = std::sync::OnceLock::new();
    let matcher = MATCHER.get_or_init(SkimMatcherV2::default);

    let mut scored: Vec<(String, i64)> = candidates
        .iter()
        .filter_map(|c| matcher.fuzzy_match(c, input).map(|score| (c.clone(), score)))
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().take(limit).map(|(s, _)| s).collect()
}

/// Trim the leading "Type to search ..." link banner cht.sh prepends.
fn strip_noise(text: &str) -> String {
    text.lines()
        .skip_while(|l| l.starts_with('<') || l.contains("Type to search"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_suggest_ranks_exact_and_prefix() {
        let candidates = vec![
            "rust".into(),
            "ruby".into(),
            "read_file".into(),
            "regex".into(),
            "python".into(),
        ];
        let res = fuzzy_suggest(&candidates, "ru", 3);
        assert!(!res.is_empty());
        assert!(res.contains(&"rust".to_string()) || res.contains(&"ruby".to_string()));
    }

    #[test]
    fn strips_search_banner() {
        let noise = "<a ...>Type to search</a>\n\n# real content\nhello";
        let out = strip_noise(noise);
        assert_eq!(out, "# real content\nhello");
    }
}
