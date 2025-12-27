//! @acp:module "Bridge Command"
//! @acp:summary "RFC-0006: Documentation bridging status and management"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Implements `acp bridge` command for managing documentation bridging.

use anyhow::Result;
use console::style;
use serde::Serialize;

use crate::cache::Cache;
use crate::config::Config;

/// Subcommands for the bridge command
#[derive(Debug, Clone)]
pub enum BridgeSubcommand {
    /// Show bridging configuration and statistics
    Status { json: bool },
}

/// Options for the bridge command
#[derive(Debug, Clone)]
pub struct BridgeOptions {
    /// Cache file path
    pub cache: std::path::PathBuf,
    /// Subcommand to execute
    pub subcommand: BridgeSubcommand,
}

/// Status output for JSON format
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeStatusJson {
    enabled: bool,
    precedence: String,
    summary: BridgeSummaryJson,
    by_format: std::collections::HashMap<String, u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeSummaryJson {
    total_annotations: u64,
    explicit_count: u64,
    converted_count: u64,
    merged_count: u64,
}

/// Execute the bridge command
pub fn execute_bridge(options: BridgeOptions, config: Config) -> Result<()> {
    match options.subcommand {
        BridgeSubcommand::Status { json } => execute_status(&options.cache, &config, json),
    }
}

/// Execute the bridge status subcommand
fn execute_status(cache_path: &std::path::Path, config: &Config, json_output: bool) -> Result<()> {
    // Load cache if it exists
    let cache = if cache_path.exists() {
        Some(Cache::from_json(cache_path)?)
    } else {
        None
    };

    if json_output {
        output_status_json(config, cache.as_ref())
    } else {
        output_status_text(config, cache.as_ref())
    }
}

/// Output status in JSON format
fn output_status_json(config: &Config, cache: Option<&Cache>) -> Result<()> {
    let status = if let Some(cache) = cache {
        BridgeStatusJson {
            enabled: config.bridge.enabled,
            precedence: config.bridge.precedence.to_string(),
            summary: BridgeSummaryJson {
                total_annotations: cache.bridge.summary.total_annotations,
                explicit_count: cache.bridge.summary.explicit_count,
                converted_count: cache.bridge.summary.converted_count,
                merged_count: cache.bridge.summary.merged_count,
            },
            by_format: cache.bridge.by_format.clone(),
        }
    } else {
        BridgeStatusJson {
            enabled: config.bridge.enabled,
            precedence: config.bridge.precedence.to_string(),
            summary: BridgeSummaryJson {
                total_annotations: 0,
                explicit_count: 0,
                converted_count: 0,
                merged_count: 0,
            },
            by_format: std::collections::HashMap::new(),
        }
    };

    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

/// Output status in human-readable format
fn output_status_text(config: &Config, cache: Option<&Cache>) -> Result<()> {
    println!("{}", style("Bridge Configuration:").bold());
    println!("  Enabled:    {}", if config.bridge.enabled {
        style("yes").green()
    } else {
        style("no").yellow()
    });
    println!("  Precedence: {}", config.bridge.precedence);
    println!("  Strictness: {:?}", config.bridge.strictness);
    println!();

    println!("{}", style("Language Support:").bold());
    println!("  JSDoc/TSDoc: {}", if config.bridge.jsdoc.enabled {
        style("enabled").green()
    } else {
        style("disabled").dim()
    });
    println!("  Python:      {}", if config.bridge.python.enabled {
        style("enabled").green()
    } else {
        style("disabled").dim()
    });
    if config.bridge.python.enabled {
        println!("    Style: {:?}", config.bridge.python.docstring_style);
    }
    println!("  Rust:        {}", if config.bridge.rust.enabled {
        style("enabled").green()
    } else {
        style("disabled").dim()
    });
    println!();

    if let Some(cache) = cache {
        let total = cache.bridge.summary.total_annotations;
        if total > 0 {
            println!("{}", style("Statistics:").bold());
            println!("  Total annotations: {}", total);

            let explicit = cache.bridge.summary.explicit_count;
            let converted = cache.bridge.summary.converted_count;
            let merged = cache.bridge.summary.merged_count;

            println!(
                "    Explicit (ACP only):     {:>5} ({:>5.1}%)",
                explicit,
                (explicit as f64 / total as f64) * 100.0
            );
            println!(
                "    Converted (from native): {:>5} ({:>5.1}%)",
                converted,
                (converted as f64 / total as f64) * 100.0
            );
            println!(
                "    Merged (ACP + native):   {:>5} ({:>5.1}%)",
                merged,
                (merged as f64 / total as f64) * 100.0
            );
            println!();

            if !cache.bridge.by_format.is_empty() {
                println!("{}", style("By Format:").bold());
                for (format, count) in &cache.bridge.by_format {
                    println!("  {}: {}", format, count);
                }
            }
        } else {
            println!("{}", style("Statistics:").bold());
            println!("  No bridged annotations found.");
            if !config.bridge.enabled {
                println!();
                println!(
                    "  {} Run `acp index --bridge` to enable bridging",
                    style("Hint:").cyan()
                );
            }
        }
    } else {
        println!("{}", style("Statistics:").bold());
        println!("  Cache not found. Run `acp index` first.");
    }

    Ok(())
}
