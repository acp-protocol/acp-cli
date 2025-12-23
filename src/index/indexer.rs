//! @acp:module "Indexer"
//! @acp:summary "Codebase indexing and cache generation (schema-compliant, RFC-003 provenance)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Walks the codebase and builds the cache/vars files.
//! Uses tree-sitter AST parsing for symbol extraction and git2 for metadata.
//! Supports RFC-0003 annotation provenance tracking.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::fs;

use chrono::{DateTime, Utc};
use rayon::prelude::*;
use walkdir::WalkDir;
use glob::Pattern;

use crate::ast::{AstParser, ExtractedSymbol, SymbolKind, Visibility as AstVisibility};
use crate::cache::{
    Cache, CacheBuilder, DomainEntry, Language, SymbolEntry, SymbolType, Visibility,
    AnnotationProvenance, ProvenanceStats, LowConfidenceEntry,
};
use crate::config::Config;
use crate::constraints::{ConstraintIndex, Constraints, MutationConstraint, LockLevel, HackMarker, HackType};
use crate::error::Result;
use crate::git::{GitRepository, BlameInfo, FileHistory, GitFileInfo, GitSymbolInfo};
use crate::parse::{Parser, AnnotationWithProvenance, SourceOrigin};
use crate::vars::{VarsFile, VarEntry};

/// @acp:summary "Codebase indexer with parallel file processing"
/// Uses tree-sitter AST parsing for accurate symbol extraction and git2 for metadata.
pub struct Indexer {
    config: Config,
    parser: Arc<Parser>,
    ast_parser: Arc<AstParser>,
}

impl Indexer {
    pub fn new(config: Config) -> Result<Self> {
        Ok(Self {
            config,
            parser: Arc::new(Parser::new()),
            ast_parser: Arc::new(AstParser::new()?),
        })
    }

    /// @acp:summary "Index the codebase and generate cache"
    /// @acp:ai-careful "This processes many files in parallel"
    pub async fn index<P: AsRef<Path>>(&self, root: P) -> Result<Cache> {
        let root = root.as_ref();
        let project_name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());

        let mut builder = CacheBuilder::new(&project_name, &root.to_string_lossy());

        // Try to open git repository for metadata
        let git_repo = GitRepository::open(root).ok();

        // Set git commit if available
        if let Some(ref repo) = git_repo {
            if let Ok(commit) = repo.head_commit() {
                builder = builder.set_git_commit(commit);
            }
        }

        // Find all matching files
        let files = self.find_files(root)?;

        // Add source_files with modification times
        for file_path in &files {
            if let Ok(metadata) = fs::metadata(file_path) {
                if let Ok(modified) = metadata.modified() {
                    let modified_dt: DateTime<Utc> = modified.into();
                    let relative_path = Path::new(file_path)
                        .strip_prefix(root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| file_path.clone());
                    builder = builder.add_source_file(relative_path, modified_dt);
                }
            }
        }

        // Parse files in parallel using rayon
        // Uses annotation parser as primary for metadata, AST parser for accurate symbols
        let ast_parser = Arc::clone(&self.ast_parser);
        let annotation_parser = Arc::clone(&self.parser);
        let root_path = root.to_path_buf();

        // RFC-0003: Get review threshold from config (default 0.8)
        let review_threshold = 0.8; // TODO: Read from config when available

