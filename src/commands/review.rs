//! @acp:module "Review Command"
//! @acp:summary "Review and manage annotation provenance (RFC-0003)"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Provides functionality for reviewing auto-generated annotations:
//! - List annotations needing review
//! - Mark annotations as reviewed
//! - Interactive review mode

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use console::style;

use crate::cache::{AnnotationProvenance, Cache};
use crate::commands::query::ConfidenceFilter;
use crate::parse::SourceOrigin;

/// Options for the review command (RFC-0003)
#[derive(Debug, Clone)]
pub struct ReviewOptions {
    /// Cache file path
    pub cache: PathBuf,
    /// Filter by source origin
    pub source: Option<SourceOrigin>,
    /// Filter by confidence expression (e.g., "<0.7", ">=0.9")
    pub confidence: Option<String>,
    /// Output as JSON
    pub json: bool,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            cache: PathBuf::from(".acp/acp.cache.json"),
            source: None,
            confidence: None,
            json: false,
        }
    }
}

/// Review subcommands
#[derive(Debug, Clone)]
pub enum ReviewSubcommand {
    /// List annotations needing review
    List,
    /// Mark annotations as reviewed
    Mark {
        file: Option<PathBuf>,
        symbol: Option<String>,
        all: bool,
    },
    /// Interactive review mode
    Interactive,
}

/// Item for review display
#[derive(Debug, Clone)]
struct ReviewItem {
    target: String,
    annotation: String,
    value: String,
    source: SourceOrigin,
    confidence: Option<f64>,
}

/// Execute the review command
pub fn execute_review(options: ReviewOptions, subcommand: ReviewSubcommand) -> Result<()> {
    match subcommand {
        ReviewSubcommand::List => {
            let cache = Cache::from_json(&options.cache)?;
            list_for_review(&cache, &options)
        }
        ReviewSubcommand::Mark { file, symbol, all } => {
            let mut cache = Cache::from_json(&options.cache)?;
            mark_reviewed(&mut cache, &options, file.as_ref(), symbol.as_deref(), all)?;
            cache.write_json(&options.cache)?;
            Ok(())
        }
        ReviewSubcommand::Interactive => {
            let mut cache = Cache::from_json(&options.cache)?;
            interactive_review(&mut cache, &options)?;
            cache.write_json(&options.cache)?;
            Ok(())
        }
    }
}

/// List all annotations needing review
fn list_for_review(cache: &Cache, options: &ReviewOptions) -> Result<()> {
    let items = collect_review_items(cache, options);

    if items.is_empty() {
        println!("{} No annotations need review!", style("✓").green());
        return Ok(());
    }

    if options.json {
        let json_items: Vec<_> = items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "target": item.target,
                    "annotation": item.annotation,
                    "value": item.value,
                    "source": format!("{:?}", item.source).to_lowercase(),
                    "confidence": item.confidence,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_items)?);
        return Ok(());
    }

    println!(
        "{} {} annotations need review:",
        style("→").cyan(),
        items.len()
    );
    println!();

    for (i, item) in items.iter().enumerate() {
        println!("{}. {}", i + 1, style(&item.target).cyan());
        println!(
            "   @acp:{} \"{}\"",
            item.annotation,
            truncate_value(&item.value, 50)
        );
        println!("   Source: {:?}", item.source);
        if let Some(conf) = item.confidence {
            println!("   Confidence: {:.2}", conf);
        }
        println!();
    }

    println!(
        "Run {} to mark all as reviewed",
        style("acp review mark --all").cyan()
    );

    Ok(())
}

