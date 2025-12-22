//! @acp:module "Query Command"
//! @acp:summary "Query the cache for symbols, files, and domains"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::cache::Cache;
use crate::query::Query;

/// Options for the query command
#[derive(Debug, Clone)]
pub struct QueryOptions {
    /// Cache file to query
    pub cache: PathBuf,
    /// Output as JSON
    pub json: bool,
}

/// Query subcommand types
#[derive(Debug, Clone)]
pub enum QuerySubcommand {
    Symbol { name: String },
    File { path: String },
    Callers { symbol: String },
    Callees { symbol: String },
    Domains,
    Domain { name: String },
    Hotpaths,
    Stats,
}

/// Execute the query command
pub fn execute_query(options: QueryOptions, subcommand: QuerySubcommand) -> Result<()> {
    let cache_data = Cache::from_json(&options.cache)?;
    let q = Query::new(&cache_data);

    match subcommand {
        QuerySubcommand::Symbol { name } => query_symbol(&q, &name, options.json),
        QuerySubcommand::File { path } => query_file(&q, &cache_data, &path, options.json),
        QuerySubcommand::Callers { symbol } => query_callers(&q, &symbol, options.json),
        QuerySubcommand::Callees { symbol } => query_callees(&q, &symbol, options.json),
        QuerySubcommand::Domains => query_domains(&q, options.json),
        QuerySubcommand::Domain { name } => query_domain(&q, &name),
        QuerySubcommand::Hotpaths => query_hotpaths(&q),
        QuerySubcommand::Stats => query_stats(&cache_data, options.json),
    }
}

fn query_symbol(q: &Query, name: &str, json: bool) -> Result<()> {
    if let Some(sym) = q.symbol(name) {
        if json {
            println!("{}", serde_json::to_string_pretty(sym)?);
        } else {
            println!("{}", style(&sym.name).bold());
            println!("{}", "=".repeat(60));
            println!();

            // Location
            if sym.lines.len() >= 2 {
                println!("Location: {}:{}-{}", sym.file, sym.lines[0], sym.lines[1]);
            } else if !sym.lines.is_empty() {
                println!("Location: {}:{}", sym.file, sym.lines[0]);
            } else {
                println!("Location: {}", sym.file);
            }

            println!("Type:     {:?}", sym.symbol_type);

            if let Some(ref purpose) = sym.purpose {
                println!("Purpose:  {}", purpose);
            }

            if let Some(ref constraints) = sym.constraints {
                println!();
                println!("{}:", style("Constraints").bold());
                println!("  @acp:lock {} - {}", constraints.level, &constraints.directive);
            }

            if let Some(ref sig) = sym.signature {
                println!();
                println!("{}:", style("Signature").bold());
                println!("  {}", sig);
            }

            let callers = q.callers(name);
            if !callers.is_empty() {
                println!();
                println!("{} ({}):", style("Callers").bold(), callers.len());
                println!("  {}", callers.join(", "));
            }
        }
    } else {
        eprintln!("{} Symbol not found: {}", style("✗").red(), name);
    }
    Ok(())
}

fn query_file(q: &Query, cache_data: &Cache, path: &str, json: bool) -> Result<()> {
    if let Some(file) = q.file(path) {
        if json {
            println!("{}", serde_json::to_string_pretty(file)?);
        } else {
            println!("{}", style(&file.path).bold());
            println!("{}", "=".repeat(60));
            println!();

            println!("{}:", style("File Metadata").bold());

            if let Some(ref purpose) = file.purpose {
                println!("  Purpose:     {}", purpose);
            }

            println!("  Lines:       {}", file.lines);
            println!("  Language:    {:?}", file.language);

            if let Some(ref constraints) = cache_data.constraints {
                if let Some(fc) = constraints.by_file.get(&file.path) {
                    if let Some(ref mutation) = fc.mutation {
                        println!("  Constraint:  {:?}", mutation.level);
                    }
                }
            }

            if !file.exports.is_empty() {
                println!();
                println!("{}:", style("Symbols").bold());
                for sym_name in &file.exports {
                    if let Some(sym) = cache_data.symbols.get(sym_name) {
                        let sym_type = format!("{:?}", sym.symbol_type).to_lowercase();
                        let line_info = if sym.lines.len() >= 2 {
                            format!("{}:{}-{}", sym_type, sym.lines[0], sym.lines[1])
                        } else if !sym.lines.is_empty() {
                            format!("{}:{}", sym_type, sym.lines[0])
                        } else {
                            sym_type
                        };

                        let frozen = if sym
                            .constraints
                            .as_ref()
                            .map(|c| c.level == "frozen")
                            .unwrap_or(false)
                        {
                            " [frozen]"
                        } else {
                            ""
                        };
                        println!("  {} ({}){}", sym.name, line_info, frozen);
                    } else {
                        println!("  {}", sym_name);
                    }
                }
            }

            if !file.inline.is_empty() {
                println!();
                println!("{}:", style("Inline Annotations").bold());
                for ann in &file.inline {
                    let expires = ann
                        .expires
                        .as_ref()
                        .map(|e| format!(" (expires {})", e))
                        .unwrap_or_default();
                    println!(
                        "  Line {}: @acp:{} - {}{}",
                        ann.line, ann.annotation_type, ann.directive, expires
                    );
                }
            }
        }
    } else {
        eprintln!("{} File not found: {}", style("✗").red(), path);
    }
    Ok(())
}

fn query_callers(q: &Query, symbol: &str, json: bool) -> Result<()> {
    let callers = q.callers(symbol);
    if callers.is_empty() {
        println!("{} No callers found for {}", style("ℹ").cyan(), symbol);
    } else if json {
        println!("{}", serde_json::to_string_pretty(&callers)?);
    } else {
        for caller in callers {
            println!("{}", caller);
        }
    }
    Ok(())
}

fn query_callees(q: &Query, symbol: &str, json: bool) -> Result<()> {
    let callees = q.callees(symbol);
    if callees.is_empty() {
        println!("{} No callees found for {}", style("ℹ").cyan(), symbol);
    } else if json {
        println!("{}", serde_json::to_string_pretty(&callees)?);
    } else {
        for callee in callees {
            println!("{}", callee);
        }
    }
    Ok(())
}

fn query_domains(q: &Query, json: bool) -> Result<()> {
    let domains: Vec<_> = q.domains().collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&domains)?);
    } else {
        for domain in &domains {
            println!(
                "{}: {} files, {} symbols",
                style(&domain.name).cyan(),
                domain.files.len(),
                domain.symbols.len()
            );
        }
    }
    Ok(())
}

fn query_domain(q: &Query, name: &str) -> Result<()> {
    if let Some(domain) = q.domain(name) {
        println!("{}", serde_json::to_string_pretty(domain)?);
    } else {
        eprintln!("{} Domain not found: {}", style("✗").red(), name);
    }
    Ok(())
}

fn query_hotpaths(q: &Query) -> Result<()> {
    for hp in q.hotpaths() {
        println!("{}", hp);
    }
    Ok(())
}

fn query_stats(cache_data: &Cache, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&cache_data.stats)?);
    } else {
        println!("Files: {}", cache_data.stats.files);
        println!("Symbols: {}", cache_data.stats.symbols);
        println!("Lines: {}", cache_data.stats.lines);
        println!("Coverage: {:.1}%", cache_data.stats.annotation_coverage);
        println!("Domains: {}", cache_data.domains.len());
    }
    Ok(())
}
