//! Persistent configuration, loaded from the platform config dir.

/// App config; every field has a sane default so the file is optional.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Config {
    pub server: Server,
    pub model: Model,
    pub search: Search,
    pub providers: ProvidersConfig,
}

/// `~/.config/otto/config.toml`
pub fn config_path() -> anyhow::Result<std::path::PathBuf> {
    let dir =
        dirs::config_dir().ok_or_else(|| anyhow::anyhow!("could not determine config dir"))?;
    Ok(dir.join("otto").join("config.toml"))
}

impl Config {
    /// Load from disk or return default config.
    pub fn load() -> anyhow::Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&raw)?)
    }

    /// Save current config.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Resolve effective API key for a provider from config or environment variables.
    pub fn resolve_api_key(&self, provider: &str) -> Option<String> {
        if self.server.provider == provider {
            if let Some(ref k) = self.server.api_key {
                if !k.is_empty() {
                    return Some(k.clone());
                }
            }
        }

        let entry = match provider {
            "openai" => &self.providers.openai,
            "groq" => &self.providers.groq,
            "gemini" => &self.providers.gemini,
            "nvidia" => &self.providers.nvidia,
            "openrouter" => &self.providers.openrouter,
            "custom" => &self.providers.custom,
            _ => &self.providers.ollama,
        };

        if let Some(ref k) = entry.api_key {
            if !k.is_empty() {
                return Some(k.clone());
            }
        }

        // Fallback to environment variables
        match provider {
            "openai" => std::env::var("OPENAI_API_KEY").ok(),
            "groq" => std::env::var("GROQ_API_KEY").ok(),
            "gemini" => std::env::var("GEMINI_API_KEY").or_else(|_| std::env::var("GOOGLE_API_KEY")).ok(),
            "nvidia" => std::env::var("NVIDIA_API_KEY").or_else(|_| std::env::var("NIM_API_KEY")).ok(),
            "openrouter" => std::env::var("OPENROUTER_API_KEY").ok(),
            _ => None,
        }
    }

    /// Resolve effective URL for a provider.
    pub fn resolve_url(&self, provider: &str) -> String {
        if self.server.provider == provider && !self.server.url.is_empty() {
            return self.server.url.clone();
        }

        match provider {
            "openai" => self.providers.openai.url.clone(),
            "groq" => self.providers.groq.url.clone(),
            "gemini" => self.providers.gemini.url.clone(),
            "nvidia" => self.providers.nvidia.url.clone(),
            "openrouter" => self.providers.openrouter.url.clone(),
            "custom" => self.providers.custom.url.clone(),
            _ => self.providers.ollama.url.clone(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: Server::default(),
            model: Model::default(),
            search: Search::default(),
            providers: ProvidersConfig::default(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Server {
    pub provider: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Model {
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Search {
    pub provider: String,
    pub summarize: bool,
    pub max_results: usize,
    pub custom_sources: Vec<SourceConfig>,
}

/// Custom documentation source configuration.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub url_prefixes: Vec<String>,
    #[serde(default = "default_source_priority")]
    pub priority: u32,
}

fn default_source_priority() -> u32 {
    80
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub ollama: ProviderEntry,
    pub openai: ProviderEntry,
    pub groq: ProviderEntry,
    pub gemini: ProviderEntry,
    pub nvidia: ProviderEntry,
    pub openrouter: ProviderEntry,
    pub custom: ProviderEntry,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderEntry {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub models: Vec<String>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            url: "http://localhost:11434".to_string(),
            api_key: None,
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Self {
            name: "qwen2.5-coder-1.5b:latest".to_string(),
        }
    }
}

impl Default for Search {
    fn default() -> Self {
        Self {
            provider: "duckduckgo".to_string(),
            summarize: true,
            max_results: 40,
            custom_sources: Vec::new(),
        }
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            ollama: ProviderEntry {
                url: "http://localhost:11434".to_string(),
                api_key: None,
                default_model: "qwen2.5-coder-1.5b:latest".to_string(),
                models: vec![],
            },
            openai: ProviderEntry {
                url: "https://api.openai.com/v1".to_string(),
                api_key: None,
                default_model: "gpt-4o-mini".to_string(),
                models: vec![
                    "gpt-4o".to_string(),
                    "gpt-4o-mini".to_string(),
                    "o3-mini".to_string(),
                    "o1".to_string(),
                    "gpt-4-turbo".to_string(),
                ],
            },
            groq: ProviderEntry {
                url: "https://api.groq.com/openai/v1".to_string(),
                api_key: None,
                default_model: "llama-3.3-70b-versatile".to_string(),
                models: vec![
                    "llama-3.3-70b-versatile".to_string(),
                    "deepseek-r1-distill-llama-70b".to_string(),
                    "llama-3.1-8b-instant".to_string(),
                    "mixtral-8x7b-32768".to_string(),
                    "gemma2-9b-it".to_string(),
                ],
            },
            gemini: ProviderEntry {
                url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
                api_key: None,
                default_model: "gemini-2.0-flash".to_string(),
                models: vec![
                    "gemini-2.0-flash".to_string(),
                    "gemini-1.5-pro".to_string(),
                    "gemini-1.5-flash".to_string(),
                    "gemini-2.0-flash-thinking-exp".to_string(),
                ],
            },
            nvidia: ProviderEntry {
                url: "https://integrate.api.nvidia.com/v1".to_string(),
                api_key: None,
                default_model: "meta/llama-3.1-405b-instruct".to_string(),
                models: vec![
                    "meta/llama-3.1-405b-instruct".to_string(),
                    "meta/llama-3.1-70b-instruct".to_string(),
                    "deepseek-ai/deepseek-r1".to_string(),
                    "nvidia/nemotron-4-340b-instruct".to_string(),
                ],
            },
            openrouter: ProviderEntry {
                url: "https://openrouter.ai/api/v1".to_string(),
                api_key: None,
                default_model: "anthropic/claude-3.5-sonnet".to_string(),
                models: vec![
                    "anthropic/claude-3.5-sonnet".to_string(),
                    "deepseek/deepseek-r1".to_string(),
                    "google/gemini-2.0-flash-001".to_string(),
                    "openai/gpt-4o".to_string(),
                    "meta-llama/llama-3.3-70b-instruct".to_string(),
                ],
            },
            custom: ProviderEntry {
                url: "http://localhost:8000/v1".to_string(),
                api_key: None,
                default_model: "default".to_string(),
                models: vec![],
            },
        }
    }
}

