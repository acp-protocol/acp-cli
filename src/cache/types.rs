//! @acp:module "Cache Types"
//! @acp:summary "Data structures matching the .acp.cache.json schema"
//! @acp:domain cli
//! @acp:layer model
//!
//! These types serialize directly to/from `.acp.cache.json`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::constraints::ConstraintIndex;
use crate::error::Result;
use crate::git::{GitFileInfo, GitSymbolInfo};

/// @acp:summary "Normalize a file path for cross-platform compatibility"
///
/// Handles:
/// - Windows backslashes → forward slashes
/// - Redundant slashes (`//` → `/`)
/// - Relative components (`.` and `..`)
/// - Leading `./` prefix normalization
///
/// # Examples
/// ```
/// use acp::cache::normalize_path;
///
/// assert_eq!(normalize_path("src/file.ts"), "src/file.ts");
/// assert_eq!(normalize_path("./src/file.ts"), "src/file.ts");
/// assert_eq!(normalize_path("src\\file.ts"), "src/file.ts");
/// assert_eq!(normalize_path("src/../src/file.ts"), "src/file.ts");
/// ```
pub fn normalize_path(path: &str) -> String {
    // Convert backslashes to forward slashes (Windows compatibility)
    let path = path.replace('\\', "/");

    // Split into components and resolve . and ..
    let mut components: Vec<&str> = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => continue,  // Skip empty and current directory
            ".." => {
                // Go up one directory if possible
                components.pop();
            }
            component => components.push(component),
        }
    }

    components.join("/")
}

/// @acp:summary "Complete ACP cache file structure (schema-compliant)"
/// @acp:lock normal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    /// JSON Schema URL for validation
    #[serde(rename = "$schema", default = "default_cache_schema")]
    pub schema: String,
    /// Schema version (required)
    pub version: String,
    /// Generation timestamp (required)
    pub generated_at: DateTime<Utc>,
    /// Git commit SHA (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Project metadata (required)
    pub project: ProjectInfo,
    /// Aggregate statistics (required)
    pub stats: Stats,
    /// Map of file paths to modification times for staleness detection (required)
    pub source_files: HashMap<String, DateTime<Utc>>,
    /// Files indexed by path (required)
    pub files: HashMap<String, FileEntry>,
    /// Symbols indexed by name (required)
    pub symbols: HashMap<String, SymbolEntry>,
    /// Call graph relationships (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<CallGraph>,
    /// Domain groupings (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub domains: HashMap<String, DomainEntry>,
    /// AI behavioral constraints (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<ConstraintIndex>,
}

fn default_cache_schema() -> String {
    "https://acp-protocol.dev/schemas/v1/cache.schema.json".to_string()
}

impl Cache {
    /// @acp:summary "Create a new empty cache"
    pub fn new(project_name: &str, root: &str) -> Self {
        Self {
            schema: default_cache_schema(),
            version: crate::VERSION.to_string(),
            generated_at: Utc::now(),
            git_commit: None,
            project: ProjectInfo {
                name: project_name.to_string(),
                root: root.to_string(),
                description: None,
            },
            stats: Stats::default(),
            source_files: HashMap::new(),
            files: HashMap::new(),
            symbols: HashMap::new(),
            graph: Some(CallGraph::default()),
            domains: HashMap::new(),
            constraints: None,
        }
    }

