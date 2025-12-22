//! @acp:module "Go Doc Parser"
//! @acp:summary "Parses Go documentation comments and converts to ACP format"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Go Doc Parser
//!
//! Parses Go documentation comments in the standard godoc format:
//!
//! ## Comment Styles
//! - `//` line comments (most common)
//! - `/* */` block comments
//!
//! ## Conventions
//! - First sentence should start with the element name
//! - "Deprecated:" prefix marks deprecated items
//! - "BUG(who):" for known issues
//! - Code examples are indented by a tab
//! - Paragraphs separated by blank lines
//!
//! ## Features Detected
//! - Deprecation notices
//! - Bug annotations
//! - Code examples (indented blocks)
//! - Cross-references to other symbols

use std::sync::LazyLock;

use regex::Regex;

use super::{DocStandardParser, ParsedDocumentation};
use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Matches Deprecated: prefix"
static DEPRECATED_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Deprecated:\s*(.*)$").expect("Invalid deprecated prefix regex")
});

/// @acp:summary "Matches BUG(who): prefix"
static BUG_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^BUG\(([^)]+)\):\s*(.*)$").expect("Invalid bug prefix regex")
});

/// @acp:summary "Matches TODO(who): or FIXME(who): prefix"
static TODO_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(TODO|FIXME|XXX)(?:\(([^)]+)\))?:\s*(.*)$").expect("Invalid todo prefix regex")
});

/// @acp:summary "Matches See also: or See: references"
static SEE_ALSO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^See\s*(?:also)?:\s*(.+)$").expect("Invalid see also regex")
});

/// @acp:summary "Matches cross-reference to another symbol [Name] or [pkg.Name]"
static CROSS_REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([a-zA-Z_][a-zA-Z0-9_.]*)\]").expect("Invalid cross-ref regex")
});

/// @acp:summary "Go-specific extensions for doc comments"
#[derive(Debug, Clone, Default)]
pub struct GoDocExtensions {
    /// Whether this is package-level documentation
    pub is_package_doc: bool,

    /// Known bugs (who, description)
    pub bugs: Vec<(String, String)>,

    /// Whether the element is exported (starts with uppercase)
    pub is_exported: bool,

    /// Cross-references found in the documentation
    pub cross_refs: Vec<String>,

    /// Whether doc follows convention (starts with element name)
    pub follows_convention: bool,

    /// Code examples found (indented blocks)
    pub code_examples: Vec<String>,
}

/// @acp:summary "Parses Go doc comments"
/// @acp:lock normal
pub struct GodocParser {
    /// Go-specific extensions parsed from doc comments
    extensions: GoDocExtensions,

    /// The name of the element being documented (for convention check)
    element_name: Option<String>,
}

impl GodocParser {
    /// @acp:summary "Creates a new Godoc parser"
    pub fn new() -> Self {
        Self {
            extensions: GoDocExtensions::default(),
            element_name: None,
        }
    }

    /// @acp:summary "Creates a parser with known element name for convention checking"
    pub fn with_element_name(mut self, name: impl Into<String>) -> Self {
        self.element_name = Some(name.into());
        self
    }

    /// @acp:summary "Gets the parsed Go extensions"
    pub fn extensions(&self) -> &GoDocExtensions {
        &self.extensions
    }

    /// @acp:summary "Strips Go comment prefixes from lines"
    fn strip_comment_prefix(line: &str) -> &str {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("//") {
            // Handle "// " with space or just "//"
            rest.strip_prefix(' ').unwrap_or(rest)
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            rest.trim_start()
        } else if let Some(rest) = trimmed.strip_prefix("*/") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix('*') {
            // Handle "* " in block comments
            rest.trim_start()
        } else {
            trimmed
        }
    }

    /// @acp:summary "Checks if first sentence starts with element name (Go convention)"
    fn check_convention(&self, first_line: &str) -> bool {
        if let Some(name) = &self.element_name {
            first_line.starts_with(name)
        } else {
            // Can't check without element name
            true
        }
    }

