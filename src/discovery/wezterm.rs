use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::debug;
use url::Url;

use super::provider::{PaneInfo, PaneProvider};

pub struct WeztermProvider;

#[derive(Debug, Deserialize)]
struct WeztermPane {
    pane_id: u64,
    #[serde(default)]
    workspace: String,
    #[serde(default)]
    cwd: String,
    tab_id: u64,
    window_id: u64,
}

impl PaneProvider for WeztermProvider {
    fn list_panes(&self) -> Result<Vec<PaneInfo>> {
        let output = Command::new("wezterm")
            .args(["cli", "list", "--format", "json"])
            .output()
            .context("Failed to run wezterm cli list")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("wezterm cli list failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8(output.stdout.clone())
            .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).into_owned());
        let panes = parse_wezterm_output(&stdout)?;
        debug!("wezterm: found {} panes", panes.len());
        Ok(panes)
    }
}

fn parse_wezterm_output(json: &str) -> Result<Vec<PaneInfo>> {
    let wez_panes: Vec<WeztermPane> =
        serde_json::from_str(json).context("Failed to parse wezterm JSON")?;

    let mut panes = Vec::new();
    for wp in wez_panes {
        if wp.cwd.is_empty() {
            continue;
        }

        let cwd = parse_cwd_url(&wp.cwd);
        let cwd = match cwd {
            Some(p) => p,
            None => continue, // skip non-local (ssh) panes
        };

        let label = if wp.workspace.is_empty() {
            format!("wezterm:{}/{}/{}", wp.window_id, wp.tab_id, wp.pane_id)
        } else {
            format!(
                "wezterm:{}/{}/{}/{}",
                wp.workspace, wp.window_id, wp.tab_id, wp.pane_id
            )
        };

        panes.push(PaneInfo {
            pane_id: wp.pane_id.to_string(),
            label,
            cwd,
            source: "wezterm".to_string(),
        });
    }

    Ok(panes)
}

/// Parse a cwd URL like `file:///home/user/project` to a local path.
/// Returns None for non-local URLs (e.g., ssh://).
///
/// WezTerm emits `file://hostname/path` (with the machine's actual hostname),
/// not `file:///path`. The `url` crate's `to_file_path()` rejects non-empty
/// hosts other than "localhost", so we fall back to extracting the path
/// component directly.
fn parse_cwd_url(cwd: &str) -> Option<PathBuf> {
    if let Ok(url) = Url::parse(cwd) {
        if url.scheme() == "file" {
            url.to_file_path().ok().or_else(|| {
                let path = url.path();
                if path.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(path))
                }
            })
        } else {
            None // non-local
        }
    } else {
        // Not a URL, treat as plain path
        Some(PathBuf::from(cwd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cwd_url_file() {
        let path = parse_cwd_url("file:///home/user/project");
        assert_eq!(path, Some(PathBuf::from("/home/user/project")));
    }

    #[test]
    fn test_parse_cwd_url_ssh() {
        let path = parse_cwd_url("ssh://user@host/path");
        assert!(path.is_none());
    }

    #[test]
    fn test_parse_cwd_url_plain() {
        let path = parse_cwd_url("/home/user/project");
        assert_eq!(path, Some(PathBuf::from("/home/user/project")));
    }

    #[test]
    fn test_parse_wezterm_output() {
        let json = r#"[
            {
                "pane_id": 1,
                "workspace": "default",
                "cwd": "file:///home/user/project",
                "title": "bash",
                "tab_id": 0,
                "window_id": 0
            },
            {
                "pane_id": 2,
                "workspace": "default",
                "cwd": "ssh://user@remote/path",
                "title": "ssh",
                "tab_id": 0,
                "window_id": 0
            }
        ]"#;
        let panes = parse_wezterm_output(json).unwrap();
        assert_eq!(panes.len(), 1); // ssh pane is skipped
        assert_eq!(panes[0].cwd, PathBuf::from("/home/user/project"));
    }

    #[test]
    fn test_parse_cwd_url_file_with_hostname() {
        // wezterm actually emits file://hostname/path, not file:///path
        let path = parse_cwd_url("file://mymachine/home/user/project");
        assert_eq!(path, Some(PathBuf::from("/home/user/project")));
    }

    #[test]
    fn test_parse_wezterm_empty() {
        let json = "[]";
        let panes = parse_wezterm_output(json).unwrap();
        assert!(panes.is_empty());
    }
}