    /// @acp:summary "Load cache from JSON file"
    pub fn from_json<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let cache = serde_json::from_reader(reader)?;
        Ok(cache)
    }

    /// @acp:summary "Write cache to JSON file"
    pub fn write_json<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    /// @acp:summary "Get a symbol by name - O(1) lookup"
    pub fn get_symbol(&self, name: &str) -> Option<&SymbolEntry> {
        self.symbols.get(name)
    }

    /// @acp:summary "Get a file by path - O(1) lookup with cross-platform path normalization"
    ///
    /// Handles various path formats:
    /// - With or without `./` prefix: `src/file.ts` and `./src/file.ts`
    /// - Windows backslashes: `src\file.ts`
    /// - Redundant separators: `src//file.ts`
    /// - Relative components: `src/../src/file.ts`
    pub fn get_file(&self, path: &str) -> Option<&FileEntry> {
        // Try exact match first (fastest path)
        if let Some(file) = self.files.get(path) {
            return Some(file);
        }

        // Normalize and try variations
        let normalized = normalize_path(path);

        // Try normalized path directly
        if let Some(file) = self.files.get(&normalized) {
            return Some(file);
        }

        // Try with ./ prefix
        let with_prefix = format!("./{}", &normalized);
        if let Some(file) = self.files.get(&with_prefix) {
            return Some(file);
        }

        // Try stripping ./ prefix from normalized
        if let Some(stripped) = normalized.strip_prefix("./") {
            if let Some(file) = self.files.get(stripped) {
                return Some(file);
            }
        }

        None
    }

    /// @acp:summary "Get callers of a symbol from reverse call graph"
    pub fn get_callers(&self, symbol: &str) -> Option<&Vec<String>> {
        self.graph.as_ref().and_then(|g| g.reverse.get(symbol))
    }

    /// @acp:summary "Get callees of a symbol from forward call graph"
    pub fn get_callees(&self, symbol: &str) -> Option<&Vec<String>> {
        self.graph.as_ref().and_then(|g| g.forward.get(symbol))
    }

    /// @acp:summary "Get all files in a domain"
    pub fn get_domain_files(&self, domain: &str) -> Option<&Vec<String>> {
        self.domains.get(domain).map(|d| &d.files)
    }

    /// @acp:summary "Recalculate statistics after indexing"
    pub fn update_stats(&mut self) {
        self.stats.files = self.files.len();
        self.stats.symbols = self.symbols.len();
        self.stats.lines = self.files.values().map(|f| f.lines).sum();

        let annotated = self.symbols.values()
            .filter(|s| s.summary.is_some())
            .count();

        if self.stats.symbols > 0 {
            self.stats.annotation_coverage =
                (annotated as f64 / self.stats.symbols as f64) * 100.0;
        }
    }
}

/// @acp:summary "Builder for incremental cache construction"
pub struct CacheBuilder {
    cache: Cache,
}

impl CacheBuilder {
    pub fn new(project_name: &str, root: &str) -> Self {
        Self {
            cache: Cache::new(project_name, root),
        }
    }

    pub fn add_file(mut self, file: FileEntry) -> Self {
        let path = file.path.clone();
        self.cache.files.insert(path, file);
        self
    }

    pub fn add_symbol(mut self, symbol: SymbolEntry) -> Self {
        let name = symbol.name.clone();
        self.cache.symbols.insert(name, symbol);
        self
    }

    pub fn add_call_edge(mut self, from: &str, to: Vec<String>) -> Self {
        let graph = self.cache.graph.get_or_insert_with(CallGraph::default);
        graph.forward.insert(from.to_string(), to.clone());

        // Build reverse graph
        for callee in to {
            graph.reverse
                .entry(callee)
                .or_default()
                .push(from.to_string());
        }
        self
    }

    pub fn add_source_file(mut self, path: String, modified_at: DateTime<Utc>) -> Self {
        self.cache.source_files.insert(path, modified_at);
        self
    }

    pub fn add_domain(mut self, domain: DomainEntry) -> Self {
        let name = domain.name.clone();
        self.cache.domains.insert(name, domain);
        self
    }

    pub fn set_constraints(mut self, constraints: ConstraintIndex) -> Self {
        self.cache.constraints = Some(constraints);
        self
    }

    pub fn set_git_commit(mut self, commit: String) -> Self {
        self.cache.git_commit = Some(commit);
        self
    }

    pub fn build(mut self) -> Cache {
        self.cache.update_stats();
        self.cache
    }
}

/// @acp:summary "Project metadata"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// @acp:summary "Aggregate statistics"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub files: usize,
    pub symbols: usize,
    pub lines: usize,
    #[serde(default)]
    pub annotation_coverage: f64,
}

/// @acp:summary "File entry with metadata (RFC-001 compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from project root (required)
    pub path: String,
    /// Line count (required)
    pub lines: usize,
    /// Programming language identifier (required)
    pub language: Language,
    /// Exported symbols (required)
    #[serde(default)]
    pub exports: Vec<String>,
    /// Imported modules (required)
    #[serde(default)]
    pub imports: Vec<String>,
    /// Human-readable module name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Brief file description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// RFC-001: File purpose from @acp:purpose annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// RFC-001: File owner from @acp:owner annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// RFC-001: Inline annotations (hack, todo, fixme, critical, perf)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inline: Vec<InlineAnnotation>,
    /// Domain classifications (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
    /// Architectural layer (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Stability level (optional, null if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stability: Option<Stability>,
    /// AI behavioral hints (e.g., "ai-careful", "ai-readonly")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_hints: Vec<String>,
    /// Git metadata (optional - last commit, author, contributors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitFileInfo>,
}

/// @acp:summary "RFC-001: Inline annotation (hack, todo, fixme, critical, perf)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineAnnotation {
    /// Line number (1-indexed)
    pub line: usize,
    /// Annotation type (hack, todo, fixme, critical, perf)
    #[serde(rename = "type")]
    pub annotation_type: String,
    /// Annotation value/description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// RFC-001: Self-documenting directive
    pub directive: String,
    /// Expiry date for hacks (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Related ticket/issue (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// Whether directive was auto-generated
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_generated: bool,
}