    /// @acp:summary "Extracts the first sentence as summary"
    fn extract_summary(text: &str) -> String {
        // Find first sentence ending with period, question mark, or exclamation
        let mut summary = String::new();

        for line in text.lines() {
            let trimmed = line.trim();

            // Skip leading empty lines, but break on empty line after content
            if trimmed.is_empty() {
                if summary.is_empty() {
                    continue; // Skip leading empty lines
                } else {
                    break; // Empty line ends the summary paragraph
                }
            }

            // Add space between lines
            if !summary.is_empty() {
                summary.push(' ');
            }

            // Look for sentence-ending punctuation followed by whitespace or end
            for (i, c) in trimmed.char_indices() {
                if c == '.' || c == '!' || c == '?' {
                    let next_byte = i + c.len_utf8();
                    let rest = &trimmed[next_byte..];
                    // End of line or followed by whitespace = end of sentence
                    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                        summary.push_str(&trimmed[..next_byte]);
                        return summary;
                    }
                }
            }

            // No sentence end found, add whole line and continue
            summary.push_str(trimmed);
        }

        summary
    }

    /// @acp:summary "Extracts cross-references from text"
    fn extract_cross_refs(&self, text: &str) -> Vec<String> {
        CROSS_REF
            .captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// @acp:summary "Extracts code examples from text"
    fn extract_code_examples(&self, lines: &[&str]) -> Vec<String> {
        let mut examples = Vec::new();
        let mut current_example = Vec::new();
        let mut in_example = false;

        for line in lines {
            let stripped = Self::strip_comment_prefix(line);

            if stripped.starts_with('\t') || stripped.starts_with("    ") {
                // Indented line = code
                in_example = true;
                // Remove one level of indentation
                let code = stripped.strip_prefix('\t').unwrap_or_else(|| stripped.trim_start());
                current_example.push(code.to_string());
            } else if in_example {
                // End of code block
                if !current_example.is_empty() {
                    examples.push(current_example.join("\n"));
                    current_example.clear();
                }
                in_example = false;
            }
        }

        // Don't forget last example
        if !current_example.is_empty() {
            examples.push(current_example.join("\n"));
        }

        examples
    }
}

impl Default for GodocParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocStandardParser for GodocParser {
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let mut extensions = GoDocExtensions::default();

        let lines: Vec<&str> = raw_comment.lines().collect();
        let mut content_lines = Vec::new();
        let mut in_deprecated = false;
        let mut deprecated_text = Vec::new();

        for line in &lines {
            let stripped = Self::strip_comment_prefix(line);

            // Check for Deprecated: prefix
            if let Some(caps) = DEPRECATED_PREFIX.captures(stripped) {
                in_deprecated = true;
                let rest = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                if !rest.is_empty() {
                    deprecated_text.push(rest.to_string());
                }
                continue;
            }

            // Continue collecting deprecated text if in deprecated block
            if in_deprecated {
                if stripped.is_empty() {
                    // End of deprecated block
                    in_deprecated = false;
                } else {
                    deprecated_text.push(stripped.to_string());
                    continue;
                }
            }

            // Check for BUG(who): prefix
            if let Some(caps) = BUG_PREFIX.captures(stripped) {
                let who = caps.get(1).map(|m| m.as_str()).unwrap_or("unknown");
                let desc = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                extensions.bugs.push((who.to_string(), desc.to_string()));
                continue;
            }

            // Check for TODO/FIXME prefix
            if let Some(caps) = TODO_PREFIX.captures(stripped) {
                let desc = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                doc.todos.push(desc.to_string());
                continue;
            }

            // Check for See also: references
            if let Some(caps) = SEE_ALSO.captures(stripped) {
                let refs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                for r in refs.split(',') {
                    let r = r.trim();
                    if !r.is_empty() {
                        doc.see_refs.push(r.to_string());
                    }
                }
                continue;
            }

            content_lines.push(*line);
        }

        // Set deprecated if found
        if !deprecated_text.is_empty() {
            doc.deprecated = Some(deprecated_text.join(" "));
        }

        // Process content lines for summary and description
        let content: Vec<String> = content_lines
            .iter()
            .map(|l| Self::strip_comment_prefix(l).to_string())
            .collect();

        let full_text = content.join("\n");

