//! @acp:module "Documentation Converters"
//! @acp:summary "Converts existing documentation standards to ACP format"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Documentation Converters
//!
//! Provides parsers and converters for various documentation standards:
//! - JSDoc/TSDoc (JavaScript/TypeScript)
//! - Python docstrings (Google, NumPy, Sphinx, plain)
//! - Rust doc comments
//! - Go doc comments
//! - Javadoc (Java)
//!
//! Each converter parses the raw documentation format into a structured
//! [`ParsedDocumentation`] and then converts it to ACP [`Suggestion`]s.

pub mod jsdoc;
pub mod docstring;
pub mod rustdoc;
pub mod godoc;
pub mod javadoc;

pub use jsdoc::{JsDocParser, TsDocParser, TsDocExtensions};
pub use docstring::DocstringParser;
pub use rustdoc::{RustdocParser, RustDocExtensions};
pub use godoc::{GodocParser, GoDocExtensions};
pub use javadoc::{JavadocParser, JavadocExtensions};

use crate::annotate::{Suggestion, SuggestionSource, AnnotationType};

/// @acp:summary "Parsed documentation from an existing standard"
/// Contains structured information extracted from any documentation format.
#[derive(Debug, Clone, Default)]
pub struct ParsedDocumentation {
    /// First line or @description content
    pub summary: Option<String>,

    /// Full description text
    pub description: Option<String>,

    /// Deprecation notice
    pub deprecated: Option<String>,

    /// @see/@link references
    pub see_refs: Vec<String>,

    /// @todo/@fixme items
    pub todos: Vec<String>,

    /// @param entries: (name, type, description)
    pub params: Vec<(String, Option<String>, Option<String>)>,

    /// @returns/@return entry: (type, description)
    pub returns: Option<(Option<String>, Option<String>)>,

    /// @throws/@raises/@exception entries: (type, description)
    pub throws: Vec<(String, Option<String>)>,

    /// @example blocks
    pub examples: Vec<String>,

    /// @since version
    pub since: Option<String>,

    /// @author
    pub author: Option<String>,

    /// Custom tags: (tag_name, value)
    pub custom_tags: Vec<(String, String)>,

    /// Any warnings or notes
    pub notes: Vec<String>,
}

impl ParsedDocumentation {
    /// @acp:summary "Creates an empty parsed documentation"
    pub fn new() -> Self {
        Self::default()
    }

    /// @acp:summary "Checks if this documentation has any content"
    pub fn is_empty(&self) -> bool {
        self.summary.is_none()
            && self.description.is_none()
            && self.deprecated.is_none()
            && self.see_refs.is_empty()
            && self.todos.is_empty()
            && self.params.is_empty()
            && self.returns.is_none()
            && self.throws.is_empty()
            && self.examples.is_empty()
            && self.notes.is_empty()
            && self.custom_tags.is_empty()
    }

    /// @acp:summary "Gets the visibility modifier from custom tags"
    pub fn get_visibility(&self) -> Option<&str> {
        self.custom_tags
            .iter()
            .find(|(k, _)| k == "visibility")
            .map(|(_, v)| v.as_str())
    }

    /// @acp:summary "Gets the module name from custom tags"
    pub fn get_module(&self) -> Option<&str> {
        self.custom_tags
            .iter()
            .find(|(k, _)| k == "module")
            .map(|(_, v)| v.as_str())
    }

    /// @acp:summary "Gets the category from custom tags"
    pub fn get_category(&self) -> Option<&str> {
        self.custom_tags
            .iter()
            .find(|(k, _)| k == "category")
            .map(|(_, v)| v.as_str())
    }

    /// @acp:summary "Checks if marked as readonly"
    pub fn is_readonly(&self) -> bool {
        self.custom_tags
            .iter()
            .any(|(k, v)| k == "readonly" && v == "true")
    }
}

/// @acp:summary "Trait for parsing language-specific doc standards"
pub trait DocStandardParser: Send + Sync {
    /// @acp:summary "Parses a raw doc comment into structured documentation"
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation;

    /// @acp:summary "Gets the standard name"
    fn standard_name(&self) -> &'static str;

    /// @acp:summary "Converts parsed documentation to ACP suggestions"
    ///
    /// Default implementation converts common fields to suggestions.
    /// Override for standard-specific behavior.
    fn to_suggestions(
        &self,
        parsed: &ParsedDocumentation,
        target: &str,
        line: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Convert summary
        if let Some(summary) = &parsed.summary {
            let truncated = truncate_summary(summary, 100);
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

        // Convert @see references to @acp:ref
        for see_ref in &parsed.see_refs {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Ref,
                see_ref,
                SuggestionSource::Converted,
            ));
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

        // Convert visibility to lock level
        if let Some(visibility) = parsed.get_visibility() {
            let lock_level = match visibility {
                "private" | "internal" => "restricted",
                "protected" => "normal",
                _ => "normal",
            };
            suggestions.push(Suggestion::lock(
                target,
                line,
                lock_level,
                SuggestionSource::Converted,
            ));
        }

        // Convert @module
        if let Some(module_name) = parsed.get_module() {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Module,
                module_name,
                SuggestionSource::Converted,
            ));
        }

        // Convert @category to domain
        if let Some(category) = parsed.get_category() {
            suggestions.push(Suggestion::domain(
                target,
                line,
                category.to_lowercase(),
                SuggestionSource::Converted,
            ));
        }

        // Convert throws to AI hint
        if !parsed.throws.is_empty() {
            let throws_list: Vec<String> = parsed
                .throws
                .iter()
                .map(|(t, _)| t.clone())
                .collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("throws {}", throws_list.join(", ")),
                SuggestionSource::Converted,
            ));
        }

        // Convert readonly to AI hint
        if parsed.is_readonly() {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "readonly",
                SuggestionSource::Converted,
            ));
        }

        // Convert examples existence to AI hint
        if !parsed.examples.is_empty() {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "has examples",
                SuggestionSource::Converted,
            ));
        }

        // Convert notes/warnings to AI hints
        for note in &parsed.notes {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                note,
                SuggestionSource::Converted,
            ));
        }

        suggestions
    }
}

/// @acp:summary "Truncates a summary to the specified length"
fn truncate_summary(summary: &str, max_len: usize) -> String {
    let trimmed = summary.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        // Find the last space before max_len to avoid cutting words
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
    fn test_parsed_documentation_is_empty() {
        let empty = ParsedDocumentation::new();
        assert!(empty.is_empty());

        let mut with_summary = ParsedDocumentation::new();
        with_summary.summary = Some("Test".to_string());
        assert!(!with_summary.is_empty());
    }

    #[test]
    fn test_truncate_summary() {
        assert_eq!(truncate_summary("Short", 100), "Short");
        assert_eq!(
            truncate_summary("This is a very long summary that needs to be truncated", 20),
            "This is a very long..."
        );
    }

    #[test]
    fn test_parsed_documentation_getters() {
        let mut doc = ParsedDocumentation::new();
        doc.custom_tags.push(("visibility".to_string(), "private".to_string()));
        doc.custom_tags.push(("module".to_string(), "TestModule".to_string()));
        doc.custom_tags.push(("category".to_string(), "Security".to_string()));
        doc.custom_tags.push(("readonly".to_string(), "true".to_string()));

        assert_eq!(doc.get_visibility(), Some("private"));
        assert_eq!(doc.get_module(), Some("TestModule"));
        assert_eq!(doc.get_category(), Some("Security"));
        assert!(doc.is_readonly());
    }
}
