//! @acp:module "Parser"
//! @acp:summary "Source code parsing and annotation extraction (RFC-001/RFC-003 compliant)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Parses source files to extract symbols, calls, and documentation.
//! Supports RFC-001 self-documenting annotations with directive extraction.
//! Supports RFC-003 annotation provenance tracking.
//! Currently uses regex-based parsing with tree-sitter support planned.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cache::{
    BehavioralAnnotations, DocumentationAnnotations, FileEntry, InlineAnnotation,
    LifecycleAnnotations, MemoizedValue, PerformanceAnnotations, SymbolEntry, SymbolType,
    TypeInfo, TypeParamInfo, TypeReturnInfo, TypeSource, TypeTypeParam, Visibility,
};
use crate::error::{AcpError, Result};
use crate::index::detect_language;

/// Regex pattern for parsing @acp: annotations with directive support (RFC-001)
/// Matches: @acp:name [value] [- directive]
/// Groups: 1=name, 2=value (before dash), 3=directive (after dash)
static ANNOTATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@acp:([\w-]+)(?:\s+([^-\n]+?))?(?:\s+-\s+(.+))?$").unwrap()
});

/// Regex for detecting comment continuation lines (for multiline directives)
static CONTINUATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?://|#|/?\*)\s{2,}(.+)$").unwrap()
});

// ============================================================================
// RFC-0003: Annotation Provenance Tracking
// ============================================================================

/// Regex for @acp:source annotation (RFC-0003)
static SOURCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@acp:source\s+(explicit|converted|heuristic|refined|inferred)(?:\s+-\s+(.+))?$")
        .unwrap()
});

/// Regex for @acp:source-confidence annotation (RFC-0003)
static CONFIDENCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@acp:source-confidence\s+(\d+\.?\d*)(?:\s+-\s+(.+))?$").unwrap()
});

/// Regex for @acp:source-reviewed annotation (RFC-0003)
static REVIEWED_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@acp:source-reviewed\s+(true|false)(?:\s+-\s+(.+))?$").unwrap()
});

/// Regex for @acp:source-id annotation (RFC-0003)
static ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@acp:source-id\s+([a-zA-Z0-9\-]+)(?:\s+-\s+(.+))?$").unwrap()
});

/// Source origin for annotation provenance (RFC-0003)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SourceOrigin {
    /// Annotation was written by a human developer
    #[default]
    Explicit,
    /// Annotation was converted from existing documentation (JSDoc, rustdoc, etc.)
    Converted,
    /// Annotation was inferred using heuristic analysis
    Heuristic,
    /// Annotation was refined by AI from lower-quality source
    Refined,
    /// Annotation was fully inferred by AI
    Inferred,
}

impl SourceOrigin {
    /// Get string representation for serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceOrigin::Explicit => "explicit",
            SourceOrigin::Converted => "converted",
            SourceOrigin::Heuristic => "heuristic",
            SourceOrigin::Refined => "refined",
            SourceOrigin::Inferred => "inferred",
        }
    }
}

impl std::str::FromStr for SourceOrigin {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "explicit" => Ok(SourceOrigin::Explicit),
            "converted" => Ok(SourceOrigin::Converted),
            "heuristic" => Ok(SourceOrigin::Heuristic),
            "refined" => Ok(SourceOrigin::Refined),
            "inferred" => Ok(SourceOrigin::Inferred),
            _ => Err(format!("Unknown source origin: {}", s)),
        }
    }
}

impl std::fmt::Display for SourceOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Provenance metadata for an annotation (RFC-0003)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceMarker {
    /// Source origin (explicit, converted, heuristic, refined, inferred)
    pub source: SourceOrigin,
    /// Confidence score (0.0-1.0), only for auto-generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Whether annotation has been reviewed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<bool>,
    /// Generation batch identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
}

/// Extended annotation with provenance (RFC-0003)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationWithProvenance {
    /// The base annotation
    pub annotation: Annotation,
    /// Optional provenance metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceMarker>,
}

/// @acp:summary "Result of parsing a source file"
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub file: FileEntry,
    pub symbols: Vec<SymbolEntry>,
    pub calls: Vec<(String, Vec<String>)>, // (caller, callees)
    pub lock_level: Option<String>,         // from @acp:lock
    pub lock_directive: Option<String>,     // RFC-001: directive text for lock
    pub ai_hints: Vec<String>,              // from @acp:ai-careful, @acp:ai-readonly, etc.
    pub hacks: Vec<HackAnnotation>,         // from @acp:hack
    pub inline_annotations: Vec<InlineAnnotation>, // RFC-001: inline annotations (todo, fixme, critical, perf)
    pub purpose: Option<String>,            // RFC-001: file purpose from @acp:purpose
    pub owner: Option<String>,              // RFC-001: file owner from @acp:owner
}