        let mut results: Vec<_> = files
            .par_iter()
            .filter_map(|path| {
                // Parse with annotation parser (metadata, domains, etc.)
                let mut parse_result = annotation_parser.parse(path).ok()?;

                // Try AST parsing for accurate symbol extraction
                if let Ok(source) = std::fs::read_to_string(path) {
                    // RFC-0003: Parse annotations with provenance support
                    let annotations_with_prov = annotation_parser.parse_annotations_with_provenance(&source);
                    let file_provenance = extract_provenance(&annotations_with_prov, review_threshold);

                    // Add provenance to file entry
                    parse_result.file.annotations = file_provenance;

                    if let Ok(ast_symbols) = ast_parser.parse_file(Path::new(path), &source) {
                        // Convert AST symbols to cache symbols and merge
                        let relative_path = Path::new(path)
                            .strip_prefix(&root_path)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| path.clone());

                        let converted = convert_ast_symbols(&ast_symbols, &relative_path);

                        // Merge: prefer AST symbols but keep annotation metadata
                        if !converted.is_empty() {
                            // Keep summaries from annotation parser
                            let annotation_summaries: HashMap<_, _> =
                                parse_result.symbols.iter()
                                    .filter_map(|s| s.summary.as_ref().map(|sum| (s.name.clone(), sum.clone())))
                                    .collect();

                            parse_result.symbols = converted;

                            // Restore summaries from annotations
                            for symbol in &mut parse_result.symbols {
                                if symbol.summary.is_none() {
                                    if let Some(sum) = annotation_summaries.get(&symbol.name) {
                                        symbol.summary = Some(sum.clone());
                                    }
                                }
                            }
                        }

                        // Extract calls from AST
                        if let Ok(calls) = ast_parser.parse_calls(Path::new(path), &source) {
                            for call in calls {
                                if !call.caller.is_empty() {
                                    parse_result.calls.push((call.caller.clone(), vec![call.callee.clone()]));
                                }
                            }
                        }
                    }
                }

                Some(parse_result)
            })
            .collect();

        // Add git metadata sequentially (git2::Repository is not Sync)
        if let Some(ref repo) = git_repo {
            for parse_result in &mut results {
                let file_path = &parse_result.file.path;
                // Strip "./" prefix if present - git expects paths like "src/lib.rs" not "./src/lib.rs"
                let clean_path = file_path.strip_prefix("./").unwrap_or(file_path);
                let relative_path = Path::new(clean_path);

                // Add git metadata for the file (only if we have valid git history)
                if let Ok(history) = FileHistory::for_file(repo, relative_path, 100) {
                    if let Some(latest) = history.latest() {
                        // Only set git info if we have actual commit data
                        parse_result.file.git = Some(GitFileInfo {
                            last_commit: latest.commit.clone(),
                            last_author: latest.author.clone(),
                            last_modified: latest.timestamp,
                            commit_count: history.commit_count(),
                            contributors: history.contributors(),
                        });
                    }
                }

                // Add git metadata for symbols using blame
                if let Ok(blame) = BlameInfo::for_file(repo, relative_path) {
                    for symbol in &mut parse_result.symbols {
                        if let Some(line_blame) = blame.last_modified(symbol.lines[0], symbol.lines[1]) {
                            let age_days = (Utc::now() - line_blame.timestamp).num_days().max(0) as u32;
                            symbol.git = Some(GitSymbolInfo {
                                last_commit: line_blame.commit.clone(),
                                last_author: line_blame.author.clone(),
                                code_age_days: age_days,
                            });
                        }
                    }
                }
            }
        }

        // Build cache from results
        let mut domains: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut constraint_index = ConstraintIndex::default();

        for result in &results {
            // Add file
            builder = builder.add_file(result.file.clone());

            // Add symbols
            for symbol in &result.symbols {
                builder = builder.add_symbol(symbol.clone());
            }

            // Add call edges
            for (from, to) in &result.calls {
                builder = builder.add_call_edge(from, to.clone());
            }

            // Track domains
            for domain in &result.file.domains {
                domains
                    .entry(domain.clone())
                    .or_default()
                    .push(result.file.path.clone());
            }

            // Build constraints from parse result (RFC-001 compliant)
            if result.lock_level.is_some() || !result.ai_hints.is_empty() {
                let lock_level = result.lock_level.as_ref().map(|l| {
                    match l.to_lowercase().as_str() {
                        "frozen" => LockLevel::Frozen,
                        "restricted" => LockLevel::Restricted,
                        "approval-required" => LockLevel::ApprovalRequired,
                        "tests-required" => LockLevel::TestsRequired,
                        "docs-required" => LockLevel::DocsRequired,
                        "experimental" => LockLevel::Experimental,
                        _ => LockLevel::Normal,
                    }
                }).unwrap_or(LockLevel::Normal);

                let constraints = Constraints {
                    mutation: Some(MutationConstraint {
                        level: lock_level,
                        reason: None,
                        contact: None,
                        requires_approval: matches!(lock_level, LockLevel::ApprovalRequired),
                        requires_tests: matches!(lock_level, LockLevel::TestsRequired),
                        requires_docs: matches!(lock_level, LockLevel::DocsRequired),
                        max_lines_changed: None,
                        allowed_operations: None,
                        forbidden_operations: None,
                    }),
                    // RFC-001: Include directive from lock annotation
                    directive: result.lock_directive.clone(),
                    auto_generated: result.lock_directive.is_none(),
                    ..Default::default()
                };
                constraint_index.by_file.insert(result.file.path.clone(), constraints);

                // Track by lock level
                let level_str = format!("{:?}", lock_level).to_lowercase();
                constraint_index.by_lock_level
                    .entry(level_str)
                    .or_default()
                    .push(result.file.path.clone());
            }

            // Build hack markers
            for hack in &result.hacks {
                let hack_marker = HackMarker {
                    id: format!("{}:{}", result.file.path, hack.line),
                    hack_type: HackType::Workaround,
                    file: result.file.path.clone(),
                    line: Some(hack.line),
                    created_at: Utc::now(),
                    author: None,
                    reason: hack.reason.clone().unwrap_or_else(|| "Temporary hack".to_string()),
                    ticket: hack.ticket.clone(),
                    expires: hack.expires.as_ref().and_then(|e| {
                        chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d")
                            .ok()
                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                    }),
                    original_code: None,
                    revert_instructions: None,
                };
                constraint_index.hacks.push(hack_marker);
            }
        }

        // Add domains to cache
        for (name, files) in domains {
            builder = builder.add_domain(DomainEntry {
                name: name.clone(),
                files: files.clone(),
                symbols: vec![],
                description: None,
            });
        }

        // Add constraints if any were found
        if !constraint_index.by_file.is_empty() || !constraint_index.hacks.is_empty() {
            builder = builder.set_constraints(constraint_index);
        }

        // Build the cache
        let mut cache = builder.build();

        // RFC-0003: Compute provenance statistics
        let low_conf_threshold = 0.5; // TODO: Read from config when available
        cache.provenance = compute_provenance_stats(&cache, low_conf_threshold);

        Ok(cache)
    }

    /// @acp:summary "Find all files matching include/exclude patterns"
    fn find_files<P: AsRef<Path>>(&self, root: P) -> Result<Vec<String>> {
        let root = root.as_ref();
        let include_patterns: Vec<_> = self.config.include
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        let exclude_patterns: Vec<_> = self.config.exclude
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        let files: Vec<String> = WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                // Get path relative to root for pattern matching
                let full_path = e.path().to_string_lossy().to_string();
                let relative_path = e.path()
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| full_path.clone());

                // Must match at least one include pattern
                let match_opts = glob::MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                };
                let included = include_patterns.is_empty() ||
                    include_patterns.iter().any(|p| p.matches_with(&relative_path, match_opts));
                // Must not match any exclude pattern
                let excluded = exclude_patterns.iter().any(|p| p.matches_with(&relative_path, match_opts));

                if included && !excluded {
                    Some(full_path)
                } else {
                    None
                }
            })
            .collect();

        Ok(files)
    }

    /// @acp:summary "Generate vars file from cache (schema-compliant)"
    pub fn generate_vars(&self, cache: &Cache) -> VarsFile {
        let mut vars_file = VarsFile::new();

        // Build a map of symbol names to var names for ref resolution
        let mut symbol_to_var: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (name, symbol) in &cache.symbols {
            if symbol.exported {
                let var_name = format!("SYM_{}", name.to_uppercase().replace('.', "_"));
                symbol_to_var.insert(name.clone(), var_name);
            }
        }

        // Generate symbol vars with refs from call graph
        for (name, symbol) in &cache.symbols {
            if symbol.exported {
                let var_name = format!("SYM_{}", name.to_uppercase().replace('.', "_"));

                // Build refs from symbols this one calls
                let refs: Vec<String> = symbol.calls.iter()
                    .filter_map(|callee| symbol_to_var.get(callee).cloned())
                    .collect();

                let entry = VarEntry {
                    var_type: crate::vars::VarType::Symbol,
                    value: symbol.qualified_name.clone(),
                    description: symbol.summary.clone(),
                    refs,
                    source: Some(symbol.file.clone()),
                    lines: Some(symbol.lines),
                };

                vars_file.add_variable(var_name, entry);
            }
        }

        // Generate domain vars
        for (name, domain) in &cache.domains {
            let var_name = format!("DOM_{}", name.to_uppercase().replace('-', "_"));
            vars_file.add_variable(
                var_name,
                VarEntry::domain(
                    name.clone(),
                    Some(format!("Domain: {} ({} files)", name, domain.files.len())),
                ),
            );
        }

        // Generate file vars for important files
        for (path, file) in &cache.files {
            // Only generate vars for files with modules or summaries
            if file.module.is_some() || file.summary.is_some() {
                let var_name = format!("FILE_{}",
                    path.replace(['/', '.'], "_")
                        .to_uppercase());
                vars_file.add_variable(
                    var_name,
                    VarEntry::file(
                        path.clone(),
                        file.summary.clone().or_else(|| file.module.clone()),
                    ),
                );
            }
        }

        // Generate layer vars from unique layers
        let mut layers: std::collections::HashSet<String> = std::collections::HashSet::new();
        for file in cache.files.values() {
            if let Some(layer) = &file.layer {
                layers.insert(layer.clone());
            }
        }
        for layer in layers {
            let var_name = format!("LAYER_{}", layer.to_uppercase().replace('-', "_"));
            let file_count = cache.files.values()
                .filter(|f| f.layer.as_ref() == Some(&layer))
                .count();
            vars_file.add_variable(
                var_name,
                VarEntry::layer(
                    layer.clone(),
                    Some(format!("Layer: {} ({} files)", layer, file_count)),
                ),
            );
        }

        vars_file
    }
}

