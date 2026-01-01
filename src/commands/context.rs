//! @acp:module "Context Command"
//! @acp:summary "RFC-0015: Operation-specific context for AI agents"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Provides context tailored for specific operations:
//! - create: Naming conventions, import style, directory patterns
//! - modify: Constraints, importers, affected files
//! - debug: Error context, related symbols
//! - explore: Directory structure, domain overview

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::cache::{Cache, FileEntry, FileNamingConvention};
use crate::query::Query;

/// Options for the context command
#[derive(Debug, Clone)]
pub struct ContextOptions {
    /// Cache file path
    pub cache: PathBuf,
    /// Output as JSON
    pub json: bool,
    /// Verbose output with additional details
    pub verbose: bool,
}

impl Default for ContextOptions {
    fn default() -> Self {
        Self {
            cache: PathBuf::from(".acp/acp.cache.json"),
            json: false,
            verbose: false,
        }
    }
}

/// Context subcommand for different operations
#[derive(Debug, Clone)]
pub enum ContextOperation {
    /// Context for creating a new file
    Create {
        /// Directory where the file will be created
        directory: String,
    },
    /// Context for modifying an existing file
    Modify {
        /// File path to modify
        file: String,
    },
    /// Context for debugging an issue
    Debug {
        /// File or symbol with the issue
        target: String,
    },
    /// Context for exploring the codebase
    Explore {
        /// Optional domain to focus on
        domain: Option<String>,
    },
}

/// Context output for create operation
#[derive(Debug, Clone, Serialize)]
pub struct CreateContext {
    /// Target directory
    pub directory: String,
    /// Detected language for this directory
    pub language: Option<String>,
    /// File naming conventions in this directory
    pub naming: Option<FileNamingConvention>,
    /// Import style preferences
    pub import_style: Option<ImportStyle>,
    /// Similar files in the directory
    pub similar_files: Vec<String>,
    /// Recommended file pattern
    pub recommended_pattern: Option<String>,
}

/// Context output for modify operation
#[derive(Debug, Clone, Serialize)]
pub struct ModifyContext {
    /// Target file path
    pub file: String,
    /// Files that import this file (will be affected by changes)
    pub importers: Vec<String>,
    /// Number of importers
    pub importer_count: usize,
    /// Constraints on this file
    pub constraints: Option<FileConstraint>,
    /// Symbols in this file
    pub symbols: Vec<String>,
    /// Domain this file belongs to
    pub domain: Option<String>,
}

/// Context output for debug operation
#[derive(Debug, Clone, Serialize)]
pub struct DebugContext {
    /// Target file or symbol
    pub target: String,
    /// Related files (dependencies)
    pub related_files: Vec<String>,
    /// Symbols in the target
    pub symbols: Vec<SymbolInfo>,
    /// Potential hotpaths through this code
    pub hotpaths: Vec<String>,
}

/// Context output for explore operation
#[derive(Debug, Clone, Serialize)]
pub struct ExploreContext {
    /// Domain being explored (if specified)
    pub domain: Option<String>,
    /// Project statistics
    pub stats: ProjectStats,
    /// Domain list
    pub domains: Vec<DomainInfo>,
    /// Recent/important files
    pub key_files: Vec<String>,
}

/// Import style preferences
#[derive(Debug, Clone, Serialize)]
pub struct ImportStyle {
    /// Module system (esm, commonjs)
    pub module_system: String,
    /// Path style (relative, absolute, alias)
    pub path_style: String,
    /// Whether index exports are used
    pub index_exports: bool,
}

/// File-level constraints
#[derive(Debug, Clone, Serialize)]
pub struct FileConstraint {
    /// Mutation level
    pub level: String,
    /// Constraint reason
    pub reason: Option<String>,
}

/// Symbol information
#[derive(Debug, Clone, Serialize)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Symbol type
    pub symbol_type: String,
    /// Purpose if available
    pub purpose: Option<String>,
}

/// Domain information
#[derive(Debug, Clone, Serialize)]
pub struct DomainInfo {
    /// Domain name
    pub name: String,
    /// Number of files
    pub file_count: usize,
    /// Number of symbols
    pub symbol_count: usize,
}

