//! @acp:module "Cache Types"
//! @acp:summary "Data structures matching the .acp.cache.json schema (RFC-001/RFC-003 compliant)"
//! @acp:domain cli
//! @acp:layer model
//!
//! These types serialize directly to/from `.acp.cache.json`
//! Includes RFC-003 annotation provenance tracking support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::constraints::ConstraintIndex;
use crate::error::Result;
use crate::git::{GitFileInfo, GitSymbolInfo};
use crate::parse::SourceOrigin;

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
            "" | "." => continue, // Skip empty and current directory
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
    /// RFC-0003: Annotation provenance statistics (optional)
    #[serde(default, skip_serializing_if = "ProvenanceStats::is_empty")]
    pub provenance: ProvenanceStats,
    /// RFC-0006: Bridge statistics (optional)
    #[serde(default, skip_serializing_if = "BridgeStats::is_empty")]
    pub bridge: BridgeStats,
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
            provenance: ProvenanceStats::default(),
            bridge: BridgeStats::default(),
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

        let annotated = self
            .symbols
            .values()
            .filter(|s| s.summary.is_some())
            .count();

        if self.stats.symbols > 0 {
            self.stats.annotation_coverage = (annotated as f64 / self.stats.symbols as f64) * 100.0;
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
            graph
                .reverse
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
    /// RFC-0003: Annotation provenance tracking
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, AnnotationProvenance>,
    /// RFC-0006: Bridge metadata for this file
    #[serde(default, skip_serializing_if = "BridgeMetadata::is_empty")]
    pub bridge: BridgeMetadata,
    /// RFC-0009: File version (from @acp:version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// RFC-0009: Version when introduced (from @acp:since)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// RFC-0009: File license (from @acp:license)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// RFC-0009: File author (from @acp:author)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// RFC-0009: Lifecycle status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleAnnotations>,
    /// RFC-0002: Documentation references
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<RefEntry>,
    /// RFC-0002: Style guide configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleEntry>,
}

/// @acp:summary "RFC-0002: Documentation reference entry"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefEntry {
    /// Documentation URL
    pub url: String,
    /// Approved source ID from config (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Documentation version (from @acp:ref-version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Section within documentation (from @acp:ref-section)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Whether AI should fetch this reference (from @acp:ref-fetch)
    #[serde(default)]
    pub fetch: bool,
}

/// @acp:summary "RFC-0002: Style guide configuration entry"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleEntry {
    /// Style guide name or ID
    pub name: String,
    /// Parent style guide (from @acp:style-extends)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Documentation source ID for this style
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Direct URL to style guide documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Style rules (from @acp:style-rules)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
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
    /// RFC-0003: Annotation provenance tracking
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, AnnotationProvenance>,
    /// RFC-0009: Behavioral characteristics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavioral: Option<BehavioralAnnotations>,
    /// RFC-0009: Lifecycle status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleAnnotations>,
    /// RFC-0009: Documentation metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<DocumentationAnnotations>,
    /// RFC-0009: Performance characteristics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<PerformanceAnnotations>,
    /// RFC-0008: Type annotation information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_info: Option<TypeInfo>,
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

// ============================================================================
// RFC-0006: Documentation System Bridging Types
// ============================================================================

/// @acp:summary "Source of type information (RFC-0006, RFC-0008)"
/// Indicates where type information was extracted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeSource {
    /// Type from ACP annotation {Type} (RFC-0008)
    Acp,
    /// Inline type annotation (TypeScript, Python type hint)
    TypeHint,
    /// JSDoc @param {Type} or @returns {Type}
    Jsdoc,
    /// Python docstring type specification
    Docstring,
    /// Rust doc comment type specification
    Rustdoc,
    /// Javadoc @param type specification
    Javadoc,
    /// Inferred from usage or default values
    Inferred,
    /// Type bridged from native docs - general category (RFC-0008)
    Native,
}

/// @acp:summary "Source of bridged documentation (RFC-0006)"
/// Indicates how documentation was obtained.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BridgeSource {
    /// Pure ACP annotation (human-written)
    #[default]
    Explicit,
    /// Converted from native documentation
    Converted,
    /// Combined from native + ACP
    Merged,
    /// Auto-generated through inference
    Heuristic,
}

fn is_explicit_bridge(source: &BridgeSource) -> bool {
    matches!(source, BridgeSource::Explicit)
}

