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
    pub fn new(path: &Path, debounce: Duration, tx: Sender<InternalEvent>) -> Result<Self> {
        let tx_clone = tx.clone();
        let mut debouncer = new_debouncer(
            debounce,
            move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| match res
            {
                Ok(events) => {
                    let dominated_by_gitdir = events.iter().all(|e| {
                        e.path.to_string_lossy().contains(".git/")
                            || e.path.to_string_lossy().contains(".git\\")
                    });
                    if dominated_by_gitdir {
                        debug!("Ignoring .git internal events");
                        return;
                    }
                    debug!("Watch triggered: {} events", events.len());
                    let _ = tx_clone.send(InternalEvent::WatchTriggered);
                }
                Err(e) => {
                    error!("Watch error: {:?}", e);
                }
            },
        )
        .context("Failed to create file watcher")?;

        debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
            .context("Failed to watch path")?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
