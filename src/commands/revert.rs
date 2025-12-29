//! @acp:module "Revert Command"
//! @acp:summary "Revert changes from attempts or checkpoints"
//! @acp:domain cli
//! @acp:layer handler

use anyhow::Result;
use console::style;

use crate::AttemptTracker;

/// Options for the revert command
#[derive(Debug, Clone)]
pub struct RevertOptions {
    /// Attempt ID to revert
    pub attempt: Option<String>,
    /// Checkpoint name to restore
    pub checkpoint: Option<String>,
}

/// Execute the revert command
pub fn execute_revert(options: RevertOptions) -> Result<()> {
    let mut tracker = AttemptTracker::load_or_create();

    if let Some(id) = options.attempt {
        let actions = tracker.revert_attempt(&id)?;
        println!("{} Reverted attempt: {}", style("↩").yellow(), id);
        for action in &actions {
            println!("  {} {}", style(&action.action).dim(), action.file);
        }
    } else if let Some(name) = options.checkpoint {
        let actions = tracker.restore_checkpoint(&name)?;
        println!("{} Restored checkpoint: {}", style("↩").yellow(), name);
        for action in &actions {
            println!("  {} {}", style(&action.action).dim(), action.file);
        }
    } else {
        eprintln!("{} Specify --attempt or --checkpoint", style("✗").red());
        std::process::exit(1);
    }

    Ok(())
}