/// Detect language from file extension
pub fn detect_language(path: &str) -> Option<Language> {
    let path = Path::new(path);
    let ext = path.extension()?.to_str()?;

    match ext.to_lowercase().as_str() {
        "ts" | "tsx" => Some(Language::Typescript),
        "js" | "jsx" | "mjs" | "cjs" => Some(Language::Javascript),
        "py" | "pyw" => Some(Language::Python),
        "rs" => Some(Language::Rust),
        "go" => Some(Language::Go),
        "java" => Some(Language::Java),
        "cs" => Some(Language::CSharp),
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" => Some(Language::Cpp),
        "c" | "h" => Some(Language::C),
        "rb" => Some(Language::Ruby),
        "php" => Some(Language::Php),
        "swift" => Some(Language::Swift),
        "kt" | "kts" => Some(Language::Kotlin),
        _ => None,
    }
}

/// Convert AST-extracted symbols to cache SymbolEntry format
fn convert_ast_symbols(ast_symbols: &[ExtractedSymbol], file_path: &str) -> Vec<SymbolEntry> {
    ast_symbols.iter().map(|sym| {
        let symbol_type = match sym.kind {
            SymbolKind::Function => SymbolType::Function,
            SymbolKind::Method => SymbolType::Method,
            SymbolKind::Class => SymbolType::Class,
            SymbolKind::Struct => SymbolType::Struct,
            SymbolKind::Interface => SymbolType::Interface,
            SymbolKind::Trait => SymbolType::Trait,
            SymbolKind::Enum => SymbolType::Enum,
            SymbolKind::EnumVariant => SymbolType::Enum,
            SymbolKind::Constant => SymbolType::Const,
            SymbolKind::Variable => SymbolType::Const,
            SymbolKind::TypeAlias => SymbolType::Type,
            SymbolKind::Module => SymbolType::Function, // No direct mapping
            SymbolKind::Namespace => SymbolType::Function, // No direct mapping
            SymbolKind::Property => SymbolType::Function, // No direct mapping
            SymbolKind::Field => SymbolType::Function, // No direct mapping
            SymbolKind::Impl => SymbolType::Class, // Map impl to class
        };

        let visibility = match sym.visibility {
            AstVisibility::Public => Visibility::Public,
            AstVisibility::Private => Visibility::Private,
            AstVisibility::Protected => Visibility::Protected,
            AstVisibility::Internal | AstVisibility::Crate => Visibility::Private,
        };

        let qualified_name = sym.qualified_name.clone()
            .unwrap_or_else(|| format!("{}:{}", file_path, sym.name));

        SymbolEntry {
            name: sym.name.clone(),
            qualified_name,
            symbol_type,
            file: file_path.to_string(),
            lines: [sym.start_line, sym.end_line],
            exported: matches!(sym.visibility, AstVisibility::Public),
            signature: sym.signature.clone(),
            summary: sym.doc_comment.clone(),
            purpose: None, // RFC-001: Populated from @acp:fn/@acp:class annotations
            constraints: None, // RFC-001: Populated from symbol-level constraints
            async_fn: sym.is_async,
            visibility,
            calls: vec![], // Populated separately from call graph
            called_by: vec![], // Populated by graph builder
            git: None, // Populated after symbol creation
            annotations: HashMap::new(), // RFC-0003: Populated during indexing
        }
    }).collect()
}