/// @acp:summary "Original documentation format (RFC-0006)"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    /// Pure ACP annotation
    Acp,
    /// JSDoc/TSDoc
    Jsdoc,
    /// Google-style Python docstring
    #[serde(rename = "docstring:google")]
    DocstringGoogle,
    /// NumPy-style Python docstring
    #[serde(rename = "docstring:numpy")]
    DocstringNumpy,
    /// Sphinx/reST-style Python docstring
    #[serde(rename = "docstring:sphinx")]
    DocstringSphinx,
    /// Rust doc comments
    Rustdoc,
    /// Javadoc comments
    Javadoc,
    /// Go doc comments
    Godoc,
    /// Inline type annotation (TypeScript, Python type hints)
    TypeHint,
}

/// @acp:summary "Parameter entry with bridge provenance (RFC-0006)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamEntry {
    /// Parameter name
    pub name: String,
    /// Type annotation (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Source of type information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_source: Option<TypeSource>,
    /// Parameter description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// AI behavioral directive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Whether parameter is optional
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
    /// Default value (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Source of documentation
    #[serde(default, skip_serializing_if = "is_explicit_bridge")]
    pub source: BridgeSource,
    /// Single source format (when from one source)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_format: Option<SourceFormat>,
    /// Multiple source formats (when merged)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_formats: Vec<SourceFormat>,
}

/// @acp:summary "Returns entry with bridge provenance (RFC-0006)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnsEntry {
    /// Return type (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Source of type information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_source: Option<TypeSource>,
    /// Return value description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// AI behavioral directive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Source of documentation
    #[serde(default, skip_serializing_if = "is_explicit_bridge")]
    pub source: BridgeSource,
    /// Single source format (when from one source)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_format: Option<SourceFormat>,
    /// Multiple source formats (when merged)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_formats: Vec<SourceFormat>,
}

/// @acp:summary "Throws/Raises entry with bridge provenance (RFC-0006)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThrowsEntry {
    /// Exception/error type
    pub exception: String,
    /// Description of when/why thrown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// AI behavioral directive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Source of documentation
    #[serde(default, skip_serializing_if = "is_explicit_bridge")]
    pub source: BridgeSource,
    /// Original documentation format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_format: Option<SourceFormat>,
}

/// @acp:summary "Per-file bridge metadata (RFC-0006)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeMetadata {
    /// Whether bridging was enabled for this file
    #[serde(default)]
    pub enabled: bool,
    /// Detected documentation format (auto-detected or configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_format: Option<SourceFormat>,
    /// Count of converted annotations
    #[serde(default)]
    pub converted_count: u64,
    /// Count of merged annotations
    #[serde(default)]
    pub merged_count: u64,
    /// Count of explicit ACP annotations
    #[serde(default)]
    pub explicit_count: u64,
}

impl BridgeMetadata {
    /// Check if bridge metadata should be serialized
    pub fn is_empty(&self) -> bool {
        !self.enabled && self.converted_count == 0 && self.merged_count == 0
    }
}

/// @acp:summary "Top-level bridge statistics (RFC-0006)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStats {
    /// Whether bridging is enabled project-wide
    pub enabled: bool,
    /// Precedence mode (acp-first, native-first, merge)
    pub precedence: String,
    /// Summary statistics
    pub summary: BridgeSummary,
    /// Counts by source format
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub by_format: HashMap<String, u64>,
}

impl BridgeStats {
    /// Check if bridge stats are empty (for serialization skip)
    pub fn is_empty(&self) -> bool {
        !self.enabled && self.summary.total_annotations == 0
    }
}

/// @acp:summary "Bridge summary counts (RFC-0006)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeSummary {
    /// Total annotations
    pub total_annotations: u64,
    /// Explicit ACP annotations
    pub explicit_count: u64,
    /// Converted from native docs
    pub converted_count: u64,
    /// Merged ACP + native
    pub merged_count: u64,
}

// ============================================================================
// RFC-0009: Extended Annotation Types
// ============================================================================

/// @acp:summary "Behavioral characteristics of a symbol (RFC-0009)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehavioralAnnotations {
    /// Function has no side effects (from @acp:pure)
    #[serde(default, skip_serializing_if = "is_false")]
    pub pure: bool,
    /// Function is safe to call multiple times (from @acp:idempotent)
    #[serde(default, skip_serializing_if = "is_false")]
    pub idempotent: bool,
    /// Results are cached; string for duration (from @acp:memoized)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memoized: Option<MemoizedValue>,
    /// Function is asynchronous (from @acp:async)
    #[serde(default, skip_serializing_if = "is_false")]
    pub r#async: bool,
    /// Function is a generator (from @acp:generator)
    #[serde(default, skip_serializing_if = "is_false")]
    pub generator: bool,
    /// Rate limit specification (from @acp:throttled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttled: Option<String>,
    /// Function runs in a database transaction (from @acp:transactional)
    #[serde(default, skip_serializing_if = "is_false")]
    pub transactional: bool,
    /// List of side effects (from @acp:side-effects)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub side_effects: Vec<String>,
}

