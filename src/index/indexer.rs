//! @acp:module "Indexer"
//! @acp:summary "Codebase indexing and cache generation (schema-compliant, RFC-003 provenance, RFC-006 bridging)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Walks the codebase and builds the cache/vars files.
//! Uses tree-sitter AST parsing for symbol extraction and git2 for metadata.
//! Supports RFC-0003 annotation provenance tracking.
//! Supports RFC-0006 documentation system bridging.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::fs;

use chrono::{DateTime, Utc};
use rayon::prelude::*;
use walkdir::WalkDir;
use glob::Pattern;

use crate::annotate::converters::{
    DocStandardParser, ParsedDocumentation,
    JsDocParser, DocstringParser, RustdocParser, GodocParser, JavadocParser,
};
use crate::ast::{AstParser, ExtractedSymbol, SymbolKind, Visibility as AstVisibility};
use crate::bridge::{BridgeConfig, FormatDetector, BridgeMerger};
use crate::bridge::merger::AcpAnnotations;
use crate::cache::{
    Cache, CacheBuilder, DomainEntry, Language, SymbolEntry, SymbolType, Visibility,
    AnnotationProvenance, ProvenanceStats, LowConfidenceEntry,
    BridgeMetadata, BridgeStats, BridgeSummary, SourceFormat, BridgeSource,
};
use crate::config::Config;
use crate::constraints::{ConstraintIndex, Constraints, MutationConstraint, LockLevel, HackMarker, HackType};
use crate::error::Result;
use crate::git::{GitRepository, BlameInfo, FileHistory, GitFileInfo, GitSymbolInfo};
use crate::parse::{Parser, AnnotationWithProvenance, SourceOrigin};
use crate::vars::{VarsFile, VarEntry};

/// @acp:summary "Codebase indexer with parallel file processing"
/// Uses tree-sitter AST parsing for accurate symbol extraction and git2 for metadata.
/// Supports RFC-0006 documentation bridging.
pub struct Indexer {
    config: Config,
    parser: Arc<Parser>,
    ast_parser: Arc<AstParser>,
    /// RFC-0006: Format detector for native documentation
    format_detector: Arc<FormatDetector>,
    /// RFC-0006: Merger for native docs with ACP annotations
    bridge_merger: Arc<BridgeMerger>,
}

