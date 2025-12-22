//! @acp:module "Rust Doc Parser"
//! @acp:summary "Parses Rust doc comments and converts to ACP format"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Rust Doc Parser
//!
//! Parses Rust documentation comments in standard format:
//!
//! ## Comment Styles
//! - `///` - Outer documentation (items)
//! - `//!` - Inner documentation (modules/crates)
//!
//! ## Standard Sections
//! - `# Examples` - Code examples
//! - `# Arguments` - Function parameters
//! - `# Returns` - Return value description
//! - `# Panics` - Panic conditions
//! - `# Errors` - Error conditions (for Result returns)
//! - `# Safety` - Safety requirements for unsafe code
//! - `# Type Parameters` - Generic type parameters
//!
//! ## Features Detected
//! - Intra-doc links: `[Type]`, `[method](Type::method)`
//! - Code blocks with language hints
//! - Deprecated markers
//! - Must-use hints

use std::sync::LazyLock;

use regex::Regex;

use super::{DocStandardParser, ParsedDocumentation};
use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Matches Markdown section headers (# Section)"
static SECTION_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^#\s+(Examples?|Arguments?|Parameters?|Returns?|Panics?|Errors?|Safety|Type\s+Parameters?|See\s+Also|Notes?|Warnings?)\s*$")
        .expect("Invalid section header regex")
});

/// @acp:summary "Matches intra-doc links [Name] or [`Name`]"
static INTRA_DOC_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[`?([a-zA-Z_][a-zA-Z0-9_:]*)`?\](?:\([^)]+\))?")
        .expect("Invalid intra-doc link regex")
});