// ============================================================================
// RFC-0003: Annotation Provenance Functions
// ============================================================================

/// Extract provenance data from parsed annotations (RFC-0003)
///
/// Converts AnnotationWithProvenance to AnnotationProvenance entries
/// suitable for storage in the cache.
fn extract_provenance(
    annotations: &[AnnotationWithProvenance],
    review_threshold: f64,
) -> HashMap<String, AnnotationProvenance> {
    let mut result = HashMap::new();

    for ann in annotations {
        // Skip provenance-only annotations (source, source-confidence, etc.)
        if ann.annotation.name.starts_with("source") {
            continue;
        }

        let key = format!("@acp:{}", ann.annotation.name);

        let prov = if let Some(ref marker) = ann.provenance {
            let needs_review = marker.confidence.map_or(false, |c| c < review_threshold);

            AnnotationProvenance {
                value: ann.annotation.value.clone().unwrap_or_default(),
                source: marker.source,
                confidence: marker.confidence,
                needs_review,
                reviewed: marker.reviewed.unwrap_or(false),
                reviewed_at: None,
                generated_at: Some(Utc::now().to_rfc3339()),
                generation_id: marker.generation_id.clone(),
            }
        } else {
            // No provenance markers = explicit annotation (human-written)
            AnnotationProvenance {
                value: ann.annotation.value.clone().unwrap_or_default(),
                source: SourceOrigin::Explicit,
                confidence: None,
                needs_review: false,
                reviewed: true, // Explicit annotations are considered reviewed
                reviewed_at: None,
                generated_at: None,
                generation_id: None,
            }
        };

        result.insert(key, prov);
    }

    result
}

