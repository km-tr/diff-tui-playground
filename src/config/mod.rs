mod persist;

pub use persist::PersistentState;

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub default_base: Vec<String>,
    pub unified_context: u32,
    pub watch: WatchConfig,
    pub discovery: DiscoveryMode,
    pub truncate_max_lines: usize,
    pub truncate_max_bytes: usize,
    pub max_recent_contexts: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_base: vec![
                "main".into(),
                "master".into(),
                "origin/main".into(),
                "origin/master".into(),
            ],
            unified_context: 3,
            watch: WatchConfig::default(),
            discovery: DiscoveryMode::Auto,
            truncate_max_lines: 10_000,
            truncate_max_bytes: 5_000_000,
            max_recent_contexts: 20,
        }
    }
}

impl AppConfig {
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_default() -> Self {
        let config_path = Self::default_config_path();
        if let Some(path) = config_path {
            if path.exists() {
                match Self::load_from(&path) {
                    Ok(c) => return c,
                    Err(e) => {
                        eprintln!("warning: failed to load config from {path:?}: {e}");
                    }
                }
            }
        }
        Self::default()
    }

    fn default_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("diffdon").join("config.toml"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchConfig {
    pub enabled: bool,
    pub debounce_ms: u64,
    pub poll_interval_secs: Option<u64>,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 300,
            poll_interval_secs: Some(30),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryMode {
    Off,
    #[default]
    Auto,
    Tmux,
    Wezterm,
}