/// Project statistics summary
#[derive(Debug, Clone, Serialize)]
pub struct ProjectStats {
    /// Total files
    pub files: usize,
    /// Total symbols
    pub symbols: usize,
    /// Total lines
    pub lines: usize,
    /// Primary language
    pub primary_language: Option<String>,
    /// Annotation coverage
    pub coverage: f64,
}

/// Execute the context command
pub fn execute_context(options: ContextOptions, operation: ContextOperation) -> Result<()> {
    let cache = Cache::from_json(&options.cache)?;

    match operation {
        ContextOperation::Create { directory } => {
            execute_create_context(&cache, &directory, &options)
        }
        ContextOperation::Modify { file } => execute_modify_context(&cache, &file, &options),
        ContextOperation::Debug { target } => execute_debug_context(&cache, &target, &options),
        ContextOperation::Explore { domain } => {
            execute_explore_context(&cache, domain.as_deref(), &options)
        }
    }
}

/// Execute context for create operation (T2.7)
fn execute_create_context(cache: &Cache, directory: &str, options: &ContextOptions) -> Result<()> {
    // Find naming conventions for this directory (direct access - not an Option)
    let naming = cache
        .conventions
        .file_naming
        .iter()
        .find(|n| n.directory == directory)
        .cloned()
        .or_else(|| {
            // Try to find a parent directory convention
            cache
                .conventions
                .file_naming
                .iter()
                .filter(|n| directory.starts_with(&n.directory))
                .max_by_key(|n| n.directory.len())
                .cloned()
        });

    // Detect primary language in directory
    let language = detect_directory_language(cache, directory);

    // Get import style from conventions
    let import_style = cache.conventions.imports.as_ref().map(|i| ImportStyle {
        module_system: i
            .module_system
            .as_ref()
            .map(|m| format!("{:?}", m).to_lowercase())
            .unwrap_or_else(|| "esm".to_string()),
        path_style: i
            .path_style
            .as_ref()
            .map(|p| format!("{:?}", p).to_lowercase())
            .unwrap_or_else(|| "relative".to_string()),
        index_exports: i.index_exports,
    });

    // Find similar files in the directory
    let similar_files: Vec<String> = cache
        .files
        .keys()
        .filter(|p| {
            let parent = std::path::Path::new(p)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            parent == directory
        })
        .take(5)
        .cloned()
        .collect();

    let recommended_pattern = naming.as_ref().map(|n| n.pattern.clone());

    let context = CreateContext {
        directory: directory.to_string(),
        language,
        naming,
        import_style,
        similar_files,
        recommended_pattern,
    };

    output_context(&context, options)
}

/// Execute context for modify operation (T2.8)
fn execute_modify_context(cache: &Cache, file: &str, options: &ContextOptions) -> Result<()> {
    let file_entry = cache.files.get(file);

    // Get importers from the file entry
    let importers = file_entry
        .map(|f| f.imported_by.clone())
        .unwrap_or_default();
    let importer_count = importers.len();

    // Get file constraints
    let constraints = cache.constraints.as_ref().and_then(|c| {
        c.by_file.get(file).and_then(|fc| {
            fc.mutation.as_ref().map(|m| FileConstraint {
                level: format!("{:?}", m.level).to_lowercase(),
                reason: m.reason.clone(),
            })
        })
    });

    // Get symbols in this file
    let symbols = file_entry.map(|f| f.exports.clone()).unwrap_or_default();

    // Get domain (domains is a HashMap<String, DomainEntry>)
    let domain = cache
        .domains
        .iter()
        .find(|(_, d)| d.files.contains(&file.to_string()))
        .map(|(name, _)| name.clone());

    let context = ModifyContext {
        file: file.to_string(),
        importers,
        importer_count,
        constraints,
        symbols,
        domain,
    };

    output_context(&context, options)
}

