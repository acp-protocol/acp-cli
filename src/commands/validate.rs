//! @acp:module "Validate Command"
//! @acp:summary "Validate cache/vars files against schemas"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::schema;

/// Options for the validate command
#[derive(Debug, Clone)]
pub struct ValidateOptions {
    /// File to validate
    pub file: PathBuf,
}

/// Execute the validate command
pub fn execute_validate(options: ValidateOptions) -> Result<()> {
    let content = std::fs::read_to_string(&options.file)?;
    let filename = options.file.to_string_lossy();

    // Use detect_schema_type() for all 6 schema types
    if let Some(schema_type) = schema::detect_schema_type(&filename) {
        schema::validate_by_type(&content, schema_type)?;
        println!(
            "{} {} file is valid",
            style("✓").green(),
            schema_type.to_uppercase()
        );
    } else {
        // Try auto-detection from $schema field
        let json: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(schema_url) = json.get("$schema").and_then(|s| s.as_str()) {
            let detected = if schema_url.contains("cache") {
                "cache"
            } else if schema_url.contains("vars") {
                "vars"
            } else if schema_url.contains("config") {
                "config"
            } else if schema_url.contains("attempts") {
                "attempts"
            } else if schema_url.contains("sync") {
                "sync"
            } else if schema_url.contains("primer") {
                "primer"
            } else {
                ""
            };

            if !detected.is_empty() {
                schema::validate_by_type(&content, detected)?;
                println!(
                    "{} {} file is valid",
                    style("✓").green(),
                    detected.to_uppercase()
                );
            } else {
                eprintln!(
                    "{} Unknown schema type. Could not detect from filename or $schema field.",
                    style("✗").red()
                );
                std::process::exit(1);
            }
        } else {
            eprintln!(
                "{} Unknown file type. Provide filename with schema type (cache, vars, config, primer, attempts, sync) or include $schema field.",
                style("✗").red()
            );
            std::process::exit(1);
        }
    }

    Ok(())
}
