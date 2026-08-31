//! Persistent configuration, loaded from the platform config dir.

/// App config; every field has a sane default so the file is optional.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Config {
    pub server: Server,
    pub model: Model,
    pub system_prompt: String,
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
        let entry_key = match provider {
            "groq" => self.providers.groq.api_key.as_ref(),
            "gemini" => self.providers.gemini.api_key.as_ref(),
            "nvidia" => self.providers.nvidia.api_key.as_ref(),
            _ => self.providers.ollama.api_key.as_ref(),
        };

        if let Some(k) = entry_key {
            if !k.trim().is_empty() {
                return Some(k.trim().to_string());
            }
        }

        if self.server.provider == provider {
            if let Some(ref k) = self.server.api_key {
                if !k.trim().is_empty() {
                    return Some(k.trim().to_string());
                }
            }
        }

        // Fallback to environment variables
        match provider {
            "groq" => std::env::var("GROQ_API_KEY").ok(),
            "gemini" => std::env::var("GEMINI_API_KEY").or_else(|_| std::env::var("GOOGLE_API_KEY")).ok(),
            "nvidia" => std::env::var("NVIDIA_API_KEY").or_else(|_| std::env::var("NIM_API_KEY")).ok(),
            _ => None,
        }
    }

    /// Resolve effective URL for a provider.
    pub fn resolve_url(&self, provider: &str) -> String {
        match provider {
            "groq" => {
                if !self.providers.groq.url.is_empty() {
                    self.providers.groq.url.clone()
                } else {
                    "https://api.groq.com/openai/v1".to_string()
                }
            }
            "gemini" => {
                if !self.providers.gemini.url.is_empty() {
                    self.providers.gemini.url.clone()
                } else {
                    "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
                }
            }
            "nvidia" => {
                if !self.providers.nvidia.url.is_empty() {
                    self.providers.nvidia.url.clone()
                } else {
                    "https://integrate.api.nvidia.com/v1".to_string()
                }
            }
            _ => {
                if !self.providers.ollama.url.is_empty() {
                    self.providers.ollama.url.clone()
                } else {
                    "http://localhost:11434".to_string()
                }
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: Server::default(),
            model: Model::default(),
            system_prompt: default_system_prompt().to_string(),
            providers: ProvidersConfig::default(),
        }
    }
}

/// The default system prompt used for new chat sessions.
pub fn default_system_prompt() -> &'static str {
    "You are an expert AI assistant. Assume the user is an expert. Do not explain code or concepts unless explicitly asked. Get straight to the point. Give concise, reliable, and direct answers. No yapping. Format output using elegant markdown."
}

/// The default model (local ollama 1.5b) used every time the app boots.
pub fn default_model_name() -> &'static str {
    "qwen2.5-coder-1.5b:latest"
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
pub struct ProvidersConfig {
    pub ollama: ProviderEntry,
    pub groq: ProviderEntry,
    pub gemini: ProviderEntry,
    pub nvidia: ProviderEntry,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderEntry {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub default_model: String,
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

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            ollama: ProviderEntry {
                url: "http://localhost:11434".to_string(),
                api_key: None,
                default_model: "qwen2.5-coder-1.5b:latest".to_string(),
            },
            groq: ProviderEntry {
                url: "https://api.groq.com/openai/v1".to_string(),
                api_key: None,
                default_model: "llama-3.3-70b-versatile".to_string(),
            },
            gemini: ProviderEntry {
                url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
                api_key: None,
                default_model: "gemini-2.0-flash".to_string(),
            },
            nvidia: ProviderEntry {
                url: "https://integrate.api.nvidia.com/v1".to_string(),
                api_key: None,
                default_model: "nvidia/llama-3.1-nemotron-70b-instruct".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_url_all_providers() {
        let config = Config::default();
        assert_eq!(config.resolve_url("ollama"), "http://localhost:11434");
        assert_eq!(config.resolve_url("groq"), "https://api.groq.com/openai/v1");
        assert_eq!(config.resolve_url("gemini"), "https://generativelanguage.googleapis.com/v1beta/openai");
        assert_eq!(config.resolve_url("nvidia"), "https://integrate.api.nvidia.com/v1");
        // Unknown providers fall back to ollama.
        assert_eq!(config.resolve_url("openai"), "http://localhost:11434");
    }

    #[test]
    fn load_ignores_old_search_and_extra_providers() {
        // A config from an older version: unknown sections/keys must be ignored.
        let raw = r#"
[server]
provider = "gemini"
url = "http://localhost:11434"

[model]
name = "gemma3:latest"

[search]
provider = "duckduckgo"
summarize = true
max_results = 40
custom_sources = []

[providers.ollama]
url = "http://localhost:11434"

[providers.groq]
url = "https://api.groq.com/openai/v1"
api_key = "gsk_x"

[providers.openai]
url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"

[providers.openrouter]
url = "https://openrouter.ai/api/v1"
"#;
        let config: Config = toml::from_str(raw).expect("old-style config should load");
        assert_eq!(config.server.provider, "gemini");
        assert_eq!(config.model.name, "gemma3:latest");
        assert_eq!(config.resolve_api_key("groq"), Some("gsk_x".to_string()));
    }

    #[test]
    fn test_resolve_api_key_from_provider_config() {
        let mut config = Config::default();
        config.providers.groq.api_key = Some("gsk_test123".to_string());
        config.providers.nvidia.api_key = Some("nvapi-xyz".to_string());

        assert_eq!(config.resolve_api_key("groq"), Some("gsk_test123".to_string()));
        assert_eq!(config.resolve_api_key("nvidia"), Some("nvapi-xyz".to_string()));
        assert_eq!(config.resolve_api_key("ollama"), None);
    }
}

