//! @acp:module "Annotate Command"
//! @acp:summary "Analyze and suggest ACP annotations (RFC-003 provenance support)"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Implements `acp annotate` command for annotation analysis and generation.
//! Supports RFC-0003 annotation provenance tracking.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use console::style;
use rand::Rng;
use rayon::prelude::*;

use crate::annotate::{
    Analyzer, AnnotateLevel, ConversionSource, OutputFormat, ProvenanceConfig, Suggester, Writer,
};
use crate::config::Config;
use crate::git::GitRepository;

/// Options for the annotate command
#[derive(Debug, Clone)]
pub struct AnnotateOptions {
    /// Path to analyze
    pub path: PathBuf,
    /// Apply changes directly
    pub apply: bool,
    /// Convert from existing doc format only (no heuristics)
    pub convert: bool,
    /// Source format for conversion
    pub from: ConversionSource,
    /// Annotation detail level
    pub level: AnnotateLevel,
    /// Output format
    pub format: OutputFormat,
    /// Filter by path pattern
    pub filter: Option<String>,
    /// Only process file-level annotations
    pub files_only: bool,
    /// Only process symbol-level annotations
    pub symbols_only: bool,
    /// CI mode - check coverage threshold
    pub check: bool,
    /// Minimum coverage threshold for CI mode
    pub min_coverage: Option<f32>,
    /// Number of parallel workers
    pub workers: Option<usize>,
    /// Verbose output
    pub verbose: bool,
    /// RFC-0003: Disable provenance markers
    pub no_provenance: bool,
    /// RFC-0003: Mark all generated annotations as needing review
    pub mark_needs_review: bool,
}

impl Default for AnnotateOptions {
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            apply: false,
            convert: false,
            from: ConversionSource::Auto,
            level: AnnotateLevel::Standard,
            format: OutputFormat::Diff,
            filter: None,
            files_only: false,
            symbols_only: false,
            check: false,
            min_coverage: None,
            workers: None,
            verbose: false,
            no_provenance: false,
            mark_needs_review: false,
        }
    }
}

/// Generate a unique generation ID for annotation batches (RFC-0003)
///
/// Format: `gen-YYYYMMDD-HHMMSS-XXXX` where XXXX is a random hex string
fn generate_generation_id() -> String {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let random_suffix: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(4)
        .map(char::from)
        .collect();
    format!("gen-{}-{}", timestamp, random_suffix.to_lowercase())
}