/// @acp:summary "Parsed hack annotation"
#[derive(Debug, Clone)]
pub struct HackAnnotation {
    pub line: usize,
    pub expires: Option<String>,
    pub ticket: Option<String>,
    pub reason: Option<String>,
}

/// @acp:summary "Parser for source files"
pub struct Parser {
    // tree-sitter parsers would be initialized here
    // For now, this is a stub implementation
}

impl Parser {
    pub fn new() -> Self {
        Self {}
    }

    /// @acp:summary "Parse a source file and extract metadata"
    pub fn parse<P: AsRef<Path>>(&self, path: P) -> Result<ParseResult> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        let file_path = path.to_string_lossy().to_string();

        let language = detect_language(&file_path)
            .ok_or_else(|| AcpError::UnsupportedLanguage(
                path.extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default()
            ))?;

        let lines = content.lines().count();
        let _file_name = path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        // Parse @acp: annotations from source
        let annotations = self.parse_annotations(&content);

        // Extract file-level metadata from annotations
        let mut module_name = None;
        let mut file_summary = None;
        let mut domains = vec![];
        let mut layer = None;
        let mut symbols = vec![];
        let mut exports = vec![];
        let mut imports = vec![];
        let mut calls = vec![];
        let mut lock_level = None;
        let mut lock_directive = None;
        let mut ai_hints = vec![];
        let mut hacks = vec![];
        let mut inline_annotations = vec![];
        let mut purpose = None;
        let mut owner = None;

        // RFC-0009: File-level extended annotation accumulators
        let mut file_version: Option<String> = None;
        let mut file_since: Option<String> = None;
        let mut file_license: Option<String> = None;
        let mut file_author: Option<String> = None;
        let mut file_lifecycle = LifecycleAnnotations::default();

        // Track current symbol context for multi-line annotations
        let mut current_symbol: Option<SymbolBuilder> = None;

