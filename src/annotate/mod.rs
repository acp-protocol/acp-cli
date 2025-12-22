//! @acp:module "Annotation Generation"
//! @acp:summary "Auto-annotation and documentation conversion for ACP adoption"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Annotation Generation
//!
//! This module provides functionality for:
//! - Analyzing code to identify symbols lacking ACP annotations
//! - Suggesting appropriate annotations based on heuristics
//! - Converting existing documentation standards (JSDoc, docstrings, etc.) to ACP format
//! - Applying changes via preview/dry-run or direct file modification
//!
//! ## Architecture
//!
//! The module is organized into several sub-modules:
//! - [`analyzer`] - Code analysis and gap detection
//! - [`suggester`] - Heuristics-based suggestion engine
//! - [`writer`] - File modification with diff support
//! - [`heuristics`] - Pattern-based inference rules
//! - [`converters`] - Per-standard documentation conversion

pub mod analyzer;
pub mod converters;
pub mod heuristics;
pub mod suggester;
pub mod writer;

pub use analyzer::Analyzer;
pub use converters::{DocStandardParser, ParsedDocumentation};
pub use suggester::Suggester;
pub use writer::Writer;

use serde::{Deserialize, Serialize};

use crate::ast::{SymbolKind, Visibility};

/// @acp:summary "Types of ACP annotations that can be suggested"
/// Represents the different annotation types supported by ACP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationType {
    /// @acp:module - Human-readable module name
    Module,
    /// @acp:summary - Brief description
    Summary,
    /// @acp:domain - Domain classification
    Domain,
    /// @acp:layer - Architectural layer
    Layer,
    /// @acp:lock - Mutation constraint level
    Lock,
    /// @acp:stability - Stability indicator
    Stability,
    /// @acp:deprecated - Deprecation notice
    Deprecated,
    /// @acp:ai-hint - AI behavioral hint
    AiHint,
    /// @acp:ref - Reference to another symbol
    Ref,
    /// @acp:hack - Temporary solution marker
    Hack,
    /// @acp:lock-reason - Justification for lock
    LockReason,
}

impl AnnotationType {
    /// @acp:summary "Formats annotation type with value into ACP syntax"
    /// Converts an annotation type and value into the proper `@acp:` format.
    ///
    /// # Arguments
    /// * `value` - The annotation value to format
    ///
    /// # Returns
    /// A string in the format `@acp:type value` or `@acp:type "value"`
    pub fn to_annotation_string(&self, value: &str) -> String {
        match self {
            Self::Module => format!("@acp:module \"{}\"", value),
            Self::Summary => format!("@acp:summary \"{}\"", value),
            Self::Domain => format!("@acp:domain {}", value),
            Self::Layer => format!("@acp:layer {}", value),
            Self::Lock => format!("@acp:lock {}", value),
            Self::Stability => format!("@acp:stability {}", value),
            Self::Deprecated => format!("@acp:deprecated \"{}\"", value),
            Self::AiHint => format!("@acp:ai-hint \"{}\"", value),
            Self::Ref => format!("@acp:ref \"{}\"", value),
            Self::Hack => format!("@acp:hack {}", value),
            Self::LockReason => format!("@acp:lock-reason \"{}\"", value),
        }
    }

    /// @acp:summary "Returns the namespace string for this annotation type"
    pub fn namespace(&self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Summary => "summary",
            Self::Domain => "domain",
            Self::Layer => "layer",
            Self::Lock => "lock",
            Self::Stability => "stability",
            Self::Deprecated => "deprecated",
            Self::AiHint => "ai-hint",
            Self::Ref => "ref",
            Self::Hack => "hack",
            Self::LockReason => "lock-reason",
        }
    }
}

impl std::fmt::Display for AnnotationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@acp:{}", self.namespace())
    }
}

/// @acp:summary "Source priority for annotation suggestions"
/// Determines the priority when merging suggestions from multiple sources.
/// Lower ordinal value means higher priority (Explicit > Converted > Heuristic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionSource {
    /// From existing @acp: annotation in source (highest priority)
    Explicit = 0,
    /// From converted doc standard (JSDoc, docstring, etc.)
    Converted = 1,
    /// From heuristics (naming, path, visibility)
    Heuristic = 2,
}

