//! Ollama API client (localhost:11434).

use serde::{Deserialize, Serialize};

use crate::event::{AppEvent, JobTx};

/// Ollama model info from `/api/tags`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<ModelEntry>,
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
    /// Tokens sent to the model (reading) for this request.
    #[serde(default)]
    pub prompt_eval_count: u64,
    /// Tokens generated (writing) for this request.
    #[serde(default)]
    pub eval_count: u64,
    /// Model context window size, when reported.
    #[serde(default, skip_serializing)]
    pub num_ctx: Option<u64>,
}

/// One chunk of a streaming `/api/chat` response.
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    pub message: Option<ChatMessage>,
    pub done: bool,
    #[serde(flatten)]
    pub tokens: TokenCounts,
}

/// Request body for `/api/chat`.
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// Client bound to a server URL.
#[derive(Debug, Clone)]
pub struct Ollama {
    pub url: String,
}

impl Ollama {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    /// Fetch the list of installed models.
    pub async fn list_models(&self) -> anyhow::Result<Vec<ModelEntry>> {
        let resp = reqwest::Client::new()
            .get(format!("{}/api/tags", self.url))
            .send()
            .await?;
        let tags: TagsResponse = resp.error_for_status()?.json().await?;
        Ok(tags.models)
    }

    /// Stream a chat, forwarding deltas + token counts to the event channel.
    ///
    /// Emits `AppEvent::ChatDelta` per chunk and `AppEvent::TokenStat` at the
    /// end (the final `done` chunk carries the cumulative counts).
    pub async fn stream_chat(
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

        let resp = reqwest::Client::new()
            .post(format!("{}/api/chat", self.url))
            .json(&body)
            .send()
            .await?;
        let mut stream = resp.error_for_status()?.bytes_stream();

        use futures_util::StreamExt;
        let mut buf = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            // Ollama sends NDJSON lines; accumulate and split on newlines.
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line[..line.len() - 1]);
                if line.trim().is_empty() {
                    continue;
                }
                let r: ChatChunk = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(_) => continue, // tolerate partial/whitespace lines
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

    /// Single-shot completion (stream=false) returning full response incl. tokens.
    pub async fn complete(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> anyhow::Result<ChatChunk> {
        let body = ChatRequest {
            model: model.to_string(),
            messages,
            stream: Some(false),
            options: None,
        };
        let resp = reqwest::Client::new()
            .post(format!("{}/api/chat", self.url))
            .json(&body)
            .send()
            .await?;
        let r: ChatChunk = resp.error_for_status()?.json().await?;
        Ok(r)
    }
}