/// Execute context for debug operation
fn execute_debug_context(cache: &Cache, target: &str, options: &ContextOptions) -> Result<()> {
    // Target could be a file or symbol
    let (file_path, symbols_info) = if cache.files.contains_key(target) {
        // It's a file
        let file = cache.files.get(target).unwrap();
        let symbols: Vec<SymbolInfo> = file
            .exports
            .iter()
            .filter_map(|name| cache.symbols.get(name))
            .map(|s| SymbolInfo {
                name: s.name.clone(),
                symbol_type: format!("{:?}", s.symbol_type).to_lowercase(),
                purpose: s.purpose.clone(),
            })
            .collect();
        (target.to_string(), symbols)
    } else if let Some(symbol) = cache.symbols.get(target) {
        // It's a symbol
        (
            symbol.file.clone(),
            vec![SymbolInfo {
                name: symbol.name.clone(),
                symbol_type: format!("{:?}", symbol.symbol_type).to_lowercase(),
                purpose: symbol.purpose.clone(),
            }],
        )
    } else {
        // Not found
        return Err(anyhow::anyhow!(
            "Target not found: {}. Provide a file path or symbol name.",
            target
        ));
    };

    // Get related files (imports)
    let related_files = cache
        .files
        .get(&file_path)
        .map(|f| f.imports.clone())
        .unwrap_or_default();

    // Get hotpaths using Query API
    let q = Query::new(cache);
    let hotpaths: Vec<String> = q
        .hotpaths()
        .filter(|hp| hp.contains(&file_path) || hp.contains(target))
        .take(3)
        .map(String::from)
        .collect();

    let context = DebugContext {
        target: target.to_string(),
        related_files,
        symbols: symbols_info,
        hotpaths,
    };

    output_context(&context, options)
}

/// Execute context for explore operation
fn execute_explore_context(
    cache: &Cache,
    domain_filter: Option<&str>,
    options: &ContextOptions,
) -> Result<()> {
    let stats = ProjectStats {
        files: cache.stats.files,
        symbols: cache.stats.symbols,
        lines: cache.stats.lines,
        primary_language: cache.stats.primary_language.clone(),
        coverage: cache.stats.annotation_coverage,
    };

    // domains is HashMap<String, DomainEntry>
    let domains: Vec<DomainInfo> = cache
        .domains
        .iter()
        .filter(|(name, _)| domain_filter.map_or(true, |f| name.contains(f)))
        .map(|(name, d)| DomainInfo {
            name: name.clone(),
            file_count: d.files.len(),
            symbol_count: d.symbols.len(),
        })
        .collect();

    // Get key files (entry points, high-importer files)
    let mut key_files: Vec<(&String, &FileEntry)> = cache.files.iter().collect();
    key_files.sort_by(|a, b| b.1.imported_by.len().cmp(&a.1.imported_by.len()));
    let key_files: Vec<String> = key_files
        .iter()
        .take(10)
        .map(|(k, _)| (*k).clone())
        .collect();

    let context = ExploreContext {
        domain: domain_filter.map(String::from),
        stats,
        domains,
        key_files,
    };

    output_context(&context, options)
}

/// Output context in the appropriate format
fn output_context<T: Serialize + std::fmt::Debug>(
    context: &T,
    _options: &ContextOptions,
) -> Result<()> {
    // Always output JSON for now - machine-readable format
    println!("{}", serde_json::to_string_pretty(context)?);
    Ok(())
}

/// Detect the primary language in a directory
fn detect_directory_language(cache: &Cache, directory: &str) -> Option<String> {
    use std::collections::HashMap;

    let mut lang_counts: HashMap<String, usize> = HashMap::new();

    for (path, file) in &cache.files {
        let parent = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if parent == directory || parent.starts_with(&format!("{}/", directory)) {
            let lang = format!("{:?}", file.language).to_lowercase();
            *lang_counts.entry(lang).or_insert(0) += 1;
        }
    }

    lang_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_style_default() {
        let style = ImportStyle {
            module_system: "esm".to_string(),
            path_style: "relative".to_string(),
            index_exports: false,
        };
        assert_eq!(style.module_system, "esm");
    }

    #[test]
    fn test_context_options_default() {
        let opts = ContextOptions::default();
        assert_eq!(opts.cache, PathBuf::from(".acp/acp.cache.json"));
        assert!(!opts.json);
    }
}