        // Extract summary (first sentence)
        let summary = Self::extract_summary(&full_text);
        let has_summary = !summary.is_empty();
        if has_summary {
            doc.summary = Some(summary.clone());

            // Check convention (starts with element name)
            extensions.follows_convention = self.check_convention(&summary);
        }

        // Set description if there's more than summary
        let trimmed_text = full_text.trim();
        if trimmed_text.len() > summary.len() {
            doc.description = Some(trimmed_text.to_string());
        }

        // Extract cross-references
        extensions.cross_refs = self.extract_cross_refs(&full_text);
        for ref_name in &extensions.cross_refs {
            doc.see_refs.push(ref_name.clone());
        }

        // Extract code examples
        extensions.code_examples = self.extract_code_examples(&content_lines);
        for example in &extensions.code_examples {
            doc.examples.push(example.clone());
        }

        // Store extensions in custom tags
        if extensions.is_package_doc {
            doc.custom_tags.push(("package_doc".to_string(), "true".to_string()));
        }
        if !extensions.bugs.is_empty() {
            doc.custom_tags.push(("has_bugs".to_string(), "true".to_string()));
            for (who, desc) in &extensions.bugs {
                doc.notes.push(format!("BUG({}): {}", who, desc));
            }
        }
        // Only flag unconventional if we have a summary to check against
        if has_summary && !extensions.follows_convention {
            doc.custom_tags.push(("unconventional_doc".to_string(), "true".to_string()));
        }

        doc
    }

    fn standard_name(&self) -> &'static str {
        "godoc"
    }

    /// @acp:summary "Converts parsed Godoc to ACP suggestions with Go-specific handling"
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

        // Convert cross-references to @acp:ref
        for see_ref in &parsed.see_refs {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Ref,
                see_ref,
                SuggestionSource::Converted,
            ));
        }

        // Convert TODOs to @acp:hack
        for todo in &parsed.todos {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Hack,
                format!("reason=\"{}\"", todo),
                SuggestionSource::Converted,
            ));
        }

        // Go-specific: has bugs documented
        if parsed.custom_tags.iter().any(|(k, v)| k == "has_bugs" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "has documented bugs - review before use",
                SuggestionSource::Converted,
            ));
        }

        // Go-specific: unconventional documentation
        if parsed.custom_tags.iter().any(|(k, v)| k == "unconventional_doc" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "doc doesn't follow Go convention (should start with element name)",
                SuggestionSource::Converted,
            ));
        }

        // Go-specific: package documentation
        if parsed.custom_tags.iter().any(|(k, v)| k == "package_doc" && v == "true") {
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

        // Convert bug notes to AI hints
        for note in &parsed.notes {
            if note.starts_with("BUG(") {
                suggestions.push(Suggestion::ai_hint(
                    target,
                    line,
                    note,
                    SuggestionSource::Converted,
                ));
            }
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
    fn test_strip_comment_prefix() {
        assert_eq!(GodocParser::strip_comment_prefix("// Hello"), "Hello");
        assert_eq!(GodocParser::strip_comment_prefix("//Hello"), "Hello");
        assert_eq!(GodocParser::strip_comment_prefix("  // Hello"), "Hello");
    }

    #[test]
    fn test_parse_basic_godoc() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// NewParser creates a new parser instance.
// It initializes the parser with default settings.
"#);

        assert_eq!(doc.summary, Some("NewParser creates a new parser instance.".to_string()));
    }

    #[test]
    fn test_parse_with_deprecated() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// OldFunction does something.
//
// Deprecated: Use NewFunction instead.
"#);

        assert!(doc.deprecated.is_some());
        assert!(doc.deprecated.unwrap().contains("NewFunction"));
    }

    #[test]
    fn test_parse_with_bug() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Calculate computes a value.
//
// BUG(alice): Does not handle negative numbers correctly.
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "has_bugs" && v == "true"));
        assert!(doc.notes.iter().any(|n| n.contains("BUG(alice)")));
    }

    #[test]
    fn test_parse_with_todo() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Process handles the input.
// TODO(bob): Add error handling
"#);

        assert!(!doc.todos.is_empty());
        assert!(doc.todos[0].contains("Add error handling"));
    }

    #[test]
    fn test_parse_with_see_also() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Read reads data from the source.