impl std::fmt::Display for SuggestionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit => write!(f, "explicit"),
            Self::Converted => write!(f, "converted"),
            Self::Heuristic => write!(f, "heuristic"),
        }
    }
}

/// @acp:summary "A suggested annotation to add to a symbol or file"
/// Represents a single annotation suggestion with its metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Target: file path for file-level, qualified_name for symbols
    pub target: String,

    /// Line number where annotation should be inserted (1-indexed)
    pub line: usize,

    /// The annotation type (summary, domain, lock, etc.)
    pub annotation_type: AnnotationType,

    /// The annotation value
    pub value: String,

    /// Source of this suggestion (for conflict resolution)
    pub source: SuggestionSource,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

impl Suggestion {
    /// @acp:summary "Creates a new suggestion"
    pub fn new(
        target: impl Into<String>,
        line: usize,
        annotation_type: AnnotationType,
        value: impl Into<String>,
        source: SuggestionSource,
    ) -> Self {
        Self {
            target: target.into(),
            line,
            annotation_type,
            value: value.into(),
            source,
            confidence: 1.0,
        }
    }

    /// @acp:summary "Creates a summary annotation suggestion"
    pub fn summary(target: impl Into<String>, line: usize, value: impl Into<String>, source: SuggestionSource) -> Self {
        Self::new(target, line, AnnotationType::Summary, value, source)
    }

    /// @acp:summary "Creates a domain annotation suggestion"
    pub fn domain(target: impl Into<String>, line: usize, value: impl Into<String>, source: SuggestionSource) -> Self {
        Self::new(target, line, AnnotationType::Domain, value, source)
    }

    /// @acp:summary "Creates a lock annotation suggestion"
    pub fn lock(target: impl Into<String>, line: usize, value: impl Into<String>, source: SuggestionSource) -> Self {
        Self::new(target, line, AnnotationType::Lock, value, source)
    }

    /// @acp:summary "Creates a layer annotation suggestion"
    pub fn layer(target: impl Into<String>, line: usize, value: impl Into<String>, source: SuggestionSource) -> Self {
        Self::new(target, line, AnnotationType::Layer, value, source)
    }

    /// @acp:summary "Creates a deprecated annotation suggestion"
    pub fn deprecated(target: impl Into<String>, line: usize, value: impl Into<String>, source: SuggestionSource) -> Self {
        Self::new(target, line, AnnotationType::Deprecated, value, source)
    }

    /// @acp:summary "Creates an AI hint annotation suggestion"
    pub fn ai_hint(target: impl Into<String>, line: usize, value: impl Into<String>, source: SuggestionSource) -> Self {
        Self::new(target, line, AnnotationType::AiHint, value, source)
    }

    /// @acp:summary "Sets the confidence score for this suggestion"
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// @acp:summary "Returns whether this is a file-level annotation"
    pub fn is_file_level(&self) -> bool {
        // File-level targets are paths (contain / or \)
        self.target.contains('/') || self.target.contains('\\')
    }

    /// @acp:summary "Formats the suggestion as an annotation string"
    pub fn to_annotation_string(&self) -> String {
        self.annotation_type.to_annotation_string(&self.value)
    }
}

/// @acp:summary "Result of analyzing a file for annotation gaps"
/// Contains information about existing annotations and missing ones.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Path to the analyzed file
    pub file_path: String,

    /// Detected language
    pub language: String,

    /// Existing ACP annotations found in the file
    pub existing_annotations: Vec<ExistingAnnotation>,

    /// Symbols that need annotations
    pub gaps: Vec<AnnotationGap>,

    /// Annotation coverage percentage (0.0 - 100.0)
    pub coverage: f32,
}

impl AnalysisResult {
    /// @acp:summary "Creates a new analysis result"
    pub fn new(file_path: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            language: language.into(),
            existing_annotations: Vec::new(),
            gaps: Vec::new(),
            coverage: 0.0,
        }
    }

    /// @acp:summary "Calculates the coverage percentage"
    pub fn calculate_coverage(&mut self) {
        let total = self.existing_annotations.len() + self.gaps.len();
        if total == 0 {
            self.coverage = 100.0;
        } else {
            self.coverage = (self.existing_annotations.len() as f32 / total as f32) * 100.0;
        }
    }
}

