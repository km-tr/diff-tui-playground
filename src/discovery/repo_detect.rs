use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::debug;

/// Detect if the given path is inside a git repo and return the repo root
pub fn detect_repo(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if root.is_empty() {
                None
            } else {
                let path = PathBuf::from(root);
                debug!("Detected repo root: {:?}", path);
                Some(path)
            }
        }
        _ => None,
    }
}