// See also: Write, Close
"#);

        assert!(doc.see_refs.contains(&"Write".to_string()));
        assert!(doc.see_refs.contains(&"Close".to_string()));
    }

    #[test]
    fn test_parse_with_code_example() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Add adds two numbers.
// Example:
//	result := Add(2, 3)
//	fmt.Println(result) // Output: 5
"#);

        assert!(!doc.examples.is_empty());
        assert!(doc.examples[0].contains("Add(2, 3)"));
    }

    #[test]
    fn test_parse_with_cross_refs() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Parse parses input using [Config] and returns a [Result].
"#);

        assert!(doc.see_refs.contains(&"Config".to_string()));
        assert!(doc.see_refs.contains(&"Result".to_string()));
    }

    #[test]
    fn test_parse_multi_paragraph() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Handler processes HTTP requests.
//
// It validates the input, performs the operation,
// and returns an appropriate response.
"#);

        assert_eq!(doc.summary, Some("Handler processes HTTP requests.".to_string()));
        assert!(doc.description.is_some());
    }

    #[test]
    fn test_convention_check_pass() {
        let parser = GodocParser::new().with_element_name("NewParser");
        let doc = parser.parse(r#"
// NewParser creates a new parser.
"#);

        // Should follow convention (starts with element name)
        assert!(!doc.custom_tags.iter().any(|(k, _)| k == "unconventional_doc"));
    }

    #[test]
    fn test_convention_check_fail() {
        let parser = GodocParser::new().with_element_name("NewParser");
        let doc = parser.parse(r#"
// Creates a new parser instance.
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "unconventional_doc" && v == "true"));
    }

    #[test]
    fn test_block_comment() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
/*
Package utils provides utility functions.

It includes helpers for common operations.
*/
"#);

        assert!(doc.summary.is_some());
        assert!(doc.summary.unwrap().contains("Package utils"));
    }

    #[test]
    fn test_to_suggestions_basic() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// NewClient creates a new API client.
"#);

        let suggestions = parser.to_suggestions(&doc, "NewClient", 10);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Summary &&
            s.value.contains("creates a new API client")
        ));
    }

    #[test]
    fn test_to_suggestions_deprecated() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Old does something.
// Deprecated: Use New instead.
"#);

        let suggestions = parser.to_suggestions(&doc, "Old", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Deprecated
        ));
    }

    #[test]
    fn test_to_suggestions_bugs() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Calculate computes values.
// BUG(dev): Off by one error.
"#);

        let suggestions = parser.to_suggestions(&doc, "Calculate", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("bugs")
        ));
    }

    #[test]
    fn test_to_suggestions_refs() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Process uses [Config] to process data.
"#);

        let suggestions = parser.to_suggestions(&doc, "Process", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Ref &&
            s.value == "Config"
        ));
    }

    #[test]
    fn test_to_suggestions_todos() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Incomplete function.
// TODO: Finish implementation
"#);

        let suggestions = parser.to_suggestions(&doc, "Incomplete", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Hack
        ));
    }

    #[test]
    fn test_to_suggestions_examples() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// Add adds numbers.
//	sum := Add(1, 2)
"#);

        let suggestions = parser.to_suggestions(&doc, "Add", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("examples")
        ));
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
    fn test_extract_summary_single_sentence() {
        let summary = GodocParser::extract_summary("This is a simple summary.");
        assert_eq!(summary, "This is a simple summary.");
    }

    #[test]
    fn test_extract_summary_multi_sentence() {
        let summary = GodocParser::extract_summary("First sentence. Second sentence.");
        assert_eq!(summary, "First sentence.");
    }

    #[test]
    fn test_deprecated_multiline() {
        let parser = GodocParser::new();
        let doc = parser.parse(r#"
// OldAPI is deprecated.
//
// Deprecated: This API is deprecated and will be removed in v2.0.
// Use NewAPI instead for better performance.
"#);

        assert!(doc.deprecated.is_some());
        let deprecated = doc.deprecated.unwrap();
        assert!(deprecated.contains("v2.0"));
        assert!(deprecated.contains("NewAPI"));
    }
}
