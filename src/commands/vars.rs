//! @acp:module "Vars Command"
//! @acp:summary "Generate vars file from cache"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::cache::Cache;
use crate::config::Config;
use crate::index::Indexer;

/// Options for the vars command
#[derive(Debug, Clone)]
pub struct VarsOptions {
    /// Cache file to read
    pub cache: PathBuf,
    /// Output vars file path
    pub output: PathBuf,
}

/// Execute the vars command
pub fn execute_vars(options: VarsOptions) -> Result<()> {
    println!("{} Generating vars...", style("→").cyan());

    let cache_data = Cache::from_json(&options.cache)?;
    let config = Config::default();
    let indexer = Indexer::new(config)?;
    let vars_file = indexer.generate_vars(&cache_data);

    vars_file.write_json(&options.output)?;
    println!(
        "{} Vars written to {}",
        style("✓").green(),
        options.output.display()
    );
    println!("  Variables: {}", vars_file.variables.len());

    Ok(())
}
