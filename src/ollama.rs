//! Multi-provider LLM API client supporting Ollama, OpenAI, Groq, Gemini, NVIDIA, and OpenRouter.

use serde::{Deserialize, Serialize};
use crate::event::{AppEvent, JobTx};

/// Model info entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiModelsResponse {
    #[serde(default)]
    pub data: Vec<OpenAiModelItem>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiModelItem {
    pub id: String,
}

/// One message in the chat API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Token accounting plus context metadata, surfaced in the statusline.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TokenCounts {
    #[serde(default)]
    pub prompt_eval_count: u64,
    #[serde(default)]
    pub eval_count: u64,
    #[serde(default, skip_serializing)]
    pub num_ctx: Option<u64>,
}

/// One chunk of a streaming `/api/chat` response (Ollama).
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    pub message: Option<ChatMessage>,
    #[serde(default)]
    pub done: bool,
    #[serde(flatten)]
    pub tokens: TokenCounts,
}

/// Request body for `/api/chat` (Ollama).
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// OpenAI streaming chunk structure.
#[derive(Debug, Deserialize)]
pub struct OpenAiStreamChunk {
    #[serde(default)]
    pub choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiStreamChoice {
    #[serde(default)]
    pub delta: OpenAiDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OpenAiDelta {
    pub content: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OpenAiUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
}

/// Universal LLM Client supporting Ollama and OpenAI-compatible providers.
#[derive(Debug, Clone)]
pub struct Ollama {
    pub provider: String,
    pub url: String,
    pub api_key: Option<String>,
}

impl Ollama {
    pub fn new(provider: String, url: String, api_key: Option<String>) -> Self {
        Self {
            provider,
            url,
            api_key,
        }
    }

    pub fn from_config(config: &crate::config::Config) -> Self {
        let provider = config.server.provider.clone();
        let url = config.resolve_url(&provider);
        let api_key = config.resolve_api_key(&provider);
        Self {
            provider,
            url,
            api_key,
        }
    }

    /// Fetch model list for the current provider.
    pub async fn list_models(&self) -> anyhow::Result<Vec<ModelEntry>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        if self.provider == "ollama" || (!self.url.contains("/v1") && self.url.contains("11434")) {
            let tags_url = format!("{}/api/tags", self.url.trim_end_matches('/'));
            if let Ok(resp) = client.get(&tags_url).send().await {
                if resp.status().is_success() {
                    if let Ok(tags) = resp.json::<TagsResponse>().await {
                        return Ok(tags.models);
                    }
                }
            }
            return Ok(Vec::new());
        } else {
            // OpenAI-compatible /models endpoint
            let base = self.url.trim_end_matches('/');
            let models_url = if base.ends_with("/v1") || base.ends_with("/openai") {
                format!("{base}/models")
            } else {
                format!("{base}/v1/models")
            };

            let mut req = client.get(&models_url);
            if let Some(ref k) = self.api_key {
                req = req.header("Authorization", format!("Bearer {k}"));
            }

            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<OpenAiModelsResponse>().await {
                        if !data.data.is_empty() {
                            return Ok(data
                                .data
                                .into_iter()
                                .map(|m| ModelEntry {
                                    name: m.id,
                                    size: 0,
                                    modified_at: String::new(),
                                })
                                .collect());
                        }
                    }
                }
            }
        }