/// @acp:summary "Matches code blocks ```rust or ```"
static CODE_BLOCK_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^```(\w*)?\s*$").expect("Invalid code block regex")
});

/// @acp:summary "Matches argument lines (name - description or * `name` - description)"
static ARG_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\*?\s*`?([a-zA-Z_][a-zA-Z0-9_]*)`?\s*[-:]?\s*(.*)$")
        .expect("Invalid argument line regex")
});

/// @acp:summary "Rust-specific extensions for doc comments"
#[derive(Debug, Clone, Default)]
pub struct RustDocExtensions {
    /// Whether this is module-level documentation (//!)
    pub is_module_doc: bool,

    /// Whether the item is marked unsafe
    pub is_unsafe: bool,

    /// Whether the item returns a Result type
    pub returns_result: bool,

    /// Whether the item is async
    pub is_async: bool,

    /// Panic conditions
    pub panics: Vec<String>,

    /// Error conditions (for Result types)
    pub errors: Vec<String>,

    /// Safety requirements (for unsafe code)
    pub safety: Vec<String>,

    /// Type parameters with descriptions
    pub type_params: Vec<(String, Option<String>)>,

    /// Intra-doc links found
    pub doc_links: Vec<String>,

    /// Whether marked with #[must_use]
    pub must_use: Option<String>,

    /// Deprecation info (since, note)
    pub deprecated_since: Option<String>,

    /// Feature gate if any
    pub feature_gate: Option<String>,
}

/// @acp:summary "Parses Rust doc comments"
/// @acp:lock normal
pub struct RustdocParser {
    /// Rust-specific extensions parsed from doc comments
    extensions: RustDocExtensions,
}

impl RustdocParser {
    /// @acp:summary "Creates a new Rustdoc parser"
    pub fn new() -> Self {
        Self {
            extensions: RustDocExtensions::default(),
        }
    }

    /// @acp:summary "Gets the parsed Rust extensions"
    pub fn extensions(&self) -> &RustDocExtensions {
        &self.extensions
    }

    /// @acp:summary "Checks if this is module-level documentation"
    fn is_module_doc(raw: &str) -> bool {
        // Check if the raw comment uses //! style
        raw.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("//!")
        })
    }

    /// @acp:summary "Strips doc comment prefixes from lines"
    fn strip_doc_prefix(line: &str) -> &str {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            rest.trim_start()
        } else if let Some(rest) = trimmed.strip_prefix("//!") {
            rest.trim_start()
        } else if let Some(rest) = trimmed.strip_prefix("*") {
            // Handle /** ... */ style if used
            rest.trim_start()
        } else {
            trimmed
        }
    }

    /// @acp:summary "Parses section content into structured data"
    fn parse_section_content(
        &self,
        section: &str,
        content: &[String],
        doc: &mut ParsedDocumentation,
        extensions: &mut RustDocExtensions,
    ) {
        let text = content.join("\n").trim().to_string();
        if text.is_empty() {
            return;
        }

        match section.to_lowercase().as_str() {
            "arguments" | "argument" | "parameters" | "parameter" => {
                // Parse argument entries
                for param in self.parse_arguments(&text) {
                    doc.params.push(param);
                }
            }
            "returns" | "return" => {
                doc.returns = Some((None, Some(text)));
            }
            "panics" | "panic" => {
                extensions.panics.push(text.clone());
                doc.notes.push(format!("Panics: {}", text));
            }
            "errors" | "error" => {
                extensions.errors.push(text.clone());
                extensions.returns_result = true;
                doc.notes.push(format!("Errors: {}", text));
            }
            "safety" => {
                extensions.is_unsafe = true;
                extensions.safety.push(text.clone());
                doc.notes.push(format!("Safety: {}", text));
            }
            "type parameters" | "type parameter" => {
                // Parse type parameter entries
                for (name, desc) in self.parse_type_params(&text) {
                    extensions.type_params.push((name, desc));
                }
            }
            "examples" | "example" => {
                doc.examples.push(text);
            }
            "see also" => {
                for ref_line in text.lines() {
                    let ref_line = ref_line.trim();
                    if !ref_line.is_empty() {
                        doc.see_refs.push(ref_line.to_string());
                    }
                }
            }
            "notes" | "note" => {
                doc.notes.push(text);
            }
            "warnings" | "warning" => {
                doc.notes.push(format!("Warning: {}", text));
            }
            _ => {}
        }
    }

    /// @acp:summary "Parses argument entries from text"
    fn parse_arguments(&self, text: &str) -> Vec<(String, Option<String>, Option<String>)> {
        let mut args = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_desc = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Check for list item (- or *)
            let content = if let Some(rest) = trimmed.strip_prefix('-') {
                rest.trim()
            } else if let Some(rest) = trimmed.strip_prefix('*') {
                rest.trim()
            } else {
                trimmed
            };

            // Try to match argument pattern
            if let Some(caps) = ARG_LINE.captures(content) {
                // Save previous argument
                if let Some(name) = current_name.take() {
                    let desc = if current_desc.is_empty() {
                        None
                    } else {
                        Some(current_desc.join(" "))
                    };
                    args.push((name, None, desc));
                    current_desc.clear();
                }

                let name = caps.get(1).unwrap().as_str().to_string();
                let desc = caps.get(2).map(|m| m.as_str().trim().to_string());
                current_name = Some(name);
                if let Some(d) = desc {
                    if !d.is_empty() {
                        current_desc.push(d);
                    }
                }
            } else if current_name.is_some() && (line.starts_with("  ") || line.starts_with("\t")) {
                // Continuation of description
                current_desc.push(trimmed.to_string());
            }
        }

        // Save last argument
        if let Some(name) = current_name {
            let desc = if current_desc.is_empty() {
                None
            } else {
                Some(current_desc.join(" "))
            };
            args.push((name, None, desc));
        }

        args
    }

    /// @acp:summary "Parses type parameter entries"
    fn parse_type_params(&self, text: &str) -> Vec<(String, Option<String>)> {
        let mut params = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Remove list markers
            let content = if let Some(rest) = trimmed.strip_prefix('-') {
                rest.trim()
            } else if let Some(rest) = trimmed.strip_prefix('*') {
                rest.trim()
            } else {
                trimmed
            };

            // Parse "T - description" or "`T` - description"
            if let Some(caps) = ARG_LINE.captures(content) {
                let name = caps.get(1).unwrap().as_str().to_string();
                let desc = caps.get(2).map(|m| m.as_str().trim().to_string());
                params.push((name, desc));
            }
        }

        params
    }

    /// @acp:summary "Extracts intra-doc links from text"
    fn extract_doc_links(&self, text: &str) -> Vec<String> {
        INTRA_DOC_LINK
            .captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// @acp:summary "Checks if the doc contains code examples"
    fn has_code_examples(&self, raw: &str) -> bool {
        let mut in_code_block = false;
        for line in raw.lines() {
            let stripped = Self::strip_doc_prefix(line);
            if stripped.starts_with("```") {
                in_code_block = !in_code_block;
            }
        }
        raw.contains("```")
    }
}