/// @acp:summary "An existing ACP annotation found in source"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingAnnotation {
    /// Target symbol or file path
    pub target: String,

    /// The annotation type
    pub annotation_type: AnnotationType,

    /// The annotation value
    pub value: String,

    /// Line number where found (1-indexed)
    pub line: usize,
}

/// @acp:summary "A symbol or file lacking required annotations"
/// Represents a gap in annotation coverage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationGap {
    /// Target symbol name or file path
    pub target: String,

    /// Symbol kind (None for file-level gaps)
    pub symbol_kind: Option<SymbolKind>,

    /// Line number of the symbol (1-indexed)
    pub line: usize,

    /// Which annotation types are missing
    pub missing: Vec<AnnotationType>,

    /// Existing doc comment (if any) that could be converted
    pub doc_comment: Option<String>,

    /// Line range of existing doc comment (start, end) - 1-indexed
    pub doc_comment_range: Option<(usize, usize)>,

    /// Whether this is exported/public
    pub is_exported: bool,

    /// Visibility of the symbol
    pub visibility: Option<Visibility>,
}

impl AnnotationGap {
    /// @acp:summary "Creates a new annotation gap"
    pub fn new(target: impl Into<String>, line: usize) -> Self {
        Self {
            target: target.into(),
            symbol_kind: None,
            line,
            missing: Vec::new(),
            doc_comment: None,
            doc_comment_range: None,
            is_exported: false,
            visibility: None,
        }
    }

    /// @acp:summary "Sets the symbol kind"
    pub fn with_symbol_kind(mut self, kind: SymbolKind) -> Self {
        self.symbol_kind = Some(kind);
        self
    }

    /// @acp:summary "Sets the doc comment"
    pub fn with_doc_comment(mut self, doc: impl Into<String>) -> Self {
        self.doc_comment = Some(doc.into());
        self
    }

    /// @acp:summary "Sets the doc comment with its line range"
    pub fn with_doc_comment_range(mut self, doc: impl Into<String>, start: usize, end: usize) -> Self {
        self.doc_comment = Some(doc.into());
        self.doc_comment_range = Some((start, end));
        self
    }

    /// @acp:summary "Marks as exported"
    pub fn exported(mut self) -> Self {
        self.is_exported = true;
        self
    }

    /// @acp:summary "Sets the visibility of the symbol"
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    /// @acp:summary "Adds a missing annotation type"
    pub fn add_missing(&mut self, annotation_type: AnnotationType) {
        if !self.missing.contains(&annotation_type) {
            self.missing.push(annotation_type);
        }
    }
}

/// @acp:summary "Annotation level for controlling generation depth"
/// Controls how many annotation types are generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotateLevel {
    /// Only @acp:module and @acp:summary
    Minimal,
    /// + @acp:domain, @acp:lock, @acp:layer
    #[default]
    Standard,
    /// + @acp:ref, @acp:stability, @acp:ai-hint
    Full,
}

impl AnnotateLevel {
    /// @acp:summary "Returns annotation types included at this level"
    pub fn included_types(&self) -> Vec<AnnotationType> {
        match self {
            Self::Minimal => vec![
                AnnotationType::Module,
                AnnotationType::Summary,
            ],
            Self::Standard => vec![
                AnnotationType::Module,
                AnnotationType::Summary,
                AnnotationType::Domain,
                AnnotationType::Lock,
                AnnotationType::Layer,
                AnnotationType::Deprecated,
            ],
            Self::Full => vec![
                AnnotationType::Module,
                AnnotationType::Summary,
                AnnotationType::Domain,
                AnnotationType::Lock,
                AnnotationType::Layer,
                AnnotationType::Deprecated,
                AnnotationType::Stability,
                AnnotationType::AiHint,
                AnnotationType::Ref,
                AnnotationType::Hack,
                AnnotationType::LockReason,
            ],
        }
    }

    /// @acp:summary "Checks if an annotation type is included at this level"
    pub fn includes(&self, annotation_type: AnnotationType) -> bool {
        self.included_types().contains(&annotation_type)
    }
}

/// @acp:summary "Source documentation standard for conversion"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionSource {
    /// Auto-detect based on language
    #[default]
    Auto,
    /// JSDoc (JavaScript/TypeScript)
    Jsdoc,
    /// TSDoc (TypeScript)
    Tsdoc,
    /// Python docstrings (Google/NumPy/Sphinx)
    Docstring,
    /// Rust doc comments
    Rustdoc,
    /// Go documentation comments
    Godoc,
    /// Javadoc
    Javadoc,
}

