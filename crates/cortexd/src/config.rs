use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexConfig {
    #[serde(default = "default_watch_dirs")]
    pub watch_dirs: Vec<PathBuf>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,
    #[serde(default)]
    pub claude_enabled: bool,
    pub claude_api_key: Option<String>,
    #[serde(default = "default_max_calls")]
    pub claude_max_calls_per_hour: u32,
    #[serde(default = "default_threshold")]
    pub insight_threshold: f64,
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
    #[serde(default)]
    pub notifications_enabled: bool,
}

fn default_watch_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    vec![home.join("projects")]
}

fn default_retention_days() -> u64 {
    90
}

fn default_max_calls() -> u32 {
    10
}

fn default_threshold() -> f64 {
    0.6
}

fn default_debounce() -> u64 {
    500
}

impl Default for CortexConfig {
    fn default() -> Self {
        Self {
            watch_dirs: default_watch_dirs(),
            exclude_patterns: Vec::new(),
            retention_days: default_retention_days(),
            claude_enabled: false,
            claude_api_key: None,
            claude_max_calls_per_hour: default_max_calls(),
            insight_threshold: default_threshold(),
            debounce_ms: default_debounce(),
            notifications_enabled: false,
        }
    }
}

impl CortexConfig {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_file_path();
        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let config: CortexConfig = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    fn config_file_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("cortex")
            .join("config.toml")
    }

    pub fn data_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("cortex")
    }

    pub fn socket_path() -> PathBuf {
        Self::data_dir().join("cortexd.sock")
    }

    pub fn pipe_path(name: &str) -> PathBuf {
        Self::data_dir().join(name)
    }
}