impl Indexer {
    pub fn new(config: Config) -> Result<Self> {
        // RFC-0006: Initialize bridge components
        let format_detector = FormatDetector::new(&config.bridge);
        let bridge_merger = BridgeMerger::new(&config.bridge);

        Ok(Self {
            config,
            parser: Arc::new(Parser::new()),
            ast_parser: Arc::new(AstParser::new()?),
            format_detector: Arc::new(format_detector),
            bridge_merger: Arc::new(bridge_merger),
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

        // RFC-0003: Get review threshold from config
        let review_threshold = self.config.annotate.provenance.review_threshold;

        // RFC-0006: Clone bridge components for parallel access
        let format_detector = Arc::clone(&self.format_detector);
        let bridge_merger = Arc::clone(&self.bridge_merger);
        let bridge_enabled = self.config.bridge.enabled;

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

                    // RFC-0006: Detect documentation format and populate bridge metadata
                    if bridge_enabled {
                        let language = language_name_from_enum(parse_result.file.language);
                        let detected_format = format_detector.detect(&source, language);

                        // Initialize bridge metadata
                        parse_result.file.bridge = BridgeMetadata {
                            enabled: true,
                            detected_format,
                            converted_count: 0,
                            merged_count: 0,
                            explicit_count: 0,
                        };

                        // Count explicit ACP annotations
                        let explicit_count = parse_result.file.annotations.values()
                            .filter(|p| matches!(p.source, SourceOrigin::Explicit))
                            .count() as u64;
                        parse_result.file.bridge.explicit_count = explicit_count;

                        // Count converted annotations (from provenance tracking)
                        let converted_count = parse_result.file.annotations.values()
                            .filter(|p| matches!(p.source, SourceOrigin::Converted))
                            .count() as u64;
                        parse_result.file.bridge.converted_count = converted_count;
                    }

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

                            // RFC-0006: Apply bridge merging for symbols with doc comments
                            if bridge_enabled {
                                if let Some(ref detected_format) = parse_result.file.bridge.detected_format {
                                    // Build map of AST symbols by name for doc_comment lookup
                                    let ast_doc_comments: HashMap<_, _> = ast_symbols.iter()
                                        .filter_map(|s| s.doc_comment.as_ref().map(|doc| (s.name.clone(), doc.clone())))
                                        .collect();

                                    let mut merged_count = 0u64;
                                    for symbol in &mut parse_result.symbols {
                                        if let Some(doc_comment) = ast_doc_comments.get(&symbol.name) {
                                            // Parse native documentation
                                            let native_docs = parse_native_docs(doc_comment, detected_format);

                                            // Extract ACP annotations from doc comment
                                            let acp_annotations = extract_acp_annotations(doc_comment, &annotation_parser);

                                            // Merge using bridge merger
                                            let bridge_result = bridge_merger.merge(
                                                native_docs.as_ref(),
                                                *detected_format,
                                                &acp_annotations,
                                            );

                                            // Update symbol with merged data
                                            if bridge_result.summary.is_some() {
                                                symbol.summary = bridge_result.summary;
                                            }
                                            if bridge_result.directive.is_some() {
                                                symbol.purpose = bridge_result.directive;
                                            }

                                            // Track merged count
                                            if matches!(bridge_result.source, BridgeSource::Merged) {
                                                merged_count += 1;
                                            }
                                        }
                                    }
                                    parse_result.file.bridge.merged_count = merged_count;
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

        // RFC-0006: Compute bridge statistics
        cache.bridge = compute_bridge_stats(&cache, &self.config.bridge);

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
            // RFC-0009: Extended annotation types
            behavioral: None,
            lifecycle: None,
            documentation: None,
            performance: None,
            // RFC-0008: Type annotation info
            type_info: None,
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
            let needs_review = marker.confidence.is_some_and(|c| c < review_threshold);

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

// ============================================================================
// RFC-0006: Documentation Bridging Functions
// ============================================================================

/// Convert Language enum to string for FormatDetector
fn language_name_from_enum(lang: Language) -> &'static str {
    match lang {
        Language::Typescript => "typescript",
        Language::Javascript => "javascript",
        Language::Python => "python",
        Language::Rust => "rust",
        Language::Go => "go",
        Language::Java => "java",
        Language::CSharp => "csharp",
        Language::Cpp => "cpp",
        Language::C => "c",
        Language::Ruby => "ruby",
        Language::Php => "php",
        Language::Swift => "swift",
        Language::Kotlin => "kotlin",
    }
}

/// Compute aggregate bridge statistics from cache (RFC-0006)
///
/// Aggregates bridging data from all files to produce summary statistics.
fn compute_bridge_stats(cache: &Cache, config: &BridgeConfig) -> BridgeStats {
    let mut stats = BridgeStats {
        enabled: config.enabled,
        precedence: config.precedence.to_string(),
        summary: BridgeSummary::default(),
        by_format: HashMap::new(),
    };

    if !config.enabled {
        return stats;
    }

    // Aggregate from file bridge metadata
    for file in cache.files.values() {
        if !file.bridge.enabled {
            continue;
        }

        stats.summary.explicit_count += file.bridge.explicit_count;
        stats.summary.converted_count += file.bridge.converted_count;
        stats.summary.merged_count += file.bridge.merged_count;

        // Track by detected format
        if let Some(format) = &file.bridge.detected_format {
            let format_key = format_to_string(format);
            let format_count = file.bridge.converted_count + file.bridge.merged_count;
            if format_count > 0 {
                *stats.by_format.entry(format_key).or_insert(0) += format_count;
            }
        }
    }

    stats.summary.total_annotations =
        stats.summary.explicit_count + stats.summary.converted_count + stats.summary.merged_count;

    stats
}

/// Convert SourceFormat to string key for by_format map
fn format_to_string(format: &SourceFormat) -> String {
    match format {
        SourceFormat::Acp => "acp".to_string(),
        SourceFormat::Jsdoc => "jsdoc".to_string(),
        SourceFormat::DocstringGoogle => "docstring:google".to_string(),
        SourceFormat::DocstringNumpy => "docstring:numpy".to_string(),
        SourceFormat::DocstringSphinx => "docstring:sphinx".to_string(),
        SourceFormat::Rustdoc => "rustdoc".to_string(),
        SourceFormat::Javadoc => "javadoc".to_string(),
        SourceFormat::Godoc => "godoc".to_string(),
        SourceFormat::TypeHint => "type_hint".to_string(),
    }
}

/// Parse native documentation from a doc comment based on detected format
fn parse_native_docs(doc_comment: &str, format: &SourceFormat) -> Option<ParsedDocumentation> {
    let parsed = match format {
        SourceFormat::Jsdoc => JsDocParser::new().parse(doc_comment),
        SourceFormat::DocstringGoogle | SourceFormat::DocstringNumpy | SourceFormat::DocstringSphinx => {
            DocstringParser::new().parse(doc_comment)
        }
        SourceFormat::Rustdoc => RustdocParser::new().parse(doc_comment),
        SourceFormat::Javadoc => JavadocParser::new().parse(doc_comment),
        SourceFormat::Godoc => GodocParser::new().parse(doc_comment),
        SourceFormat::Acp | SourceFormat::TypeHint => return None,
    };

    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

/// Extract ACP annotations from a doc comment and convert to AcpAnnotations
fn extract_acp_annotations(doc_comment: &str, parser: &Parser) -> AcpAnnotations {
    let annotations = parser.parse_annotations(doc_comment);

    let mut result = AcpAnnotations::default();

    for ann in annotations {
        match ann.name.as_str() {
            "summary" => {
                if let Some(ref value) = ann.value {
                    result.summary = Some(value.clone());
                }
            }
            "fn" | "method" => {
                // @acp:fn "summary" - directive
                // The parser already extracts value and directive separately
                if let Some(ref value) = ann.value {
                    // Value might be the summary in quotes
                    if let Some((summary, _)) = parse_fn_annotation(value) {
                        if result.summary.is_none() {
                            result.summary = Some(summary);
                        }
                    }
                }
                // Directive is already parsed by the Parser
                if let Some(ref directive) = ann.directive {
                    result.directive = Some(directive.clone());
                }
            }
            "param" => {
                // @acp:param {type} name - directive
                // Extract name from value, directive is already parsed
                if let Some(ref value) = ann.value {
                    if let Some((name, _)) = parse_param_annotation(value) {
                        let directive = ann.directive.clone().unwrap_or_default();
                        result.params.push((name, directive));
                    }
                }
            }
            "returns" => {
                // @acp:returns {type} - directive
                // Directive is already parsed by the Parser
                if let Some(ref directive) = ann.directive {
                    result.returns = Some(directive.clone());
                } else if let Some(ref value) = ann.value {
                    // Fallback: try to extract directive from value
                    if let Some(directive) = parse_returns_annotation(value) {
                        result.returns = Some(directive);
                    }
                }
            }
            "throws" => {
                // @acp:throws {exception} - directive
                if let Some(ref value) = ann.value {
                    // Extract exception type from value
                    let exception = if value.starts_with('{') {
                        if let Some(close) = value.find('}') {
                            value[1..close].to_string()
                        } else {
                            value.clone()
                        }
                    } else {
                        value.split_whitespace().next().unwrap_or(value).to_string()
                    };
                    let directive = ann.directive.clone().unwrap_or_default();
                    result.throws.push((exception, directive));
                }
            }
            _ => {}
        }
    }

    result
}

/// Parse @acp:fn value into (summary, directive)
fn parse_fn_annotation(value: &str) -> Option<(String, String)> {
    // Format: "summary text" - directive text
    // or just: directive text
    if let Some(stripped) = value.strip_prefix('"') {
        if let Some(end_quote) = stripped.find('"') {
            let summary = stripped[..end_quote].to_string();
            let rest = &stripped[end_quote + 1..];
            let directive = rest.trim().trim_start_matches('-').trim().to_string();
            if !directive.is_empty() {
                return Some((summary, directive));
            }
        }
    }
    None
}

/// Parse @acp:param value into (name, directive)
fn parse_param_annotation(value: &str) -> Option<(String, String)> {
    // Format: {type} name - directive  OR  name - directive
    let value = value.trim();

    // Skip type annotation if present
    let rest = if value.starts_with('{') {
        if let Some(close) = value.find('}') {
            &value[close + 1..]
        } else {
            value
        }
    } else {
        value
    };

    let rest = rest.trim();

    // Handle optional params: [name] or [name=default]
    let (name, after_name) = if rest.starts_with('[') {
        if let Some(close) = rest.find(']') {
            let inner = &rest[1..close];
            let name = inner.split('=').next().unwrap_or(inner).trim();
            (name.to_string(), &rest[close + 1..])
        } else {
            return None;
        }
    } else {
        // Regular param: name - directive
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.is_empty() {
            return None;
        }
        let after = if parts.len() > 1 { parts[1] } else { "" };
        (parts[0].to_string(), after)
    };

    // Extract directive after the dash
    let directive = after_name.trim().trim_start_matches('-').trim().to_string();

    if name.is_empty() {
        None
    } else {
        Some((name, directive))
    }
}

/// Parse @acp:returns value into directive
fn parse_returns_annotation(value: &str) -> Option<String> {
    // Format: {type} - directive  OR  - directive  OR  directive
    let value = value.trim();

    // Skip type annotation if present
    let rest = if value.starts_with('{') {
        if let Some(close) = value.find('}') {
            &value[close + 1..]
        } else {
            value
        }
    } else {
        value
    };

    let directive = rest.trim().trim_start_matches('-').trim().to_string();

    if directive.is_empty() {
        None
    } else {
        Some(directive)
    }
}

