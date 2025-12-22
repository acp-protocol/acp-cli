//! @acp:module "Check Command"
//! @acp:summary "Check guardrails for a file"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::cache::Cache;

/// Options for the check command
#[derive(Debug, Clone)]
pub struct CheckOptions {
    /// File to check
    pub file: PathBuf,
    /// Cache file
    pub cache: PathBuf,
}

/// Execute the check command
pub fn execute_check(options: CheckOptions) -> Result<()> {
    let cache_data = Cache::from_json(&options.cache)?;

    // Try multiple path formats to find the file
    let file_str = options.file.to_string_lossy().to_string();
    let file_entry = cache_data
        .files
        .get(&file_str)
        .or_else(|| cache_data.files.get(&format!("./{}", file_str)))
        .or_else(|| {
            let stripped = file_str.strip_prefix("./").unwrap_or(&file_str);
            cache_data.files.get(stripped)
        });

    if let Some(file_entry) = file_entry {
        println!("{} File found in cache", style("✓").green());
        println!("  Path: {}", file_entry.path);
        println!("  Lines: {}", file_entry.lines);
        println!("  Language: {:?}", file_entry.language);

        if let Some(stability) = &file_entry.stability {
            println!("  Stability: {:?}", stability);
        }

        if !file_entry.ai_hints.is_empty() {
            println!("  AI hints: {}", file_entry.ai_hints.join(", "));
        }

        // Check constraints if available
        if let Some(ref constraints) = cache_data.constraints {
            let file_constraints = constraints
                .by_file
                .get(&file_entry.path)
                .or_else(|| constraints.by_file.get(&format!("./{}", file_entry.path)))
                .or_else(|| {
                    let stripped = file_entry
                        .path
                        .strip_prefix("./")
                        .unwrap_or(&file_entry.path);
                    constraints.by_file.get(stripped)
                });

            if let Some(file_constraints) = file_constraints {
                if let Some(mutation) = &file_constraints.mutation {
                    println!("  Lock level: {:?}", mutation.level);
                    if mutation.requires_approval {
                        println!("  {} Requires approval", style("⚠").yellow());
                    }
                    if mutation.requires_tests {
                        println!("  {} Requires tests", style("⚠").yellow());
                    }
                    if mutation.requires_docs {
                        println!("  {} Requires documentation", style("⚠").yellow());
                    }
                }
            }
        }
    } else {
        eprintln!(
            "{} File not in cache: {}",
            style("✗").red(),
            options.file.display()
        );
    }

    Ok(())
}
