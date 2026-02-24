use std::fmt;

/// Application error that can be displayed in the UI
#[derive(Debug, Clone)]
pub struct AppError {
    pub message: String,
    pub context: Option<String>,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            context: None,
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context = Some(ctx.into());
        self
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref ctx) = self.context {
            write!(f, "{}: {}", ctx, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::new(format!("{:#}", e))
    }
}