/// Compute aggregate provenance statistics from cache (RFC-0003)
///
/// Aggregates provenance data from all files and symbols to produce
/// summary statistics for the cache.
fn compute_provenance_stats(cache: &Cache, low_conf_threshold: f64) -> ProvenanceStats {
    let mut stats = ProvenanceStats::default();
    let mut confidence_sums: HashMap<String, (f64, u64)> = HashMap::new();

    // Process file annotations
    for (path, file) in &cache.files {
        for (key, prov) in &file.annotations {
            update_provenance_stats(
                &mut stats,
                &mut confidence_sums,
                key,
                prov,
                path,
                low_conf_threshold,
            );
        }
    }

    // Process symbol annotations
    for symbol in cache.symbols.values() {
        for (key, prov) in &symbol.annotations {
            let target = format!("{}:{}", symbol.file, symbol.name);
            update_provenance_stats(
                &mut stats,
                &mut confidence_sums,
                key,
                prov,
                &target,
                low_conf_threshold,
            );
        }
    }

    // Calculate average confidence per source type
    for (source, (sum, count)) in confidence_sums {
        if count > 0 {
            stats.summary.average_confidence.insert(source, sum / count as f64);
        }
    }

    // Sort low confidence entries by confidence (ascending)
    stats.low_confidence.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    stats
}

/// Update provenance statistics with a single annotation's data
fn update_provenance_stats(
    stats: &mut ProvenanceStats,
    confidence_sums: &mut HashMap<String, (f64, u64)>,
    key: &str,
    prov: &AnnotationProvenance,
    target: &str,
    low_conf_threshold: f64,
) {
    stats.summary.total += 1;

    // Count by source type
    match prov.source {
        SourceOrigin::Explicit => stats.summary.by_source.explicit += 1,
        SourceOrigin::Converted => stats.summary.by_source.converted += 1,
        SourceOrigin::Heuristic => stats.summary.by_source.heuristic += 1,
        SourceOrigin::Refined => stats.summary.by_source.refined += 1,
        SourceOrigin::Inferred => stats.summary.by_source.inferred += 1,
    }

    // Count review status
    if prov.needs_review {
        stats.summary.needs_review += 1;
    }
    if prov.reviewed {
        stats.summary.reviewed += 1;
    }

    // Track confidence for averaging
    if let Some(conf) = prov.confidence {
        let source_key = prov.source.as_str().to_string();
        let entry = confidence_sums.entry(source_key).or_insert((0.0, 0));
        entry.0 += conf;
        entry.1 += 1;

        // Track low confidence annotations
        if conf < low_conf_threshold {
            stats.low_confidence.push(LowConfidenceEntry {
                target: target.to_string(),
                annotation: key.to_string(),
                confidence: conf,
                value: prov.value.clone(),
            });
        }
    }
}
