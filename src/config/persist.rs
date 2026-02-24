use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::git::model::WorktreeMode;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistentState {
    pub recent_contexts: Vec<RecentContext>,
    pub last_mode: Option<String>,
    pub last_worktree_mode: Option<String>,
    pub last_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentContext {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub timestamp: u64,
}

impl PersistentState {
    pub fn load() -> Self {
        let path = Self::state_path();
        if let Some(path) = path {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(state) => return state,
                    Err(e) => {
                        tracing::warn!("Failed to parse persistent state: {}", e);
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!("Failed to read persistent state: {}", e);
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(path) = Self::state_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)?;
            std::fs::write(&path, content)?;
        }
        Ok(())
    }

    pub fn add_recent_context(&mut self, path: PathBuf, branch: Option<String>, max: usize) {
        self.recent_contexts.retain(|c| c.path != path);
        self.recent_contexts.insert(
            0,
            RecentContext {
                path,
                branch,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            },
        );
        self.recent_contexts.truncate(max);
    }

    pub fn save_mode(&mut self, mode: &str, worktree_mode: WorktreeMode, base: Option<&str>) {
        self.last_mode = Some(mode.to_string());
        self.last_worktree_mode = Some(match worktree_mode {
            WorktreeMode::Unstaged => "unstaged".to_string(),
            WorktreeMode::Staged => "staged".to_string(),
        });
        self.last_base = base.map(|b| b.to_string());
    }

    fn state_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("diffdon").join("state.json"))
    }
}
