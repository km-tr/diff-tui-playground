use anyhow::Result;
use tracing::warn;

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            cb.set_text(text.to_string())?;
            Ok(())
        }
        Err(e) => {
            warn!("Clipboard not available: {}", e);
            Err(anyhow::anyhow!(
                "Clipboard not available: {}. Use export instead.",
                e
            ))
        }
    }
}