impl BehavioralAnnotations {
    /// Check if behavioral annotations are empty (for skip_serializing)
    pub fn is_empty(&self) -> bool {
        !self.pure
            && !self.idempotent
            && self.memoized.is_none()
            && !self.r#async
            && !self.generator
            && self.throttled.is_none()
            && !self.transactional
            && self.side_effects.is_empty()
    }
}

/// @acp:summary "Memoized value - either boolean or string duration (RFC-0009)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MemoizedValue {
    /// Simple memoization flag
    Enabled(bool),
    /// Memoization with duration (e.g., "5min", "1h")
    Duration(String),
}

/// @acp:summary "Lifecycle status of a symbol or file (RFC-0009)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleAnnotations {
    /// Deprecation message with version/replacement (from @acp:deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    /// API may change without notice (from @acp:experimental)
    #[serde(default, skip_serializing_if = "is_false")]
    pub experimental: bool,
    /// Feature in beta testing (from @acp:beta)
    #[serde(default, skip_serializing_if = "is_false")]
    pub beta: bool,
    /// Not intended for external use (from @acp:internal)
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal: bool,
    /// Stable public interface (from @acp:public-api)
    #[serde(default, skip_serializing_if = "is_false")]
    pub public_api: bool,
    /// Version when introduced (from @acp:since)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

impl LifecycleAnnotations {
    /// Check if lifecycle annotations are empty (for skip_serializing)
    pub fn is_empty(&self) -> bool {
        self.deprecated.is_none()
            && !self.experimental
            && !self.beta
            && !self.internal
            && !self.public_api
            && self.since.is_none()
    }
}

/// @acp:summary "Documentation metadata for a symbol (RFC-0009)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentationAnnotations {
    /// Code examples (from @acp:example)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    /// References to related symbols (from @acp:see)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see_also: Vec<String>,
    /// External documentation URLs (from @acp:link)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    /// Important notes (from @acp:note)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Warnings about usage (from @acp:warning)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Pending work items (from @acp:todo)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todos: Vec<String>,
}

impl DocumentationAnnotations {
    /// Check if documentation annotations are empty (for skip_serializing)
    pub fn is_empty(&self) -> bool {
        self.examples.is_empty()
            && self.see_also.is_empty()
            && self.links.is_empty()
            && self.notes.is_empty()
            && self.warnings.is_empty()
            && self.todos.is_empty()
    }
}

/// @acp:summary "Performance characteristics of a symbol (RFC-0009)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceAnnotations {
    /// Time complexity notation (from @acp:perf)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity: Option<String>,
    /// Space complexity notation (from @acp:memory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// Caching duration or strategy (from @acp:cached)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<String>,
}

impl PerformanceAnnotations {
    /// Check if performance annotations are empty (for skip_serializing)
    pub fn is_empty(&self) -> bool {
        self.complexity.is_none() && self.memory.is_none() && self.cached.is_none()
    }
}

// ============================================================================
// RFC-0008: Type Annotation Types
// ============================================================================

/// @acp:summary "Type annotation information for a symbol (RFC-0008)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfo {
    /// Parameter type information from @acp:param {Type}
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypeParamInfo>,
    /// Return type information from @acp:returns {Type}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeReturnInfo>,
    /// Generic type parameters from @acp:template
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<TypeTypeParam>,
}

impl TypeInfo {
    /// Check if type info is empty (for skip_serializing)
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.returns.is_none() && self.type_params.is_empty()
    }
}

/// @acp:summary "Parameter type information (RFC-0008)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeParamInfo {
    /// Parameter name (required)
    pub name: String,
    /// Type expression (e.g., "string", "Promise<User>")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Where the type came from: acp, inferred, native
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_source: Option<TypeSource>,
    /// Whether parameter is optional (from [name] syntax)
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
    /// Default value (from [name=default] syntax)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Directive text for this parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
}

/// @acp:summary "Return type information (RFC-0008)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeReturnInfo {
    /// Return type expression
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Where the type came from: acp, inferred, native
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_source: Option<TypeSource>,
    /// Directive text for return value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
}

