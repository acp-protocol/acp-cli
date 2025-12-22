//! @acp:module "File Watcher"
//! @acp:summary "Watches for file changes and triggers incremental updates"
//! @acp:domain cli
//! @acp:layer service
//!
//! Watches for file changes and updates cache/vars incrementally.

use std::path::Path;
use std::sync::mpsc;

use console::style;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config};

use crate::config::Config as AcpConfig;
use crate::error::Result;

/// File watcher for incremental updates
pub struct FileWatcher {
    _config: AcpConfig,
}

impl FileWatcher {
    pub fn new(config: AcpConfig) -> Self {
        Self { _config: config }
    }

    /// Start watching for changes
    pub fn watch<P: AsRef<Path>>(&self, root: P) -> Result<()> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|e| crate::error::AcpError::Other(e.to_string()))?;

        watcher.watch(root.as_ref(), RecursiveMode::Recursive)
            .map_err(|e| crate::error::AcpError::Other(e.to_string()))?;

        println!("Watching for changes...");

        loop {
            match rx.recv() {
                Ok(event) => {
                    match event {
                        Ok(event) => {
                            println!("Change detected: {:?}", event);
                            // TODO: Incremental update based on event.kind
                        }
                        Err(e) => {
                            eprintln!("{} Watch error: {}", style("✗").red(), e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} Channel error: {}", style("✗").red(), e);
                    break;
                }
            }
        }

        Ok(())
    }
}