/// @acp:summary "Symbol entry with metadata (RFC-001 compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    /// Simple symbol name (required)
    pub name: String,
    /// Qualified name: file_path:class.symbol (required)
    pub qualified_name: String,
    /// Symbol type (required)
    #[serde(rename = "type")]
    pub symbol_type: SymbolType,
    /// Containing file path (required)
    pub file: String,
    /// [start_line, end_line] (required)
    pub lines: [usize; 2],
    /// Whether exported (required)
    pub exported: bool,
    /// Function signature (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Brief description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// RFC-001: Symbol purpose from @acp:fn/@acp:class/@acp:method directive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// RFC-001: Symbol-level constraints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<SymbolConstraint>,
    /// Whether async (optional, default false)
    #[serde(rename = "async", default, skip_serializing_if = "is_false")]
    pub async_fn: bool,
    /// Symbol visibility (optional, default public)
    #[serde(default, skip_serializing_if = "is_default_visibility")]
    pub visibility: Visibility,
    /// Symbols this calls (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<String>,
    /// Symbols calling this (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub called_by: Vec<String>,
    /// Git metadata (optional - last commit, author, code age)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitSymbolInfo>,
}

/// @acp:summary "RFC-001: Symbol-level constraint"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolConstraint {
    /// Lock level for this symbol
    pub level: String,
    /// Self-documenting directive
    pub directive: String,
    /// Whether directive was auto-generated
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_generated: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_default_visibility(v: &Visibility) -> bool {
    *v == Visibility::Public
}

/// @acp:summary "Symbol type enumeration (schema-compliant)"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolType {
    #[default]
    Function,
    Method,
    Class,
    Interface,
    Type,
    Enum,
    Struct,
    Trait,
    Const,
}

/// @acp:summary "Symbol visibility"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Protected,
}

/// @acp:summary "Stability classification (schema-compliant)"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stability {
    Stable,
    Experimental,
    Deprecated,
}

/// @acp:summary "Programming language identifier (schema-compliant)"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Typescript,
    Javascript,
    Python,
    Rust,
    Go,
    Java,
    #[serde(rename = "c-sharp")]
    CSharp,
    Cpp,
    C,
    Ruby,
    Php,
    Swift,
    Kotlin,
}

/// @acp:summary "Bidirectional call graph"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallGraph {
    /// Forward: caller -> [callees]
    #[serde(default)]
    pub forward: HashMap<String, Vec<String>>,
    /// Reverse: callee -> [callers]
    #[serde(default)]
    pub reverse: HashMap<String, Vec<String>>,
}