        for ann in &annotations {
            match ann.name.as_str() {
                "module" => {
                    if let Some(val) = &ann.value {
                        module_name = Some(val.trim_matches('"').to_string());
                    }
                }
                "summary" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.summary = Some(val.trim_matches('"').to_string());
                        }
                    } else if let Some(val) = &ann.value {
                        // File-level summary
                        file_summary = Some(val.trim_matches('"').to_string());
                    }
                }
                "domain" => {
                    if let Some(val) = &ann.value {
                        domains.push(val.trim_matches('"').to_string());
                    }
                }
                "layer" => {
                    if let Some(val) = &ann.value {
                        layer = Some(val.trim_matches('"').to_string());
                    }
                }
                "lock" => {
                    if let Some(val) = &ann.value {
                        lock_level = Some(val.trim_matches('"').to_string());
                    }
                    // RFC-001: Capture directive for lock annotation
                    lock_directive = ann.directive.clone();
                }
                // RFC-001: File purpose annotation
                "purpose" => {
                    if let Some(val) = &ann.value {
                        purpose = Some(val.trim_matches('"').to_string());
                    } else if let Some(dir) = &ann.directive {
                        purpose = Some(dir.clone());
                    }
                }
                // RFC-001: File owner annotation
                "owner" => {
                    if let Some(val) = &ann.value {
                        owner = Some(val.trim_matches('"').to_string());
                    }
                }
                "ai-careful" | "ai-readonly" | "ai-avoid" | "ai-no-modify" => {
                    let hint = if let Some(val) = &ann.value {
                        format!("{}: {}", ann.name, val.trim_matches('"'))
                    } else {
                        ann.name.clone()
                    };
                    ai_hints.push(hint);
                }
                "hack" => {
                    // Parse hack annotation: @acp:hack expires=2025-03-01 ticket=JIRA-123 "reason"
                    let mut expires = None;
                    let mut ticket = None;
                    let mut reason = None;

                    if let Some(val) = &ann.value {
                        // Parse key=value pairs and quoted reason
                        for part in val.split_whitespace() {
                            if let Some(exp) = part.strip_prefix("expires=") {
                                expires = Some(exp.to_string());
                            } else if let Some(tkt) = part.strip_prefix("ticket=") {
                                ticket = Some(tkt.to_string());
                            } else if part.starts_with('"') {
                                // Capture the rest as reason
                                reason = Some(val.split('"').nth(1).unwrap_or("").to_string());
                                break;
                            }
                        }
                    }

                    let hack = HackAnnotation {
                        line: ann.line,
                        expires: expires.clone(),
                        ticket: ticket.clone(),
                        reason,
                    };
                    hacks.push(hack);

                    // RFC-001: Also add to inline annotations
                    inline_annotations.push(InlineAnnotation {
                        line: ann.line,
                        annotation_type: "hack".to_string(),
                        value: ann.value.clone(),
                        directive: ann.directive.clone().unwrap_or_else(|| "Temporary workaround".to_string()),
                        expires,
                        ticket,
                        auto_generated: ann.auto_generated,
                    });
                }
                // RFC-001: Inline annotation types
                "todo" | "fixme" | "critical" => {
                    inline_annotations.push(InlineAnnotation {
                        line: ann.line,
                        annotation_type: ann.name.clone(),
                        value: ann.value.clone(),
                        directive: ann.directive.clone().unwrap_or_else(|| {
                            match ann.name.as_str() {
                                "todo" => "Pending work item".to_string(),
                                "fixme" => "Known issue requiring fix".to_string(),
                                "critical" => "Critical section - extra review required".to_string(),
                                _ => "".to_string(),
                            }
                        }),
                        expires: None,
                        ticket: None,
                        auto_generated: ann.auto_generated,
                    });
                    // RFC-0009: Also add to symbol documentation.todos
                    if ann.name == "todo" {
                        if let Some(ref mut builder) = current_symbol {
                            let todo_text = ann.directive.clone()
                                .or_else(|| ann.value.clone().map(|v| v.trim_matches('"').to_string()))
                                .unwrap_or_else(|| "Pending work item".to_string());
                            builder.documentation.todos.push(todo_text);
                        }
                    }
                }
                // RFC-0009: Performance annotation (extends RFC-001 perf)
                "perf" => {
                    inline_annotations.push(InlineAnnotation {
                        line: ann.line,
                        annotation_type: ann.name.clone(),
                        value: ann.value.clone(),
                        directive: ann.directive.clone().unwrap_or_else(|| "Performance-sensitive code".to_string()),
                        expires: None,
                        ticket: None,
                        auto_generated: ann.auto_generated,
                    });
                    // RFC-0009: Also set symbol performance.complexity
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.performance.complexity = Some(val.trim_matches('"').to_string());
                        }
                    }
                }
                "symbol" => {
                    // Save previous symbol if exists
                    if let Some(builder) = current_symbol.take() {
                        let sym = builder.build(&file_path);
                        exports.push(sym.name.clone());
                        symbols.push(sym);
                    }
                    // Start new symbol
                    if let Some(val) = &ann.value {
                        current_symbol = Some(SymbolBuilder::new(
                            val.trim_matches('"').to_string(),
                            ann.line,
                            &file_path,
                        ));
                    }
                }
                // RFC-001: Symbol-level annotations
                "fn" | "function" | "class" | "method" => {
                    // Save previous symbol if exists
                    if let Some(builder) = current_symbol.take() {
                        let sym = builder.build(&file_path);
                        exports.push(sym.name.clone());
                        symbols.push(sym);
                    }
                    // Start new symbol with RFC-001 type
                    if let Some(val) = &ann.value {
                        let mut builder = SymbolBuilder::new(
                            val.trim_matches('"').to_string(),
                            ann.line,
                            &file_path,
                        );
                        builder.symbol_type = match ann.name.as_str() {
                            "fn" | "function" => SymbolType::Function,
                            "class" => SymbolType::Class,
                            "method" => SymbolType::Method,
                            _ => SymbolType::Function,
                        };
                        builder.purpose = ann.directive.clone();
                        current_symbol = Some(builder);
                    }
                }
                "calls" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            let callees: Vec<String> = val
                                .split(',')
                                .map(|s| s.trim().trim_matches('"').to_string())
                                .collect();
                            builder.calls.extend(callees);
                        }
                    }
                }
                "imports" | "depends" => {
                    if let Some(val) = &ann.value {
                        let import_list: Vec<String> = val
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').to_string())
                            .collect();
                        imports.extend(import_list);
                    }
                }

                // ================================================================
                // RFC-0008: Type Annotations
                // ================================================================
                "param" => {
                    if let Some(ref mut builder) = current_symbol {
                        // Parse type, name, optional marker, and default from value
                        // Value format: "{Type} [name]=default" or just "name"
                        if let Some(val) = &ann.value {
                            let val = val.trim();
                            let (type_expr, rest) = if val.starts_with('{') {
                                // Extract type from {Type}
                                if let Some(close_idx) = val.find('}') {
                                    let type_str = val[1..close_idx].trim().to_string();
                                    let remaining = val[close_idx + 1..].trim();
                                    (Some(type_str), remaining)
                                } else {
                                    (None, val)
                                }
                            } else {
                                (None, val)
                            };

                            // Parse optional marker and name: [name]=default or name
                            let (optional, name, default) = if rest.starts_with('[') {
                                // Optional parameter: [name] or [name=default]
                                if let Some(close_idx) = rest.find(']') {
                                    let inner = &rest[1..close_idx];
                                    if let Some(eq_idx) = inner.find('=') {
                                        let n = inner[..eq_idx].trim().to_string();
                                        let d = inner[eq_idx + 1..].trim().to_string();
                                        (true, n, Some(d))
                                    } else {
                                        (true, inner.trim().to_string(), None)
                                    }
                                } else {
                                    (false, rest.trim_matches('"').to_string(), None)
                                }
                            } else {
                                // Required parameter
                                let name = rest.split_whitespace().next()
                                    .unwrap_or("")
                                    .trim_matches('"')
                                    .to_string();
                                (false, name, None)
                            };

                            if !name.is_empty() {
                                builder.type_info.params.push(TypeParamInfo {
                                    name,
                                    r#type: type_expr.clone(),
                                    type_source: type_expr.as_ref().map(|_| TypeSource::Acp),
                                    optional,
                                    default,
                                    directive: ann.directive.clone(),
                                });
                            }
                        }
                    }
                }
                "returns" | "return" => {
                    if let Some(ref mut builder) = current_symbol {
                        // Parse type from value: "{Type}" or empty
                        let type_expr = ann.value.as_ref().and_then(|val| {
                            let val = val.trim();
                            if val.starts_with('{') {
                                val.find('}').map(|close_idx| val[1..close_idx].trim().to_string())
                            } else {
                                None
                            }
                        });

                        builder.type_info.returns = Some(TypeReturnInfo {
                            r#type: type_expr.clone(),
                            type_source: type_expr.as_ref().map(|_| TypeSource::Acp),
                            directive: ann.directive.clone(),
                        });
                    }
                }
                "template" => {
                    if let Some(ref mut builder) = current_symbol {
                        // Parse @acp:template T [extends Constraint] from value
                        if let Some(val) = &ann.value {
                            let val = val.trim();
                            // Check for "extends" keyword
                            let (name, constraint) = if let Some(extends_idx) = val.find(" extends ") {
                                let n = val[..extends_idx].trim().to_string();
                                let c = val[extends_idx + 9..].trim().to_string();
                                (n, Some(c))
                            } else {
                                (val.split_whitespace().next().unwrap_or("").to_string(), None)
                            };

                            if !name.is_empty() {
                                builder.type_info.type_params.push(TypeTypeParam {
                                    name,
                                    constraint,
                                    directive: ann.directive.clone(),
                                });
                            }
                        }
                    }
                }

                // ================================================================
                // RFC-0009: Behavioral Annotations
                // ================================================================
                "pure" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.behavioral.pure = true;
                    }
                }
                "idempotent" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.behavioral.idempotent = true;
                    }
                }
                "memoized" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.behavioral.memoized =
                                Some(MemoizedValue::Duration(val.trim_matches('"').to_string()));
                        } else {
                            builder.behavioral.memoized = Some(MemoizedValue::Enabled(true));
                        }
                    }
                }
                "async" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.behavioral.r#async = true;
                    }
                }
                "generator" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.behavioral.generator = true;
                    }
                }
                "throttled" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.behavioral.throttled = Some(val.trim_matches('"').to_string());
                        }
                    }
                }
                "transactional" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.behavioral.transactional = true;
                    }
                }
                "side-effects" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            let effects: Vec<String> = val
                                .split(',')
                                .map(|s| s.trim().trim_matches('"').to_string())
                                .collect();
                            builder.behavioral.side_effects.extend(effects);
                        }
                    }
                }

                // ================================================================
                // RFC-0009: Lifecycle Annotations (file and symbol level)
                // ================================================================
                "deprecated" => {
                    let message = ann.directive.clone()
                        .or_else(|| ann.value.clone().map(|v| v.trim_matches('"').to_string()))
                        .unwrap_or_else(|| "Deprecated".to_string());
                    if let Some(ref mut builder) = current_symbol {
                        builder.lifecycle.deprecated = Some(message);
                    } else {
                        file_lifecycle.deprecated = Some(message);
                    }
                }
                "experimental" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.lifecycle.experimental = true;
                    } else {
                        file_lifecycle.experimental = true;
                    }
                }
                "beta" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.lifecycle.beta = true;
                    } else {
                        file_lifecycle.beta = true;
                    }
                }
                "internal" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.lifecycle.internal = true;
                    } else {
                        file_lifecycle.internal = true;
                    }
                }
                "public-api" => {
                    if let Some(ref mut builder) = current_symbol {
                        builder.lifecycle.public_api = true;
                    } else {
                        file_lifecycle.public_api = true;
                    }
                }
                "since" => {
                    if let Some(val) = &ann.value {
                        let version = val.trim_matches('"').to_string();
                        if let Some(ref mut builder) = current_symbol {
                            builder.lifecycle.since = Some(version);
                        } else {
                            file_since = Some(version);
                        }
                    }
                }

                // ================================================================
                // RFC-0009: Documentation Annotations
                // ================================================================
                "example" => {
                    if let Some(ref mut builder) = current_symbol {
                        let example_text = ann.directive.clone()
                            .or_else(|| ann.value.clone().map(|v| v.trim_matches('"').to_string()))
                            .unwrap_or_default();
                        if !example_text.is_empty() {
                            builder.documentation.examples.push(example_text);
                        }
                    }
                }
                "see" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.documentation.see_also.push(val.trim_matches('"').to_string());
                        }
                    }
                }
                "link" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.documentation.links.push(val.trim_matches('"').to_string());
                        }
                    }
                }
                "note" => {
                    if let Some(ref mut builder) = current_symbol {
                        let note_text = ann.directive.clone()
                            .or_else(|| ann.value.clone().map(|v| v.trim_matches('"').to_string()))
                            .unwrap_or_default();
                        if !note_text.is_empty() {
                            builder.documentation.notes.push(note_text);
                        }
                    }
                }
                "warning" => {
                    if let Some(ref mut builder) = current_symbol {
                        let warning_text = ann.directive.clone()
                            .or_else(|| ann.value.clone().map(|v| v.trim_matches('"').to_string()))
                            .unwrap_or_default();
                        if !warning_text.is_empty() {
                            builder.documentation.warnings.push(warning_text);
                        }
                    }
                }

                // ================================================================
                // RFC-0009: Performance Annotations (memory, cached)
                // ================================================================
                "memory" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.performance.memory = Some(val.trim_matches('"').to_string());
                        }
                    }
                }
                "cached" => {
                    if let Some(ref mut builder) = current_symbol {
                        if let Some(val) = &ann.value {
                            builder.performance.cached = Some(val.trim_matches('"').to_string());
                        } else {
                            builder.performance.cached = Some("true".to_string());
                        }
                    }
                }

                // ================================================================
                // RFC-0009: File-Level Annotations
                // ================================================================
                "version" => {
                    if let Some(val) = &ann.value {
                        file_version = Some(val.trim_matches('"').to_string());
                    }
                }
                "license" => {
                    if let Some(val) = &ann.value {
                        file_license = Some(val.trim_matches('"').to_string());
                    }
                }
                "author" => {
                    if let Some(val) = &ann.value {
                        file_author = Some(val.trim_matches('"').to_string());
                    }
                }

                _ => {}
            }
        }

        // Save last symbol
        if let Some(builder) = current_symbol {
            let sym = builder.build(&file_path);
            if !sym.calls.is_empty() {
                calls.push((sym.name.clone(), sym.calls.clone()));
            }
            exports.push(sym.name.clone());
            symbols.push(sym);
        }

        // Build call edges for earlier symbols
        for sym in &symbols {
            if !sym.calls.is_empty() {
                calls.push((sym.name.clone(), sym.calls.clone()));
            }
        }

        let file = FileEntry {
            path: file_path,
            lines,
            language,
            exports,
            imports,
            module: module_name,
            summary: file_summary,
            purpose: purpose.clone(),
            owner: owner.clone(),
            inline: inline_annotations.clone(),
            domains,
            layer,
            stability: None,
            ai_hints: ai_hints.clone(),
            git: None,
            annotations: std::collections::HashMap::new(), // RFC-0003: Populated during indexing
            bridge: crate::cache::BridgeMetadata::default(), // RFC-0006: Populated during bridging
            // RFC-0009: Extended file-level annotations
            version: file_version,
            since: file_since,
            license: file_license,
            author: file_author,
            lifecycle: if file_lifecycle.is_empty() {
                None
            } else {
                Some(file_lifecycle)
            },
            // RFC-0002: Populated during indexing with validation
            refs: Vec::new(),
            style: None,
        };

        Ok(ParseResult {
            file,
            symbols,
            calls,
            lock_level,
            lock_directive,
            ai_hints,
            hacks,
            inline_annotations,
            purpose,
            owner,
        })
    }

    /// @acp:summary "Parse @acp: annotations from source comments (RFC-001)"
    /// Extracts annotations with directive suffix support and multiline continuation.
    pub fn parse_annotations(&self, content: &str) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let line_1indexed = i + 1;

            for cap in ANNOTATION_PATTERN.captures_iter(line) {
                let name = cap.get(1).unwrap().as_str().to_string();
                let value = cap.get(2).map(|m| m.as_str().trim().to_string());
                let mut directive = cap.get(3).map(|m| m.as_str().trim().to_string());

                // Check for multiline directive continuation
                let mut j = i + 1;
                while j < lines.len() {
                    if let Some(cont_cap) = CONTINUATION_PATTERN.captures(lines[j]) {
                        let continuation = cont_cap.get(1).unwrap().as_str().trim();
                        if let Some(ref mut dir) = directive {
                            dir.push(' ');
                            dir.push_str(continuation);
                        } else {
                            directive = Some(continuation.to_string());
                        }
                        j += 1;
                    } else {
                        break;
                    }
                }

                // Auto-generate directive if missing (per RFC-001 Q04 decision)
                let (final_directive, auto_generated) = match directive {
                    Some(d) if !d.is_empty() => (Some(d), false),
                    _ => (self.default_directive(&name, value.as_deref()), true),
                };

                annotations.push(Annotation {
                    name,
                    value,
                    directive: final_directive,
                    auto_generated,
                    line: line_1indexed,
                });
            }

            i += 1;
        }

        annotations
    }

    /// @acp:summary "Generate default directive for annotation type (RFC-001 Q04)"
    /// Returns auto-generated directive text based on annotation type and value.
    fn default_directive(&self, name: &str, value: Option<&str>) -> Option<String> {
        match name {
            "lock" => match value {
                Some("frozen") => Some("MUST NOT modify this code under any circumstances".into()),
                Some("restricted") => Some("Explain proposed changes and wait for explicit approval".into()),
                Some("approval-required") => Some("Propose changes and request confirmation before applying".into()),
                Some("tests-required") => Some("All changes must include corresponding tests".into()),
                Some("docs-required") => Some("All changes must update documentation".into()),
                Some("review-required") => Some("Changes require code review before merging".into()),
                Some("normal") | None => Some("Safe to modify following project conventions".into()),
                Some("experimental") => Some("Experimental code - changes welcome but may be unstable".into()),
                _ => None,
            },
            "ref" => value.map(|url| format!("Consult {} before making changes", url)),
            "hack" => Some("Temporary workaround - check expiry before modifying".into()),
            "deprecated" => Some("Do not use or extend - see replacement annotation".into()),
            "todo" => Some("Pending work item - address before release".into()),
            "fixme" => Some("Known issue requiring fix - prioritize resolution".into()),
            "critical" => Some("Critical section - changes require extra review".into()),
            "perf" => Some("Performance-sensitive code - benchmark any changes".into()),
            "fn" | "function" => Some("Function implementation".into()),
            "class" => Some("Class definition".into()),
            "method" => Some("Method implementation".into()),
            "purpose" => value.map(|v| v.trim_matches('"').to_string()),
            _ => None,
        }
    }

    // ========================================================================
    // RFC-0003: Provenance Parsing Methods
    // ========================================================================

    /// Parse provenance annotations from comment lines (RFC-0003)
    ///
    /// Looks for @acp:source* annotations following the given start index.
    /// Returns a ProvenanceMarker if any provenance annotations are found.
    pub fn parse_provenance(&self, lines: &[&str], start_idx: usize) -> Option<ProvenanceMarker> {
        let mut marker = ProvenanceMarker::default();
        let mut found_any = false;

        for line in lines.iter().skip(start_idx) {
            let line = *line;

            // Check for @acp:source
            if let Some(cap) = SOURCE_PATTERN.captures(line) {
                if let Ok(origin) = cap.get(1).unwrap().as_str().parse() {
                    marker.source = origin;
                    found_any = true;
                }
            }

            // Check for @acp:source-confidence
            if let Some(cap) = CONFIDENCE_PATTERN.captures(line) {
                if let Ok(conf) = cap.get(1).unwrap().as_str().parse::<f64>() {
                    // Clamp to valid range [0.0, 1.0]
                    marker.confidence = Some(conf.clamp(0.0, 1.0));
                    found_any = true;
                }
            }

            // Check for @acp:source-reviewed
            if let Some(cap) = REVIEWED_PATTERN.captures(line) {
                marker.reviewed = Some(cap.get(1).unwrap().as_str() == "true");
                found_any = true;
            }

            // Check for @acp:source-id
            if let Some(cap) = ID_PATTERN.captures(line) {
                marker.generation_id = Some(cap.get(1).unwrap().as_str().to_string());
                found_any = true;
            }

            // Stop if we hit a non-provenance @acp annotation or non-comment line
            let trimmed = line.trim();
            let is_comment = trimmed.starts_with("//")
                || trimmed.starts_with('*')
                || trimmed.starts_with('#')
                || trimmed.starts_with("/*");

            if !is_comment {
                break;
            }

            // If it's a comment with @acp: but not a provenance annotation, stop
            if line.contains("@acp:") && !line.contains("@acp:source") {
                break;
            }
        }

        if found_any {
            Some(marker)
        } else {
            None
        }
    }

    /// Parse @acp: annotations with provenance support (RFC-0003)
    ///
    /// Returns annotations paired with their provenance metadata if present.
    /// Provenance is detected from @acp:source* annotations following the main annotation.
    pub fn parse_annotations_with_provenance(&self, content: &str) -> Vec<AnnotationWithProvenance> {
        let annotations = self.parse_annotations(content);
        let lines: Vec<&str> = content.lines().collect();

        annotations
            .into_iter()
            .map(|ann| {
                // Look for provenance markers on lines following the annotation
                // Annotations are 1-indexed, so ann.line points to the actual line
                let provenance = if ann.line < lines.len() {
                    self.parse_provenance(&lines, ann.line)
                } else {
                    None
                };

                AnnotationWithProvenance {
                    annotation: ann,
                    provenance,
                }
            })
            .collect()
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// @acp:summary "Parsed annotation from source (RFC-001 compliant)"
/// Supports directive extraction for self-documenting annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// Annotation type (e.g., "lock", "ref", "hack", "fn", "class")
    pub name: String,
    /// Primary value after the annotation name
    pub value: Option<String>,
    /// Self-documenting directive text after ` - ` (RFC-001)
    pub directive: Option<String>,
    /// Whether directive was auto-generated from defaults (RFC-001)
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_generated: bool,
    /// Source line number (1-indexed)
    pub line: usize,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Helper to build SymbolEntry from annotations
struct SymbolBuilder {
    name: String,
    qualified_name: String,
    line: usize,
    summary: Option<String>,
    purpose: Option<String>,
    calls: Vec<String>,
    symbol_type: SymbolType,
    // RFC-0009: Extended annotation accumulators
    behavioral: BehavioralAnnotations,
    lifecycle: LifecycleAnnotations,
    documentation: DocumentationAnnotations,
    performance: PerformanceAnnotations,
    // RFC-0008: Type annotation accumulator
    type_info: TypeInfo,
}

impl SymbolBuilder {
    fn new(name: String, line: usize, file_path: &str) -> Self {
        let qualified_name = format!("{}:{}", file_path, name);
        Self {
            name,
            qualified_name,
            line,
            summary: None,
            purpose: None,
            calls: vec![],
            symbol_type: SymbolType::Function,
            // RFC-0009: Initialize with defaults
            behavioral: BehavioralAnnotations::default(),
            lifecycle: LifecycleAnnotations::default(),
            documentation: DocumentationAnnotations::default(),
            performance: PerformanceAnnotations::default(),
            // RFC-0008: Initialize with defaults
            type_info: TypeInfo::default(),
        }
    }

    fn build(self, file_path: &str) -> SymbolEntry {
        SymbolEntry {
            name: self.name,
            qualified_name: self.qualified_name,
            symbol_type: self.symbol_type,
            file: file_path.to_string(),
            lines: [self.line, self.line + 10], // Approximate
            exported: true,
            signature: None,
            summary: self.summary,
            purpose: self.purpose,
            async_fn: self.behavioral.r#async, // RFC-0009: Use behavioral async flag
            visibility: Visibility::Public,
            calls: self.calls,
            called_by: vec![], // Populated later by indexer
            git: None,
            constraints: None,
            annotations: std::collections::HashMap::new(), // RFC-0003
            // RFC-0009: Extended annotation types (sparse serialization)
            behavioral: if self.behavioral.is_empty() {
                None
            } else {
                Some(self.behavioral)
            },
            lifecycle: if self.lifecycle.is_empty() {
                None
            } else {
                Some(self.lifecycle)
            },
            documentation: if self.documentation.is_empty() {
                None
            } else {
                Some(self.documentation)
            },
            performance: if self.performance.is_empty() {
                None
            } else {
                Some(self.performance)
            },
            // RFC-0008: Type annotation info (sparse serialization)
            type_info: if self.type_info.is_empty() {
                None
            } else {
                Some(self.type_info)
            },
        }
    }
}

// ============================================================================
// RFC-0008: Type Annotation Tests
// ============================================================================

#[cfg(test)]
mod type_annotation_tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn parse_test_file(content: &str) -> ParseResult {
        let mut file = NamedTempFile::with_suffix(".ts").unwrap();
        write!(file, "{}", content).unwrap();
        let parser = Parser::new();
        parser.parse(file.path()).unwrap()
    }

    #[test]
    fn test_param_with_type() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param {string} name - User name
// @acp:param {number} age - User age
"#;
        let result = parse_test_file(content);
        assert_eq!(result.symbols.len(), 1);

        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");
        assert_eq!(type_info.params.len(), 2);

        assert_eq!(type_info.params[0].name, "name");
        assert_eq!(type_info.params[0].r#type, Some("string".to_string()));
        assert_eq!(type_info.params[0].directive, Some("User name".to_string()));

        assert_eq!(type_info.params[1].name, "age");
        assert_eq!(type_info.params[1].r#type, Some("number".to_string()));
    }

    #[test]
    fn test_param_optional() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param {string} [name] - Optional name
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        assert_eq!(type_info.params[0].name, "name");
        assert!(type_info.params[0].optional);
    }

    #[test]
    fn test_param_with_default() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param {number} [limit=10] - Limit with default
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        assert_eq!(type_info.params[0].name, "limit");
        assert!(type_info.params[0].optional);
        assert_eq!(type_info.params[0].default, Some("10".to_string()));
    }

    #[test]
    fn test_param_without_type() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param name - Just a name param
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        assert_eq!(type_info.params[0].name, "name");
        assert!(type_info.params[0].r#type.is_none());
    }

    #[test]
    fn test_returns_with_type() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:returns {Promise<User>} - Returns user promise
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        let returns = type_info.returns.as_ref().expect("Should have returns");
        assert_eq!(returns.r#type, Some("Promise<User>".to_string()));
        assert_eq!(returns.directive, Some("Returns user promise".to_string()));
    }

    #[test]
    fn test_returns_without_type() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:returns - Returns nothing special
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        let returns = type_info.returns.as_ref().expect("Should have returns");
        assert!(returns.r#type.is_none());
    }

    #[test]
    fn test_template() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:template T - Type parameter
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        assert_eq!(type_info.type_params.len(), 1);
        assert_eq!(type_info.type_params[0].name, "T");
        assert!(type_info.type_params[0].constraint.is_none());
    }

    #[test]
    fn test_template_with_constraint() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:template T extends BaseEntity - Entity type
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        assert_eq!(type_info.type_params[0].name, "T");
        assert_eq!(type_info.type_params[0].constraint, Some("BaseEntity".to_string()));
        assert_eq!(type_info.type_params[0].directive, Some("Entity type".to_string()));
    }

    #[test]
    fn test_complex_type_expression() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param {Map<string, User | null>} userMap - Complex type
// @acp:returns {Promise<Array<User>>} - Returns users
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        assert_eq!(type_info.params[0].r#type, Some("Map<string, User | null>".to_string()));

        let returns = type_info.returns.as_ref().unwrap();
        assert_eq!(returns.r#type, Some("Promise<Array<User>>".to_string()));
    }

    #[test]
    fn test_backward_compat_no_types() {
        // Ensure old-style annotations without types still work
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param userId - User ID
// @acp:returns - User object or null
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        // Should still capture the param name and directive
        assert_eq!(type_info.params[0].name, "userId");
        assert!(type_info.params[0].r#type.is_none());
    }

    #[test]
    fn test_type_source_is_acp() {
        let content = r#"
// @acp:fn "test" - Test function
// @acp:param {string} name - Name param
// @acp:returns {void} - Returns nothing
"#;
        let result = parse_test_file(content);
        let sym = &result.symbols[0];
        let type_info = sym.type_info.as_ref().expect("Should have type_info");

        // When type is present, source should be Acp
        assert_eq!(type_info.params[0].type_source, Some(TypeSource::Acp));
        assert_eq!(type_info.returns.as_ref().unwrap().type_source, Some(TypeSource::Acp));
    }
}