impl Default for RustdocParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocStandardParser for RustdocParser {
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let mut extensions = RustDocExtensions {
            is_module_doc: Self::is_module_doc(raw_comment),
            ..Default::default()
        };

        let mut summary_lines = Vec::new();
        let mut current_section: Option<String> = None;
        let mut section_content = Vec::new();
        let mut in_code_block = false;
        let mut first_paragraph_done = false;

        for line in raw_comment.lines() {
            let stripped = Self::strip_doc_prefix(line);

            // Track code blocks (don't parse inside them)
            if CODE_BLOCK_START.is_match(stripped) {
                in_code_block = !in_code_block;
                if current_section.is_some() {
                    section_content.push(stripped.to_string());
                }
                continue;
            }

            if in_code_block {
                if current_section.is_some() {
                    section_content.push(stripped.to_string());
                }
                continue;
            }

            // Check for section header
            if let Some(caps) = SECTION_HEADER.captures(stripped) {
                // Save previous section
                if let Some(ref section) = current_section {
                    self.parse_section_content(section, &section_content, &mut doc, &mut extensions);
                }
                section_content.clear();

                current_section = Some(caps.get(1).unwrap().as_str().to_string());
                first_paragraph_done = true;
            } else if current_section.is_some() {
                section_content.push(stripped.to_string());
            } else if stripped.is_empty() {
                // Empty line ends first paragraph (summary)
                if !summary_lines.is_empty() {
                    first_paragraph_done = true;
                }
            } else if !first_paragraph_done {
                summary_lines.push(stripped.to_string());
            }
        }

        // Save last section
        if let Some(ref section) = current_section {
            self.parse_section_content(section, &section_content, &mut doc, &mut extensions);
        }

        // Set summary and description
        if !summary_lines.is_empty() {
            doc.summary = Some(summary_lines[0].clone());
            if summary_lines.len() > 1 {
                doc.description = Some(summary_lines.join(" "));
            }
        }

        // Extract intra-doc links
        extensions.doc_links = self.extract_doc_links(raw_comment);

        // Check for code examples
        if self.has_code_examples(raw_comment) && doc.examples.is_empty() {
            doc.examples.push("Has code examples".to_string());
        }

        // Store extensions info in custom tags for later use
        if extensions.is_module_doc {
            doc.custom_tags.push(("module_doc".to_string(), "true".to_string()));
        }
        if extensions.is_unsafe {
            doc.custom_tags.push(("unsafe".to_string(), "true".to_string()));
        }
        if extensions.returns_result {
            doc.custom_tags.push(("returns_result".to_string(), "true".to_string()));
        }
        if !extensions.panics.is_empty() {
            doc.custom_tags.push(("has_panics".to_string(), "true".to_string()));
        }
        if !extensions.safety.is_empty() {
            doc.custom_tags.push(("has_safety".to_string(), "true".to_string()));
        }
        for link in &extensions.doc_links {
            doc.see_refs.push(link.clone());
        }

