//! Persistent configuration, loaded from the platform config dir.

/// App config; every field has a sane default so the file is optional.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Config {
    pub server: Server,
    pub model: Model,
    pub search: Search,
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

    /// Save current config. TODO(stub): also persist to user data dir.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: Server {
                url: "http://localhost:11434".to_string(),
            },
            model: Model {
                name: "qwen2.5-coder-1.5b:latest".to_string(),
            },
            search: Search {
                provider: "google".to_string(),
                summarize: true,
            },
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Server {
    pub url: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Model {
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Search {
    pub provider: String,
    pub summarize: bool,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            url: "http://localhost:11434".to_string(),
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
        }
    }
}
