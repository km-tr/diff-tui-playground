use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use tracing::debug;

use super::provider::{PaneInfo, PaneProvider};

pub struct TmuxProvider;

impl PaneProvider for TmuxProvider {
    fn list_panes(&self) -> Result<Vec<PaneInfo>> {
        let output = Command::new("tmux")
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name}:#{window_index}.#{pane_index}\t#{pane_current_path}\t#{pane_title}",
            ])
            .output()
            .context("Failed to run tmux list-panes")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux list-panes failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let panes = parse_tmux_output(&stdout);
        debug!("tmux: found {} panes", panes.len());
        Ok(panes)
    }
}

fn parse_tmux_output(output: &str) -> Vec<PaneInfo> {
    let mut panes = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() >= 2 {
            let pane_id = parts[0].to_string();
            let cwd = PathBuf::from(parts[1]);
            let title = parts.get(2).unwrap_or(&"").to_string();

            if cwd.as_os_str().is_empty() {
                continue;
            }

            let label = if title.is_empty() {
                pane_id.clone()
            } else {
                format!("{} ({})", pane_id, title)
            };

            panes.push(PaneInfo {
                pane_id,
                label,
                cwd,
                source: "tmux".to_string(),
            });
        }
    }
    panes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tmux_output() {
        let output = "main:0.0\t/home/user/project\tbash\nmain:0.1\t/home/user/other\tvim\n";
        let panes = parse_tmux_output(output);
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].pane_id, "main:0.0");
        assert_eq!(panes[0].cwd, PathBuf::from("/home/user/project"));
        assert_eq!(panes[0].label, "main:0.0 (bash)");
        assert_eq!(panes[1].pane_id, "main:0.1");
    }

    #[test]
    fn test_parse_empty_cwd() {
        let output = "main:0.0\t\tbash\n";
        let panes = parse_tmux_output(output);
        assert!(panes.is_empty());
    }

    #[test]
    fn test_parse_no_title() {
        let output = "main:0.0\t/home/user/project\n";
        let panes = parse_tmux_output(output);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].label, "main:0.0");
    }
}