        doc
    }

    fn standard_name(&self) -> &'static str {
        "rustdoc"
    }

    /// @acp:summary "Converts parsed Rustdoc to ACP suggestions with Rust-specific handling"
    fn to_suggestions(
        &self,
        parsed: &ParsedDocumentation,
        target: &str,
        line: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Convert summary (truncated)
        if let Some(summary) = &parsed.summary {
            let truncated = truncate_for_summary(summary, 100);
            suggestions.push(Suggestion::summary(
                target,
                line,
                truncated,
                SuggestionSource::Converted,
            ));
        }

        // Convert deprecated
        if let Some(msg) = &parsed.deprecated {
            suggestions.push(Suggestion::deprecated(
                target,
                line,
                msg,
                SuggestionSource::Converted,
            ));
        }

        // Convert intra-doc links to @acp:ref
        for see_ref in &parsed.see_refs {
            // Filter out common false positives
            if !["self", "Self", "crate", "super"].contains(&see_ref.as_str()) {
                suggestions.push(Suggestion::new(
                    target,
                    line,
                    AnnotationType::Ref,
                    see_ref,
                    SuggestionSource::Converted,
                ));
            }
        }

        // Convert @todo to @acp:hack
        for todo in &parsed.todos {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Hack,
                format!("reason=\"{}\"", todo),
                SuggestionSource::Converted,
            ));
        }

        // Rust-specific: unsafe code hint
        if parsed.custom_tags.iter().any(|(k, v)| k == "unsafe" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "unsafe code - review safety requirements",
                SuggestionSource::Converted,
            ));
        }

        // Rust-specific: has safety section
        if parsed.custom_tags.iter().any(|(k, v)| k == "has_safety" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "has safety requirements documented",
                SuggestionSource::Converted,
            ));
        }

        // Rust-specific: has panic conditions
        if parsed.custom_tags.iter().any(|(k, v)| k == "has_panics" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "may panic - see Panics section",
                SuggestionSource::Converted,
            ));
        }

        // Rust-specific: returns Result
        if parsed.custom_tags.iter().any(|(k, v)| k == "returns_result" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "returns Result type with documented errors",
                SuggestionSource::Converted,
            ));
        }

        // Rust-specific: module-level docs
        if parsed.custom_tags.iter().any(|(k, v)| k == "module_doc" && v == "true") {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Module,
                parsed.summary.as_deref().unwrap_or(target),
                SuggestionSource::Converted,
            ));
        }

        // Convert examples existence to AI hint
        if !parsed.examples.is_empty() {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "has documented examples",
                SuggestionSource::Converted,
            ));
        }

        // Convert notes to AI hints
        for note in &parsed.notes {
            // Truncate long notes
            let truncated = if note.len() > 80 {
                format!("{}...", &note[..77])
            } else {
                note.clone()
            };
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                truncated,
                SuggestionSource::Converted,
            ));
        }

        suggestions
    }
}

/// @acp:summary "Truncates a string to the specified length for summary use"
fn truncate_for_summary(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        let truncate_at = trimmed[..max_len]
            .rfind(' ')
            .unwrap_or(max_len);
        format!("{}...", &trimmed[..truncate_at])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_module_doc() {
        assert!(RustdocParser::is_module_doc("//! Module docs"));
        assert!(!RustdocParser::is_module_doc("/// Item docs"));
    }

    #[test]
    fn test_strip_doc_prefix() {
        assert_eq!(RustdocParser::strip_doc_prefix("/// Hello"), "Hello");
        assert_eq!(RustdocParser::strip_doc_prefix("//! World"), "World");
        assert_eq!(RustdocParser::strip_doc_prefix("///  Spaced"), "Spaced");
    }

    #[test]
    fn test_parse_basic_rustdoc() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Creates a new instance of the parser.
///
/// This function initializes the parser with default settings.
"#);

        assert_eq!(doc.summary, Some("Creates a new instance of the parser.".to_string()));
    }

    #[test]
    fn test_parse_with_arguments_section() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Calculates the sum of two numbers.
///
/// # Arguments
///
/// * `a` - The first number
/// * `b` - The second number
///
/// # Returns
///
/// The sum of a and b
"#);

        assert_eq!(doc.summary, Some("Calculates the sum of two numbers.".to_string()));
        assert_eq!(doc.params.len(), 2);
        assert_eq!(doc.params[0].0, "a");
        assert_eq!(doc.params[1].0, "b");
        assert!(doc.returns.is_some());
    }

    #[test]
    fn test_parse_with_panics_section() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Divides two numbers.
///
/// # Panics
///
/// Panics if the divisor is zero.
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "has_panics" && v == "true"));
        assert!(doc.notes.iter().any(|n| n.contains("Panics")));
    }

    #[test]
    fn test_parse_with_errors_section() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Opens a file.
///
/// # Errors
///
/// Returns an error if the file does not exist.
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "returns_result" && v == "true"));
        assert!(doc.notes.iter().any(|n| n.contains("Errors")));
    }

    #[test]
    fn test_parse_with_safety_section() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Dereferences a raw pointer.
