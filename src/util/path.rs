use std::path::{Path, PathBuf};

/// Normalize a path by resolving it to an absolute canonical form
pub fn normalize_path(path: &Path) -> PathBuf {
    match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Fallback: just make it absolute
            if path.is_absolute() {
                path.to_path_buf()
            } else if let Ok(cwd) = std::env::current_dir() {
                cwd.join(path)
            } else {
                // Cannot determine an absolute path; return as-is
                path.to_path_buf()
            }
        }
    }
}

/// Shorten a path for display (relative to home if possible)
pub fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}
