//! @acp:module "Attempt Command"
//! @acp:summary "Manage troubleshooting attempts"
//! @acp:domain cli
//! @acp:layer handler

use anyhow::Result;
use console::style;

use crate::constraints::AttemptStatus;
use crate::AttemptTracker;

/// Subcommand types for the attempt command
#[derive(Debug, Clone)]
pub enum AttemptSubcommand {
    Start {
        id: String,
        for_issue: Option<String>,
        description: Option<String>,
    },
    List {
        active: bool,
        failed: bool,
        history: bool,
    },
    Fail {
        id: String,
        reason: Option<String>,
    },
    Verify {
        id: String,
    },
    Revert {
        id: String,
    },
    Cleanup,
    Checkpoint {
        name: String,
        files: Vec<String>,
        description: Option<String>,
    },
    Checkpoints,
    Restore {
        name: String,
    },
}

/// Execute the attempt command
pub fn execute_attempt(subcommand: AttemptSubcommand) -> Result<()> {
    let mut tracker = AttemptTracker::load_or_create();

    match subcommand {
        AttemptSubcommand::Start {
            id,
            for_issue,
            description,
        } => {
            tracker.start_attempt(&id, for_issue.as_deref(), description.as_deref());
            tracker.save()?;
            println!("{} Started attempt: {}", style("✓").green(), id);
        }

        AttemptSubcommand::List {
            active,
            failed,
            history,
        } => {
            if history {
                println!("{}", style("Attempt History:").bold());
                for entry in &tracker.history {
                    let status_color = match entry.status {
                        AttemptStatus::Verified => style("✓").green(),
                        AttemptStatus::Failed => style("✗").red(),
                        AttemptStatus::Reverted => style("↩").yellow(),
                        _ => style("?").dim(),
                    };
                    println!(
                        "  {} {} - {:?} ({} files)",
                        status_color, entry.id, entry.status, entry.files_modified
                    );
                }
            } else {
                println!("{}", style("Active Attempts:").bold());
                for attempt in tracker.attempts.values() {
                    if active && attempt.status != AttemptStatus::Active {
                        continue;
                    }
                    if failed && attempt.status != AttemptStatus::Failed {
                        continue;
                    }

                    let status_color = match attempt.status {
                        AttemptStatus::Active => style("●").cyan(),
                        AttemptStatus::Testing => style("◐").yellow(),
                        AttemptStatus::Failed => style("✗").red(),
                        _ => style("?").dim(),
                    };

                    println!("  {} {} - {:?}", status_color, attempt.id, attempt.status);
                    if let Some(issue) = &attempt.for_issue {
                        println!("    For: {}", issue);
                    }
                    println!("    Files: {}", attempt.files.len());
                }
            }
        }

        AttemptSubcommand::Fail { id, reason } => {
            tracker.fail_attempt(&id, reason.as_deref())?;
            tracker.save()?;
            println!("{} Marked attempt as failed: {}", style("✗").red(), id);
        }

        AttemptSubcommand::Verify { id } => {
            tracker.verify_attempt(&id)?;
            tracker.save()?;
            println!("{} Verified attempt: {}", style("✓").green(), id);
        }

        AttemptSubcommand::Revert { id } => {
            let actions = tracker.revert_attempt(&id)?;
            println!("{} Reverted attempt: {}", style("↩").yellow(), id);
            for action in &actions {
                println!("  {} {}", style(&action.action).dim(), action.file);
            }
        }

        AttemptSubcommand::Cleanup => {
            let actions = tracker.cleanup_failed()?;
            println!(
                "{} Cleaned up {} files from failed attempts",
                style("✓").green(),
                actions.len()
            );
        }

        AttemptSubcommand::Checkpoint {
            name,
            files,
            description,
        } => {
            let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
            tracker.create_checkpoint(&name, &file_refs, description.as_deref())?;
            println!(
                "{} Created checkpoint: {} ({} files)",
                style("✓").green(),
                name,
                files.len()
            );
        }

        AttemptSubcommand::Checkpoints => {
            println!("{}", style("Checkpoints:").bold());
            for (name, cp) in &tracker.checkpoints {
                println!(
                    "  {} - {} files, created {}",
                    style(name).cyan(),
                    cp.files.len(),
                    cp.created_at.format("%Y-%m-%d %H:%M")
                );
            }
        }

        AttemptSubcommand::Restore { name } => {
            let actions = tracker.restore_checkpoint(&name)?;
            println!("{} Restored checkpoint: {}", style("↩").yellow(), name);
            for action in &actions {
                println!("  {} {}", style(&action.action).dim(), action.file);
            }
        }
    }

    Ok(())
}
