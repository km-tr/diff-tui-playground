mod provider;
mod repo_detect;
mod tmux;
mod wezterm;

pub use provider::{PaneCandidate, PaneInfo, PaneProvider};

use crate::config::DiscoveryMode;
use tracing::{debug, warn};

/// Discover panes and resolve to repo candidates
pub fn discover_panes(mode: &DiscoveryMode) -> Vec<PaneCandidate> {
    let mut all_panes: Vec<PaneInfo> = Vec::new();

    match mode {
        DiscoveryMode::Off => return Vec::new(),
        DiscoveryMode::Tmux => match tmux::TmuxProvider.list_panes() {
            Ok(panes) => all_panes.extend(panes),
            Err(e) => warn!("tmux discovery failed: {}", e),
        },
        DiscoveryMode::Wezterm => match wezterm::WeztermProvider.list_panes() {
            Ok(panes) => all_panes.extend(panes),
            Err(e) => warn!("wezterm discovery failed: {}", e),
        },
        DiscoveryMode::Auto => {
            // Aggregate panes from all available providers
            match tmux::TmuxProvider.list_panes() {
                Ok(panes) => all_panes.extend(panes),
                Err(e) => debug!("tmux discovery unavailable in Auto mode: {}", e),
            }
            match wezterm::WeztermProvider.list_panes() {
                Ok(panes) => all_panes.extend(panes),
                Err(e) => debug!("wezterm discovery unavailable in Auto mode: {}", e),
            }
        }
    }

    debug!("Discovered {} panes", all_panes.len());

    // Resolve to repo candidates
    let mut candidates: Vec<PaneCandidate> = Vec::new();
    for pane in all_panes {
        if let Some(root) = repo_detect::detect_repo(&pane.cwd) {
            // Dedup by repo root
            if !candidates.iter().any(|c| c.repo_root == root) {
                candidates.push(PaneCandidate {
                    repo_root: root,
                    label: pane.label,
                    source: pane.source,
                });
            }
        }
    }

    debug!("Resolved {} repo candidates", candidates.len());
    candidates
}