        // Fallback to static catalog for cloud providers if dynamic fetch is unavailable
        Ok(self.default_catalog())
    }

    pub fn default_catalog(&self) -> Vec<ModelEntry> {
        let models: &[&str] = match self.provider.as_str() {
            "openai" => &["gpt-4o", "gpt-4o-mini", "o3-mini", "o1", "gpt-4-turbo"],
            "groq" => &[
                "llama-3.3-70b-versatile",
                "deepseek-r1-distill-llama-70b",
                "llama-3.1-8b-instant",
                "mixtral-8x7b-32768",
                "gemma2-9b-it",
            ],
            "gemini" => &[
                "gemini-2.0-flash",
                "gemini-1.5-pro",
                "gemini-1.5-flash",
                "gemini-2.0-flash-thinking-exp",
            ],
            "nvidia" => &[
                "nvidia/llama-3.1-nemotron-70b-instruct",
                "nvidia/nemotron-4-340b-instruct",
                "nvidia/llama-3.1-nemotron-51b-instruct",
                "nvidia/nemotron-mini-4b-instruct",
                "meta/llama-3.1-405b-instruct",
                "meta/llama-3.1-70b-instruct",
                "deepseek-ai/deepseek-r1",
                "mistralai/mistral-large-2-instruct",
            ],
            "openrouter" => &[
                "anthropic/claude-3.5-sonnet",
                "deepseek/deepseek-r1",
                "google/gemini-2.0-flash-001",
                "openai/gpt-4o",
                "meta-llama/llama-3.3-70b-instruct",
            ],
            _ => &[],
        };

        models
            .iter()
            .map(|m| ModelEntry {
                name: m.to_string(),
                size: 0,
                modified_at: String::new(),
            })
            .collect()
    }

    /// Stream a chat, forwarding deltas + token counts to the event channel.
    pub async fn stream_chat(
        &self,
        model: &str,
        history: &[ChatMessage],
        tx: &JobTx,
    ) -> anyhow::Result<()> {
        if self.provider == "ollama" || (!self.url.contains("/v1") && self.url.contains("11434")) {
            self.stream_chat_ollama(model, history, tx).await
        } else {
            self.stream_chat_openai(model, history, tx).await
        }
    }

    async fn stream_chat_ollama(
        &self,
        model: &str,
        history: &[ChatMessage],
        tx: &JobTx,
    ) -> anyhow::Result<()> {
        let body = ChatRequest {
            model: model.to_string(),
            messages: history.to_vec(),
            stream: Some(true),
            options: None,
        };

        let chat_url = format!("{}/api/chat", self.url.trim_end_matches('/'));
        let resp = reqwest::Client::new()
            .post(&chat_url)
            .json(&body)
            .send()
            .await?;

        let mut stream = resp.error_for_status()?.bytes_stream();

        use futures_util::StreamExt;
        let mut buf = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line[..line.len() - 1]);
                if line.trim().is_empty() {
                    continue;
                }
                let r: ChatChunk = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if let Some(msg) = &r.message {
                    let _ = tx.send(AppEvent::ChatDelta {
                        buffer: crate::buffers::BufferId::Chat,
                        delta: msg.content.clone(),
                    });
                }
                if r.done {
                    let _ = tx.send(AppEvent::TokenStat {
                        prompt_tokens: r.tokens.prompt_eval_count,
                        eval_tokens: r.tokens.eval_count,
                    });
                    break;
                }
            }
        }
        Ok(())
    }

    async fn stream_chat_openai(
        &self,
        model: &str,
        history: &[ChatMessage],
        tx: &JobTx,
    ) -> anyhow::Result<()> {
        let base = self.url.trim_end_matches('/');
        let chat_url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else if base.ends_with("/v1") || base.ends_with("/openai") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        };

        let body = serde_json::json!({
            "model": model,
            "messages": history,
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        let mut req = reqwest::Client::new().post(&chat_url).json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&err_text) {
                if let Some(msg) = err_json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                    anyhow::bail!("{}: {}", status, msg);
                }
            }
            anyhow::bail!("HTTP {}: {}", status, err_text);
        }

        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;
        let mut buf = Vec::new();
        let mut prompt_toks = 0u64;
        let mut eval_toks = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                let raw_line = String::from_utf8_lossy(&line_bytes);
                let trimmed = raw_line.trim();
                if trimmed.is_empty() || !trimmed.starts_with("data:") {
                    continue;
                }
                let data = trimmed.strip_prefix("data:").unwrap_or("").trim();
                if data == "[DONE]" {
                    break;
                }
                if let Ok(chunk_json) = serde_json::from_str::<OpenAiStreamChunk>(data) {
                    if let Some(choice) = chunk_json.choices.first() {
                        if let Some(ref text) = choice.delta.content {
                            let _ = tx.send(AppEvent::ChatDelta {
                                buffer: crate::buffers::BufferId::Chat,
                                delta: text.clone(),
                            });
                        }
                    }
                    if let Some(usage) = chunk_json.usage {
                        prompt_toks = usage.prompt_tokens;
                        eval_toks = usage.completion_tokens;
                    }
                }
            }
        }

        if prompt_toks > 0 || eval_toks > 0 {
            let _ = tx.send(AppEvent::TokenStat {
                prompt_tokens: prompt_toks,
                eval_tokens: eval_toks,
            });
        }

        Ok(())
    }
}

