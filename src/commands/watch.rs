//! @acp:module "Watch Command"
//! @acp:summary "Watch for changes and update cache"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;
use crate::watch::FileWatcher;

/// Options for the watch command
#[derive(Debug, Clone)]
pub struct WatchOptions {
    /// Root directory to watch
    pub root: PathBuf,
}

/// Execute the watch command
pub fn execute_watch(options: WatchOptions, config: Config) -> Result<()> {
    let watcher = FileWatcher::new(config);
    watcher.watch(&options.root)?;
    Ok(())
}