/// @acp:summary "Domain grouping (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEntry {
    /// Domain identifier (required)
    pub name: String,
    /// Files in this domain (required)
    pub files: Vec<String>,
    /// Symbols in this domain (required)
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Human description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_roundtrip() {
        let cache = CacheBuilder::new("test", "/test")
            .add_symbol(SymbolEntry {
                name: "test_fn".to_string(),
                qualified_name: "test.rs:test_fn".to_string(),
                symbol_type: SymbolType::Function,
                file: "test.rs".to_string(),
                lines: [1, 10],
                exported: true,
                signature: None,
                summary: Some("Test function".to_string()),
                purpose: None,
                constraints: None,
                async_fn: false,
                visibility: Visibility::Public,
                calls: vec![],
                called_by: vec![],
                git: None,
            })
            .build();

        let json = serde_json::to_string_pretty(&cache).unwrap();
        let parsed: Cache = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.project.name, "test");
        assert!(parsed.symbols.contains_key("test_fn"));
    }

    // ========================================================================
    // Path Normalization Tests
    // ========================================================================

    #[test]
    fn test_normalize_path_basic() {
        // Simple paths should pass through
        assert_eq!(normalize_path("src/file.ts"), "src/file.ts");
        assert_eq!(normalize_path("file.ts"), "file.ts");
        assert_eq!(normalize_path("a/b/c/file.ts"), "a/b/c/file.ts");
    }

    #[test]
    fn test_normalize_path_dot_prefix() {
        // Leading ./ should be stripped
        assert_eq!(normalize_path("./src/file.ts"), "src/file.ts");
        assert_eq!(normalize_path("./file.ts"), "file.ts");
        assert_eq!(normalize_path("././src/file.ts"), "src/file.ts");
    }

    #[test]
    fn test_normalize_path_windows_backslash() {
        // Windows backslashes should be converted
        assert_eq!(normalize_path("src\\file.ts"), "src/file.ts");
        assert_eq!(normalize_path(".\\src\\file.ts"), "src/file.ts");
        assert_eq!(normalize_path("a\\b\\c\\file.ts"), "a/b/c/file.ts");
    }

    #[test]
    fn test_normalize_path_double_slashes() {
        // Double slashes should be normalized
        assert_eq!(normalize_path("src//file.ts"), "src/file.ts");
        assert_eq!(normalize_path("a//b//c//file.ts"), "a/b/c/file.ts");
        assert_eq!(normalize_path(".//src/file.ts"), "src/file.ts");
    }

    #[test]
    fn test_normalize_path_parent_refs() {
        // Parent directory references should be resolved
        assert_eq!(normalize_path("src/../src/file.ts"), "src/file.ts");
        assert_eq!(normalize_path("a/b/../c/file.ts"), "a/c/file.ts");
        assert_eq!(normalize_path("a/b/c/../../d/file.ts"), "a/d/file.ts");
        assert_eq!(normalize_path("./src/../src/file.ts"), "src/file.ts");
    }

    #[test]
    fn test_normalize_path_current_dir() {
        // Current directory references should be removed
        assert_eq!(normalize_path("src/./file.ts"), "src/file.ts");
        assert_eq!(normalize_path("./src/./file.ts"), "src/file.ts");
        assert_eq!(normalize_path("a/./b/./c/file.ts"), "a/b/c/file.ts");
    }

    #[test]
    fn test_normalize_path_mixed() {
        // Mixed cases
        assert_eq!(normalize_path(".\\src/../src\\file.ts"), "src/file.ts");
        assert_eq!(normalize_path("./a\\b//../c//file.ts"), "a/c/file.ts");
    }

    // ========================================================================
    // get_file Tests with Various Path Formats
    // ========================================================================

    fn create_test_cache_with_file() -> Cache {
        let mut cache = Cache::new("test", ".");
        cache.files.insert("./src/sample.ts".to_string(), FileEntry {
            path: "./src/sample.ts".to_string(),
            lines: 100,
            language: Language::Typescript,
            exports: vec![],
            imports: vec![],
            module: None,
            summary: None,
            purpose: None,
            owner: None,
            inline: vec![],
            domains: vec![],
            layer: None,
            stability: None,
            ai_hints: vec![],
            git: None,
        });
        cache
    }

    #[test]
    fn test_get_file_exact_match() {
        let cache = create_test_cache_with_file();
        // Exact match should work
        assert!(cache.get_file("./src/sample.ts").is_some());
    }

    #[test]
    fn test_get_file_without_prefix() {
        let cache = create_test_cache_with_file();
        // Without ./ prefix should work
        assert!(cache.get_file("src/sample.ts").is_some());
    }

    #[test]
    fn test_get_file_windows_path() {
        let cache = create_test_cache_with_file();
        // Windows-style path should work
        assert!(cache.get_file("src\\sample.ts").is_some());
        assert!(cache.get_file(".\\src\\sample.ts").is_some());
    }

    #[test]
    fn test_get_file_with_parent_ref() {
        let cache = create_test_cache_with_file();
        // Path with .. should work
        assert!(cache.get_file("src/../src/sample.ts").is_some());
        assert!(cache.get_file("./src/../src/sample.ts").is_some());
    }

    #[test]
    fn test_get_file_double_slash() {
        let cache = create_test_cache_with_file();
        // Double slashes should work
        assert!(cache.get_file("src//sample.ts").is_some());
    }

    #[test]
    fn test_get_file_not_found() {
        let cache = create_test_cache_with_file();
        // Non-existent files should return None
        assert!(cache.get_file("src/other.ts").is_none());
        assert!(cache.get_file("other/sample.ts").is_none());
    }

    #[test]
    fn test_get_file_stored_without_prefix() {
        // Test when cache stores paths without ./ prefix
        let mut cache = Cache::new("test", ".");
        cache.files.insert("src/sample.ts".to_string(), FileEntry {
            path: "src/sample.ts".to_string(),
            lines: 100,
            language: Language::Typescript,
            exports: vec![],
            imports: vec![],
            module: None,
            summary: None,
            purpose: None,
            owner: None,
            inline: vec![],
            domains: vec![],
            layer: None,
            stability: None,
            ai_hints: vec![],
            git: None,
        });

        // All formats should find it
        assert!(cache.get_file("src/sample.ts").is_some());
        assert!(cache.get_file("./src/sample.ts").is_some());
        assert!(cache.get_file("src\\sample.ts").is_some());
    }
}