/// Execute the annotate command
pub fn execute_annotate(options: AnnotateOptions, config: Config) -> Result<()> {
    // Configure thread pool if workers specified
    if let Some(num_workers) = options.workers {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_workers)
            .build_global()
            .ok(); // Ignore error if already initialized
    }

    println!(
        "{} Analyzing codebase for annotations...",
        style("→").cyan()
    );

    // Create analyzer and suggester
    // When --convert is set, only use documentation conversion (no heuristics)
    let analyzer = Arc::new(Analyzer::new(&config)?.with_level(options.level));
    let suggester = Arc::new(
        Suggester::new(options.level)
            .with_conversion_source(options.from)
            .with_heuristics(!options.convert),
    );

    // RFC-0003: Create provenance config if enabled
    // CLI --no-provenance flag overrides config setting
    let provenance_enabled = if options.no_provenance {
        false
    } else {
        config.annotate.provenance.enabled
    };

    // CLI --mark-needs-review flag overrides config setting
    let mark_needs_review = options.mark_needs_review || config.annotate.defaults.mark_needs_review;

    let provenance_config = if provenance_enabled {
        let generation_id = generate_generation_id();
        if options.verbose {
            eprintln!("Provenance generation ID: {}", generation_id);
            eprintln!(
                "  Review threshold: {:.0}%",
                config.annotate.provenance.review_threshold * 100.0
            );
            eprintln!(
                "  Min confidence: {:.0}%",
                config.annotate.provenance.min_confidence * 100.0
            );
        }
        Some(
            ProvenanceConfig::new()
                .with_generation_id(generation_id)
                .with_needs_review(mark_needs_review)
                .with_review_threshold(config.annotate.provenance.review_threshold as f32)
                .with_min_confidence(config.annotate.provenance.min_confidence as f32),
        )
    } else {
        None
    };

    // Create writer with optional provenance config
    let writer = if let Some(config) = provenance_config {
        Writer::new().with_provenance(config)
    } else {
        Writer::new()
    };

    // Discover files
    let files = analyzer.discover_files(&options.path, options.filter.as_deref())?;

    if options.verbose {
        eprintln!("Found {} files to analyze", files.len());
    }

    // Warn if conversion source doesn't match detected file types
    if options.convert && options.from != ConversionSource::Auto {
        let mut mismatched_extensions = std::collections::HashSet::new();
        for file_path in &files {
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                let is_mismatch = match options.from {
                    ConversionSource::Jsdoc | ConversionSource::Tsdoc => {
                        !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
                    }
                    ConversionSource::Docstring => !matches!(ext, "py" | "pyi"),
                    ConversionSource::Rustdoc => ext != "rs",
                    ConversionSource::Godoc => ext != "go",
                    ConversionSource::Javadoc => ext != "java",
                    ConversionSource::Auto => false,
                };
                if is_mismatch {
                    mismatched_extensions.insert(ext.to_string());
                }
            }
        }

        if !mismatched_extensions.is_empty() {
            let expected = match options.from {
                ConversionSource::Jsdoc => ".ts, .tsx, .js, .jsx",
                ConversionSource::Tsdoc => ".ts, .tsx",
                ConversionSource::Docstring => ".py, .pyi",
                ConversionSource::Rustdoc => ".rs",
                ConversionSource::Godoc => ".go",
                ConversionSource::Javadoc => ".java",
                ConversionSource::Auto => "any",
            };
            eprintln!(
                "{} Warning: --convert {:?} is intended for {} files, but found files with extensions: {}",
                style("⚠").yellow(),
                options.from,
                expected,
                mismatched_extensions.into_iter().collect::<Vec<_>>().join(", ")
            );
            eprintln!(
                "{}  Consider using --convert auto or the appropriate source for your file types",
                style("→").cyan()
            );
        }
    }

    // Clone path for parallel access
    let repo_path = options.path.clone();

    // Process files in parallel
    let results: Vec<_> = files
        .par_iter()
        .filter_map(|file_path| {
            // Analyze file
            let analysis = match analyzer.analyze_file(file_path) {
                Ok(a) => a,
                Err(_) => return None,
            };

            // Open git repo per-thread for thread safety
            let git_repo = GitRepository::open(&repo_path).ok();

            // Generate suggestions (with git-based heuristics if repo is available)
            let mut suggestions = suggester.suggest_with_git(&analysis, git_repo.as_ref());

            // Filter by scope
            if options.files_only {
                suggestions.retain(|s| s.is_file_level());
            }
            if options.symbols_only {
                suggestions.retain(|s| !s.is_file_level());
            }

            // Filter by minimum confidence (from config)
            let min_conf = config.annotate.provenance.min_confidence as f32;
            suggestions.retain(|s| s.confidence >= min_conf);

            Some((file_path.clone(), analysis, suggestions))
        })
        .collect();

    // Aggregate results
    let mut total_suggestions = 0;
    let mut files_with_changes = 0;
    let mut all_changes = Vec::new();
    let mut all_results = Vec::new();

    for (file_path, analysis, suggestions) in results {
        all_results.push(analysis.clone());

        if !suggestions.is_empty() {
            files_with_changes += 1;
            total_suggestions += suggestions.len();

            let changes = writer.plan_changes(&file_path, &suggestions, &analysis)?;
            all_changes.push((file_path, changes));
        }
    }

    // Calculate statistics for output
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    let mut source_counts: HashMap<String, usize> = HashMap::new();
    let mut total_confidence: f32 = 0.0;
    let mut suggestion_count: usize = 0;

    for (_, changes) in &all_changes {
        for change in changes {
            for suggestion in &change.annotations {
                let type_name = format!("{:?}", suggestion.annotation_type).to_lowercase();
                *type_counts.entry(type_name).or_insert(0) += 1;

                let source_name = format!("{:?}", suggestion.source);
                *source_counts.entry(source_name).or_insert(0) += 1;

                total_confidence += suggestion.confidence;
                suggestion_count += 1;
            }
        }
    }

    let avg_confidence = if suggestion_count > 0 {
        total_confidence / suggestion_count as f32
    } else {
        0.0
    };

    let coverage = Analyzer::calculate_total_coverage(&all_results);

    // Output results
    match options.format {
        OutputFormat::Diff => {
            for (file_path, changes) in &all_changes {
                let diff = writer.generate_diff(file_path, changes)?;
                if !diff.is_empty() {
                    println!("{}", diff);
                }
            }
        }
        OutputFormat::Json => {
            let output = serde_json::json!({
                "summary": {
                    "files_analyzed": files.len(),
                    "files_with_suggestions": files_with_changes,
                    "total_suggestions": total_suggestions,
                    "coverage_percent": coverage,
                    "average_confidence": (avg_confidence * 100.0).round() / 100.0,
                },
                "breakdown": {
                    "by_type": type_counts,
                    "by_source": source_counts,
                },
                "files": all_changes.iter().map(|(path, changes)| {
                    let file_suggestions: Vec<_> = changes.iter().flat_map(|c| {
                        c.annotations.iter().map(|s| {
                            serde_json::json!({
                                "target": c.symbol_name.as_deref().unwrap_or("(file)"),
                                "line": s.line,
                                "type": format!("{:?}", s.annotation_type).to_lowercase(),
                                "value": s.value,
                                "source": format!("{:?}", s.source),
                                "confidence": (s.confidence * 100.0).round() / 100.0,
                            })
                        }).collect::<Vec<_>>()
                    }).collect();

                    serde_json::json!({
                        "path": path.display().to_string(),
                        "suggestion_count": file_suggestions.len(),
                        "suggestions": file_suggestions,
                    })
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Summary => {
            println!("\n{}", style("Annotation Summary").bold());
            println!("==================");
            println!("Files analyzed:          {}", files.len());
            println!("Files with suggestions:  {}", files_with_changes);
            println!("Total suggestions:       {}", total_suggestions);
            println!("Current coverage:        {:.1}%", coverage);
            println!("Avg confidence:          {:.0}%", avg_confidence * 100.0);

            // Show breakdown by annotation type
            if !type_counts.is_empty() {
                println!("\n{}", style("By Annotation Type").bold());
                println!("------------------");
                let mut sorted_types: Vec<_> = type_counts.iter().collect();
                sorted_types.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending
                for (type_name, count) in sorted_types {
                    println!("  @acp:{:<14} {}", type_name, count);
                }
            }

            // Show breakdown by source
            if !source_counts.is_empty() && total_suggestions > 0 {
                println!("\n{}", style("By Suggestion Source").bold());
                println!("--------------------");
                for (source_name, count) in &source_counts {
                    let pct = (*count as f32 / total_suggestions as f32) * 100.0;
                    println!("  {:<20} {} ({:.0}%)", source_name, count, pct);
                }
            }

            if options.verbose {
                println!("\n{}", style("File Details").bold());
                println!("------------");
                for (file_path, changes) in &all_changes {
                    println!("\n{}:", file_path.display());
                    for change in changes {
                        let target = change.symbol_name.as_deref().unwrap_or("(file)");
                        println!(
                            "  - {} @ line {}: {} annotations",
                            target,
                            change.line,
                            change.annotations.len()
                        );
                    }
                }
            }
        }
    }

    // Apply changes if requested
    if options.apply {
        for (file_path, changes) in &all_changes {
            writer.apply_changes(file_path, changes)?;
            if options.verbose {
                eprintln!("Updated: {}", file_path.display());
            }
        }
        eprintln!(
            "\n{} Applied {} suggestions to {} files",
            style("✓").green(),
            total_suggestions,
            files_with_changes
        );
    } else if !options.check && total_suggestions > 0 {
        eprintln!("\nRun with {} to write changes", style("--apply").cyan());
    }

    // CI mode: exit with error if coverage below threshold
    if options.check {
        let coverage = Analyzer::calculate_total_coverage(&all_results);
        let threshold = options.min_coverage.unwrap_or(80.0);

        if coverage < threshold {
            eprintln!(
                "\n{} Coverage {:.1}% is below threshold {:.1}%",
                style("✗").red(),
                coverage,
                threshold
            );
            std::process::exit(1);
        } else {
            println!(
                "\n{} Coverage {:.1}% meets threshold {:.1}%",
                style("✓").green(),
                coverage,
                threshold
            );
        }
    }

    Ok(())
}