///
/// # Safety
///
/// The pointer must be valid and properly aligned.
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "unsafe" && v == "true"));
        assert!(doc.custom_tags.iter().any(|(k, v)| k == "has_safety" && v == "true"));
    }

    #[test]
    fn test_parse_with_examples() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Adds two numbers.
///
/// # Examples
///
/// ```rust
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
"#);

        assert!(!doc.examples.is_empty());
    }

    #[test]
    fn test_parse_module_level_docs() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
//! This module provides utility functions.
//!
//! It includes helpers for parsing and formatting.
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "module_doc" && v == "true"));
        assert_eq!(doc.summary, Some("This module provides utility functions.".to_string()));
    }

    #[test]
    fn test_parse_intra_doc_links() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// See [`Parser`] and [`Config::new`] for more details.
"#);

        assert!(doc.see_refs.contains(&"Parser".to_string()));
        assert!(doc.see_refs.contains(&"Config::new".to_string()));
    }

    #[test]
    fn test_parse_type_parameters() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// A generic container.
///
/// # Type Parameters
///
/// * `T` - The type of elements stored
/// * `E` - The error type
"#);

        assert_eq!(doc.summary, Some("A generic container.".to_string()));
    }

    #[test]
    fn test_to_suggestions_basic() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Creates a new parser instance.
"#);

        let suggestions = parser.to_suggestions(&doc, "new", 10);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Summary &&
            s.value.contains("Creates a new parser")
        ));
    }

    #[test]
    fn test_to_suggestions_unsafe() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Dereferences a raw pointer.
///
/// # Safety
///
/// The pointer must be valid.
"#);

        let suggestions = parser.to_suggestions(&doc, "deref", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("unsafe")
        ));
    }

    #[test]
    fn test_to_suggestions_panics() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Divides two numbers.
///
/// # Panics
///
/// Panics if divisor is zero.
"#);

        let suggestions = parser.to_suggestions(&doc, "divide", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("panic")
        ));
    }

    #[test]
    fn test_to_suggestions_result() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Opens a file.
///
/// # Errors
///
/// Returns error if file not found.
"#);

        let suggestions = parser.to_suggestions(&doc, "open_file", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("Result")
        ));
    }

    #[test]
    fn test_to_suggestions_module_doc() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
//! Parser utilities module.
"#);

        let suggestions = parser.to_suggestions(&doc, "parser", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Module
        ));
    }

    #[test]
    fn test_to_suggestions_refs() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// See [`Config`] for configuration options.
"#);

        let suggestions = parser.to_suggestions(&doc, "func", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Ref &&
            s.value == "Config"
        ));
    }

    #[test]
    fn test_code_block_not_parsed_as_section() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Example function.
///
/// # Examples
///
/// ```rust
/// // # This is a comment, not a section
/// let x = 5;
/// ```
"#);

        // The "# This is a comment" inside the code block should not create a new section
        assert!(!doc.examples.is_empty());
    }

    #[test]
    fn test_truncate_for_summary() {
        assert_eq!(truncate_for_summary("Short", 100), "Short");
        assert_eq!(
            truncate_for_summary("This is a very long summary that needs truncation", 20),
            "This is a very long..."
        );
    }

    #[test]
    fn test_see_also_section() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Main function.
///
/// # See Also
///
/// - `other_function`
/// - `related_module::helper`
"#);

        assert!(doc.see_refs.len() >= 2);
    }

    #[test]
    fn test_multiple_sections() {
        let parser = RustdocParser::new();
        let doc = parser.parse(r#"
/// Processes input data.
///
/// # Arguments
///
/// * `input` - The input data
///
/// # Returns
///
/// The processed result
///
/// # Panics
///
/// Panics on invalid input
///
/// # Examples
///
/// ```
/// let result = process("test");
/// ```
"#);

        assert_eq!(doc.params.len(), 1);
        assert!(doc.returns.is_some());
        assert!(doc.custom_tags.iter().any(|(k, _)| k == "has_panics"));
        assert!(!doc.examples.is_empty());
    }
}
