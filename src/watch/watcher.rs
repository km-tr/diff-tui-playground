use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::new_debouncer;
use tracing::{debug, error};

use crate::app::event::InternalEvent;

pub struct FileWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<RecommendedWatcher>,
}

impl FileWatcher {
    pub fn new(paths: &[&Path], debounce: Duration, tx: Sender<InternalEvent>) -> Result<Self> {
        let mut debouncer = new_debouncer(
            debounce,
            move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| match res
            {
                Ok(events) => {
                    debug!("Watch triggered: {} events", events.len());
                    if let Err(err) = tx.try_send(InternalEvent::WatchTriggered) {
                        debug!("Watch event dropped: {}", err);
                    }
                }
                Err(e) => {
                    error!("Watch error: {:?}", e);
                }
            },
        )
        .context("Failed to create file watcher")?;

        for path in paths {
            debouncer
                .watcher()
                .watch(path, RecursiveMode::Recursive)
                .with_context(|| format!("Failed to watch path: {}", path.display()))?;
        }

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
