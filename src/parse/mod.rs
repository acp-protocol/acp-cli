//! @acp:module "Parser"
//! @acp:summary "Source code parsing and annotation extraction (RFC-001 compliant)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Parses source files to extract symbols, calls, and documentation.
//! Supports RFC-001 self-documenting annotations with directive extraction.
//! Currently uses regex-based parsing with tree-sitter support planned.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cache::{FileEntry, InlineAnnotation, SymbolEntry, SymbolType, Visibility};
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
                "todo" | "fixme" | "critical" | "perf" => {
                    inline_annotations.push(InlineAnnotation {
                        line: ann.line,
                        annotation_type: ann.name.clone(),
                        value: ann.value.clone(),
                        directive: ann.directive.clone().unwrap_or_else(|| {
                            match ann.name.as_str() {
                                "todo" => "Pending work item".to_string(),
                                "fixme" => "Known issue requiring fix".to_string(),
                                "critical" => "Critical section - extra review required".to_string(),
                                "perf" => "Performance-sensitive code".to_string(),
                                _ => "".to_string(),
                            }
                        }),
                        expires: None,
                        ticket: None,
                        auto_generated: ann.auto_generated,
                    });
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
            async_fn: false,
            visibility: Visibility::Public,
            calls: self.calls,
            called_by: vec![], // Populated later by indexer
            git: None,
            constraints: None,
        }
    }
}