/// Collect all items that need review
fn collect_review_items(cache: &Cache, options: &ReviewOptions) -> Vec<ReviewItem> {
    let mut items = Vec::new();
    let conf_filter = options
        .confidence
        .as_ref()
        .and_then(|c| ConfidenceFilter::parse(c).ok());

    // Collect from files
    for (path, file) in &cache.files {
        for (key, prov) in &file.annotations {
            if should_include(prov, options, &conf_filter) {
                items.push(ReviewItem {
                    target: path.clone(),
                    annotation: key.trim_start_matches("@acp:").to_string(),
                    value: prov.value.clone(),
                    source: prov.source,
                    confidence: prov.confidence,
                });
            }
        }
    }

    // Collect from symbols
    for symbol in cache.symbols.values() {
        for (key, prov) in &symbol.annotations {
            if should_include(prov, options, &conf_filter) {
                items.push(ReviewItem {
                    target: format!("{}:{}", symbol.file, symbol.name),
                    annotation: key.trim_start_matches("@acp:").to_string(),
                    value: prov.value.clone(),
                    source: prov.source,
                    confidence: prov.confidence,
                });
            }
        }
    }

    // Sort by confidence (lowest first)
    items.sort_by(|a, b| {
        a.confidence
            .unwrap_or(1.0)
            .partial_cmp(&b.confidence.unwrap_or(1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    items
}

/// Check if an annotation should be included based on filters
fn should_include(
    prov: &AnnotationProvenance,
    options: &ReviewOptions,
    conf_filter: &Option<ConfidenceFilter>,
) -> bool {
    // Must need review and not already reviewed
    if prov.reviewed || !prov.needs_review {
        return false;
    }

    // Source filter
    if let Some(ref source) = options.source {
        if prov.source != *source {
            return false;
        }
    }

    // Confidence filter
    if let Some(ref filter) = conf_filter {
        if let Some(conf) = prov.confidence {
            if !filter.matches(conf) {
                return false;
            }
        }
    }

    true
}

/// Mark annotations as reviewed
fn mark_reviewed(
    cache: &mut Cache,
    options: &ReviewOptions,
    file: Option<&PathBuf>,
    symbol: Option<&str>,
    all: bool,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let conf_filter = options
        .confidence
        .as_ref()
        .and_then(|c| ConfidenceFilter::parse(c).ok());
    let mut count = 0;

    // Mark file annotations
    for (path, file_entry) in cache.files.iter_mut() {
        // Check file filter
        if let Some(filter_path) = file {
            if !path.contains(&filter_path.to_string_lossy().to_string()) {
                continue;
            }
        }

        for prov in file_entry.annotations.values_mut() {
            if (all || should_include(prov, options, &conf_filter)) && !prov.reviewed {
                prov.reviewed = true;
                prov.needs_review = false;
                prov.reviewed_at = Some(now.clone());
                count += 1;
            }
        }
    }

    // Mark symbol annotations
    for sym in cache.symbols.values_mut() {
        // Check symbol filter
        if let Some(sym_filter) = symbol {
            if sym.name != sym_filter {
                continue;
            }
        }

        // Check file filter for symbol
        if let Some(filter_path) = file {
            if !sym
                .file
                .contains(&filter_path.to_string_lossy().to_string())
            {
                continue;
            }
        }

        for prov in sym.annotations.values_mut() {
            if (all || should_include(prov, options, &conf_filter)) && !prov.reviewed {
                prov.reviewed = true;
                prov.needs_review = false;
                prov.reviewed_at = Some(now.clone());
                count += 1;
            }
        }
    }

    // Recompute provenance stats
    recompute_provenance_stats(cache);

    println!(
        "{} Marked {} annotations as reviewed",
        style("✓").green(),
        count
    );

    Ok(())
}

/// Interactive review mode
fn interactive_review(cache: &mut Cache, options: &ReviewOptions) -> Result<()> {
    let items = collect_review_items(cache, options);

    if items.is_empty() {
        println!("{} No annotations need review!", style("✓").green());
        return Ok(());
    }

    println!("{}", style("Interactive Review Mode").bold());
    println!("{}", "=".repeat(40));
    println!("{} annotations to review", items.len());
    println!();
    println!("Commands: [a]ccept, [s]kip, [q]uit");
    println!();

    let now = Utc::now().to_rfc3339();
    let mut reviewed_count = 0;
    let mut skipped_count = 0;

    for item in &items {
        println!("{}", style(&item.target).cyan());
        println!(
            "  @acp:{} \"{}\"",
            item.annotation,
            truncate_value(&item.value, 50)
        );
        println!("  Source: {:?}", item.source);
        if let Some(conf) = item.confidence {
            println!("  Confidence: {:.2}", conf);
        }
        println!();

        print!("{} ", style(">").yellow());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().chars().next() {
            Some('a') | Some('A') => {
                // Mark as reviewed in cache
                mark_single_reviewed(cache, item, &now)?;
                println!("{} Marked as reviewed", style("✓").green());
                reviewed_count += 1;
            }
            Some('s') | Some('S') => {
                println!("Skipped");
                skipped_count += 1;
            }
            Some('q') | Some('Q') => {
                println!("\nExiting review");
                break;
            }
            _ => {
                println!("Unknown command, skipping");
                skipped_count += 1;
            }
        }

        println!();
    }

    // Recompute stats
    recompute_provenance_stats(cache);

    println!("{}", "=".repeat(40));
    println!(
        "Reviewed: {}, Skipped: {}",
        style(reviewed_count).green(),
        skipped_count
    );

    Ok(())
}

/// Mark a single annotation as reviewed
fn mark_single_reviewed(cache: &mut Cache, item: &ReviewItem, timestamp: &str) -> Result<()> {
    let key = format!("@acp:{}", item.annotation);

    // Check if target is a file or symbol (symbol format: file:name)
    if item.target.contains(':') {
        // Symbol (format: file:name)
        let parts: Vec<&str> = item.target.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            let sym_name = parts[0];
            if let Some(sym) = cache.symbols.get_mut(sym_name) {
                if let Some(prov) = sym.annotations.get_mut(&key) {
                    prov.reviewed = true;
                    prov.needs_review = false;
                    prov.reviewed_at = Some(timestamp.to_string());
                }
            }
        }
    } else {
        // File
        if let Some(file) = cache.files.get_mut(&item.target) {
            if let Some(prov) = file.annotations.get_mut(&key) {
                prov.reviewed = true;
                prov.needs_review = false;
                prov.reviewed_at = Some(timestamp.to_string());
            }
        }
    }

    Ok(())
}

/// Recompute provenance statistics after modifications
fn recompute_provenance_stats(cache: &mut Cache) {
    let mut total = 0u64;
    let mut needs_review = 0u64;
    let mut reviewed = 0u64;
    let mut explicit = 0u64;
    let mut converted = 0u64;
    let mut heuristic = 0u64;
    let mut refined = 0u64;
    let mut inferred = 0u64;

    // Count file annotations
    for file in cache.files.values() {
        for prov in file.annotations.values() {
            total += 1;
            if prov.needs_review {
                needs_review += 1;
            }
            if prov.reviewed {
                reviewed += 1;
            }
            match prov.source {
                SourceOrigin::Explicit => explicit += 1,
                SourceOrigin::Converted => converted += 1,
                SourceOrigin::Heuristic => heuristic += 1,
                SourceOrigin::Refined => refined += 1,
                SourceOrigin::Inferred => inferred += 1,
            }
        }
    }

    // Count symbol annotations
    for sym in cache.symbols.values() {
        for prov in sym.annotations.values() {
            total += 1;
            if prov.needs_review {
                needs_review += 1;
            }
            if prov.reviewed {
                reviewed += 1;
            }
            match prov.source {
                SourceOrigin::Explicit => explicit += 1,
                SourceOrigin::Converted => converted += 1,
                SourceOrigin::Heuristic => heuristic += 1,
                SourceOrigin::Refined => refined += 1,
                SourceOrigin::Inferred => inferred += 1,
            }
        }
    }

    // Update stats
    cache.provenance.summary.total = total;
    cache.provenance.summary.needs_review = needs_review;
    cache.provenance.summary.reviewed = reviewed;
    cache.provenance.summary.by_source.explicit = explicit;
    cache.provenance.summary.by_source.converted = converted;
    cache.provenance.summary.by_source.heuristic = heuristic;
    cache.provenance.summary.by_source.refined = refined;
    cache.provenance.summary.by_source.inferred = inferred;
}

/// Truncate a string value for display
fn truncate_value(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
