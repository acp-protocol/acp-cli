//! @acp:module "Index Command"
//! @acp:summary "Index the codebase and generate cache"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Implements `acp index` command for codebase indexing.

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::config::Config;
use crate::index::Indexer;

/// Options for the index command
#[derive(Debug, Clone)]
pub struct IndexOptions {
    /// Root directory to index
    pub root: PathBuf,
    /// Output cache file path
    pub output: PathBuf,
    /// Also generate vars file
    pub vars: bool,
    /// Enable documentation bridging (RFC-0006)
    pub bridge: bool,
    /// Disable documentation bridging (overrides config)
    pub no_bridge: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            output: PathBuf::from(".acp/acp.cache.json"),
            vars: false,
            bridge: false,
            no_bridge: false,
        }
    }
}

/// Execute the index command
pub async fn execute_index(options: IndexOptions, config: Config) -> Result<()> {
    println!("{} Indexing codebase...", style("→").cyan());

    // Use config from target root if it exists, otherwise use defaults
    let mut effective_config = {
        let root_config = options.root.join(".acp.config.json");
        let root_str = options.root.to_string_lossy();
        if root_config.exists() {
            Config::load(&root_config).unwrap_or_default()
        } else if root_str != "." && root_str != "./" {
            // Indexing a subdirectory - use defaults to avoid pattern mismatches
            Config::default()
        } else {
            config
        }
    };

    // RFC-0006: Handle bridge flag overrides
    // --no-bridge always wins, then --bridge, then config
    if options.no_bridge {
        effective_config.bridge.enabled = false;
    } else if options.bridge {
        effective_config.bridge.enabled = true;
    }

    // Show bridging status
    if effective_config.bridge.enabled {
        println!(
            "{} Documentation bridging enabled ({})",
            style("→").cyan(),
            effective_config.bridge.precedence
        );
    }

    let indexer = Indexer::new(effective_config.clone())?;
    let cache = indexer.index(&options.root).await?;

    // Warn if no files were found, but still create empty cache
    if cache.stats.files == 0 {
        eprintln!(
            "{} No files found matching include patterns",
            style("⚠").yellow()
        );
        eprintln!("  Check your .acp.config.json include/exclude patterns");
        eprintln!("  Current patterns:");
        for pattern in &effective_config.include {
            eprintln!("    include: {}", pattern);
        }
        for pattern in &effective_config.exclude {
            eprintln!("    exclude: {}", pattern);
        }
        // Still create the cache file (empty but valid)
    }

    // Create output directory if needed
    if let Some(parent) = options.output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    cache.write_json(&options.output)?;
    println!(
        "{} Cache written to {}",
        style("✓").green(),
        options.output.display()
    );
    println!("  Files: {}", cache.stats.files);
    println!("  Symbols: {}", cache.stats.symbols);
    println!("  Lines: {}", cache.stats.lines);

    if options.vars {
        let vars_file = indexer.generate_vars(&cache);
        // Replace acp.cache.json with acp.vars.json
        let output_str = options.output.to_string_lossy();
        let vars_path = if output_str.contains("acp.cache.json") {
            PathBuf::from(output_str.replace("acp.cache.json", "acp.vars.json"))
        } else if output_str.contains("cache.json") {
            PathBuf::from(output_str.replace("cache.json", "vars.json"))
        } else {
            options.output.with_extension("vars.json")
        };
        vars_file.write_json(&vars_path)?;
        println!(
            "{} Vars written to {}",
            style("✓").green(),
            vars_path.display()
        );
    }

    Ok(())
}