/// @acp:summary "Generic type parameter (RFC-0008)"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeTypeParam {
    /// Type parameter name (e.g., "T")
    pub name: String,
    /// Constraint type from 'extends' clause
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    /// Directive text for type parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
}

// ============================================================================
// RFC-0003: Annotation Provenance Types
// ============================================================================

/// Provenance metadata for a single annotation value (RFC-0003)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationProvenance {
    /// The annotation value
    pub value: String,
    /// Origin of the annotation
    #[serde(default, skip_serializing_if = "is_explicit")]
    pub source: SourceOrigin,
    /// Confidence score (0.0-1.0), only for auto-generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Whether annotation is flagged for review
    #[serde(default, skip_serializing_if = "is_false")]
    pub needs_review: bool,
    /// Whether annotation has been reviewed
    #[serde(default, skip_serializing_if = "is_false")]
    pub reviewed: bool,
    /// When the annotation was reviewed (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<String>,
    /// When the annotation was generated (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// Generation batch identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
}

fn is_explicit(source: &SourceOrigin) -> bool {
    matches!(source, SourceOrigin::Explicit)
}

/// Top-level provenance statistics (RFC-0003)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceStats {
    /// Summary counts by source type
    pub summary: ProvenanceSummary,
    /// Annotations below confidence threshold
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub low_confidence: Vec<LowConfidenceEntry>,
    /// Information about the last generation run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_generation: Option<GenerationInfo>,
}

impl ProvenanceStats {
    /// Check if provenance stats are empty (for serialization skip)
    pub fn is_empty(&self) -> bool {
        self.summary.total == 0
    }
}

/// Summary of annotation provenance counts (RFC-0003)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceSummary {
    /// Total annotations tracked
    pub total: u64,
    /// Counts by source type
    pub by_source: SourceCounts,
    /// Count needing review
    pub needs_review: u64,
    /// Count already reviewed
    pub reviewed: u64,
    /// Average confidence by source type
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub average_confidence: HashMap<String, f64>,
}

/// Counts of annotations by source origin (RFC-0003)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceCounts {
    /// Human-written annotations
    #[serde(default)]
    pub explicit: u64,
    /// Converted from existing docs
    #[serde(default)]
    pub converted: u64,
    /// Inferred via heuristics
    #[serde(default)]
    pub heuristic: u64,
    /// Refined by AI
    #[serde(default)]
    pub refined: u64,
    /// Fully AI-inferred
    #[serde(default)]
    pub inferred: u64,
}

/// Entry for low-confidence annotation tracking (RFC-0003)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LowConfidenceEntry {
    /// Target file or symbol (file:symbol format)
    pub target: String,
    /// Annotation key (e.g., "@acp:summary")
    pub annotation: String,
    /// Confidence score
    pub confidence: f64,
    /// Annotation value
    pub value: String,
}

/// Information about a generation run (RFC-0003)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationInfo {
    /// Unique batch identifier
    pub id: String,
    /// When generation occurred (ISO 8601)
    pub timestamp: String,
    /// Number of annotations generated
    pub annotations_generated: u64,
    /// Number of files affected
    pub files_affected: u64,
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
                annotations: HashMap::new(), // RFC-0003
                // RFC-0009: Extended annotation types
                behavioral: None,
                lifecycle: None,
                documentation: None,
                performance: None,
                // RFC-0008: Type annotation info
                type_info: None,
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
        cache.files.insert(
            "./src/sample.ts".to_string(),
            FileEntry {
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
                annotations: HashMap::new(),       // RFC-0003
                bridge: BridgeMetadata::default(), // RFC-0006
                // RFC-0009: Extended file-level annotations
                version: None,
                since: None,
                license: None,
                author: None,
                lifecycle: None,
                // RFC-0002: Documentation references and style
                refs: vec![],
                style: None,
            },
        );
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
        cache.files.insert(
            "src/sample.ts".to_string(),
            FileEntry {
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
                annotations: HashMap::new(),       // RFC-0003
                bridge: BridgeMetadata::default(), // RFC-0006
                // RFC-0009: Extended file-level annotations
                version: None,
                since: None,
                license: None,
                author: None,
                lifecycle: None,
                // RFC-0002: Documentation references and style
                refs: vec![],
                style: None,
            },
        );

        // All formats should find it
        assert!(cache.get_file("src/sample.ts").is_some());
        assert!(cache.get_file("./src/sample.ts").is_some());
        assert!(cache.get_file("src\\sample.ts").is_some());
    }
}
