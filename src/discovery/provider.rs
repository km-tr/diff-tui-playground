use std::path::PathBuf;

use anyhow::Result;

/// Raw pane information from a provider
#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,
    pub label: String,
    pub cwd: PathBuf,
    pub source: String,
}

/// A resolved pane candidate with repo root
#[derive(Debug, Clone)]
pub struct PaneCandidate {
    pub repo_root: PathBuf,
    pub label: String,
    pub source: String,
}

/// Trait for pane discovery providers
pub trait PaneProvider {
    fn list_panes(&self) -> Result<Vec<PaneInfo>>;
}