impl ConversionSource {
    /// @acp:summary "Returns the appropriate conversion source for a language"
    pub fn for_language(language: &str) -> Self {
        match language.to_lowercase().as_str() {
            "typescript" | "tsx" => Self::Tsdoc,
            "javascript" | "jsx" | "js" => Self::Jsdoc,
            "python" | "py" => Self::Docstring,
            "rust" | "rs" => Self::Rustdoc,
            "go" => Self::Godoc,
            "java" => Self::Javadoc,
            _ => Self::Auto,
        }
    }
}

/// @acp:summary "Output format for annotation results"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Unified diff format (default)
    #[default]
    Diff,
    /// JSON format for tooling integration
    Json,
    /// Summary statistics only
    Summary,
}

/// @acp:summary "A planned change to apply to a file"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// Path to the file
    pub file_path: String,

    /// Symbol name (None for file-level)
    pub symbol_name: Option<String>,

    /// Line number where to insert (1-indexed)
    pub line: usize,

    /// Annotations to add
    pub annotations: Vec<Suggestion>,

    /// Start line of existing doc comment (if any)
    pub existing_doc_start: Option<usize>,

    /// End line of existing doc comment (if any)
    pub existing_doc_end: Option<usize>,
}

impl FileChange {
    /// @acp:summary "Creates a new file change"
    pub fn new(file_path: impl Into<String>, line: usize) -> Self {
        Self {
            file_path: file_path.into(),
            symbol_name: None,
            line,
            annotations: Vec::new(),
            existing_doc_start: None,
            existing_doc_end: None,
        }
    }

    /// @acp:summary "Sets the symbol name"
    pub fn with_symbol(mut self, name: impl Into<String>) -> Self {
        self.symbol_name = Some(name.into());
        self
    }

    /// @acp:summary "Sets the existing doc comment range"
    pub fn with_existing_doc(mut self, start: usize, end: usize) -> Self {
        self.existing_doc_start = Some(start);
        self.existing_doc_end = Some(end);
        self
    }

    /// @acp:summary "Adds an annotation to this change"
    pub fn add_annotation(&mut self, suggestion: Suggestion) {
        self.annotations.push(suggestion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_type_formatting() {
        assert_eq!(
            AnnotationType::Summary.to_annotation_string("Test summary"),
            "@acp:summary \"Test summary\""
        );
        assert_eq!(
            AnnotationType::Domain.to_annotation_string("authentication"),
            "@acp:domain authentication"
        );
        assert_eq!(
            AnnotationType::Lock.to_annotation_string("restricted"),
            "@acp:lock restricted"
        );
    }

    #[test]
    fn test_suggestion_source_ordering() {
        assert!(SuggestionSource::Explicit < SuggestionSource::Converted);
        assert!(SuggestionSource::Converted < SuggestionSource::Heuristic);
    }

    #[test]
    fn test_annotate_level_includes() {
        assert!(AnnotateLevel::Minimal.includes(AnnotationType::Summary));
        assert!(!AnnotateLevel::Minimal.includes(AnnotationType::Domain));
        assert!(AnnotateLevel::Standard.includes(AnnotationType::Domain));
        assert!(AnnotateLevel::Full.includes(AnnotationType::AiHint));
    }

    #[test]
    fn test_suggestion_is_file_level() {
        let file_suggestion = Suggestion::summary("src/main.rs", 1, "Test", SuggestionSource::Heuristic);
        let symbol_suggestion = Suggestion::summary("MyClass", 10, "Test", SuggestionSource::Heuristic);

        assert!(file_suggestion.is_file_level());
        assert!(!symbol_suggestion.is_file_level());
    }

    #[test]
    fn test_conversion_source_for_language() {
        assert_eq!(ConversionSource::for_language("typescript"), ConversionSource::Tsdoc);
        assert_eq!(ConversionSource::for_language("python"), ConversionSource::Docstring);
        assert_eq!(ConversionSource::for_language("rust"), ConversionSource::Rustdoc);
        assert_eq!(ConversionSource::for_language("unknown"), ConversionSource::Auto);
    }
}
