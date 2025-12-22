//! @acp:module "Javadoc Parser"
//! @acp:summary "Parses Java documentation comments and converts to ACP format"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Javadoc Parser
//!
//! Parses Java documentation comments in the standard Javadoc format:
//!
//! ## Comment Style
//! - `/** ... */` block comments with leading asterisks
//!
//! ## Standard Tags
//! - `@param name description` - Parameter documentation
//! - `@return description` - Return value documentation
//! - `@throws/@exception type description` - Exception documentation
//! - `@author name` - Author information
//! - `@since version` - Version when added
//! - `@see reference` - Cross-reference
//! - `@deprecated description` - Deprecation notice
//! - `@version version` - Version information
//!
//! ## Inline Tags
//! - `{@link reference}` - Inline cross-reference
//! - `{@code text}` - Code formatting
//! - `{@inheritDoc}` - Inherit documentation
//!
//! ## Features Detected
//! - HTML stripping from descriptions
//! - First sentence extraction for summary
//! - Package/class/method level detection

use std::sync::LazyLock;

use regex::Regex;

use super::{DocStandardParser, ParsedDocumentation};
use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Matches @param tag"
static PARAM_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@param\s+(\w+)\s+(.*)$").expect("Invalid param tag regex")
});

/// @acp:summary "Matches @return/@returns tag"
static RETURN_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@returns?\s+(.*)$").expect("Invalid return tag regex")
});

/// @acp:summary "Matches @throws/@exception tag"
static THROWS_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@(?:throws|exception)\s+(\S+)\s*(.*)$").expect("Invalid throws tag regex")
});

/// @acp:summary "Matches @author tag"
static AUTHOR_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@author\s+(.+)$").expect("Invalid author tag regex")
});

/// @acp:summary "Matches @since tag"
static SINCE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@since\s+(.+)$").expect("Invalid since tag regex")
});

/// @acp:summary "Matches @version tag"
static VERSION_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@version\s+(.+)$").expect("Invalid version tag regex")
});

/// @acp:summary "Matches @see tag"
static SEE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@see\s+(.+)$").expect("Invalid see tag regex")
});

/// @acp:summary "Matches {@link reference} inline tag"
static LINK_INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{@link\s+([^}]+)\}").expect("Invalid link inline regex")
});

/// @acp:summary "Matches {@code text} inline tag"
static CODE_INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{@code\s+([^}]+)\}").expect("Invalid code inline regex")
});

/// @acp:summary "Matches {@inheritDoc} inline tag"
static INHERIT_DOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{@inheritDoc\}").expect("Invalid inheritDoc regex")
});

/// @acp:summary "Matches HTML tags for stripping"
static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<[^>]+>").expect("Invalid HTML tag regex")
});

/// @acp:summary "Matches @code block (pre tags with code)"
static CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<pre>\s*(?:<code>)?([\s\S]*?)(?:</code>)?\s*</pre>").expect("Invalid code block regex")
});

/// @acp:summary "Java-specific extensions for Javadoc comments"
#[derive(Debug, Clone, Default)]
pub struct JavadocExtensions {
    /// Whether this is package-level documentation (package-info.java)
    pub is_package_doc: bool,

    /// Whether this is class/interface level documentation
    pub is_type_doc: bool,

    /// Version information from @version
    pub version: Option<String>,

    /// Authors from @author tags
    pub authors: Vec<String>,

    /// Inherited documentation marker
    pub inherits_doc: bool,

    /// Code examples found in <pre> blocks
    pub code_examples: Vec<String>,

    /// Inline {@link} references extracted
    pub inline_links: Vec<String>,
}

/// @acp:summary "Parses Javadoc comments"
/// @acp:lock normal
pub struct JavadocParser {
    /// Java-specific extensions parsed from doc comments
    extensions: JavadocExtensions,
}

impl JavadocParser {
    /// @acp:summary "Creates a new Javadoc parser"
    pub fn new() -> Self {
        Self {
            extensions: JavadocExtensions::default(),
        }
    }

    /// @acp:summary "Creates a parser marked as package documentation"
    pub fn for_package() -> Self {
        Self {
            extensions: JavadocExtensions {
                is_package_doc: true,
                ..Default::default()
            },
        }
    }

    /// @acp:summary "Creates a parser marked as type documentation"
    pub fn for_type() -> Self {
        Self {
            extensions: JavadocExtensions {
                is_type_doc: true,
                ..Default::default()
            },
        }
    }

    /// @acp:summary "Gets the parsed Java extensions"
    pub fn extensions(&self) -> &JavadocExtensions {
        &self.extensions
    }

    /// @acp:summary "Strips Javadoc comment markers from lines"
    fn strip_comment_markers(line: &str) -> &str {
        let trimmed = line.trim();

        // Handle opening /**
        if let Some(rest) = trimmed.strip_prefix("/**") {
            return rest.trim();
        }

        // Handle closing */
        if let Some(rest) = trimmed.strip_suffix("*/") {
            let rest = rest.trim();
            // Also strip leading * if present
            if let Some(rest) = rest.strip_prefix('*') {
                return rest.trim_start();
            }
            return rest;
        }

        // Handle middle lines with leading *
        if let Some(rest) = trimmed.strip_prefix('*') {
            // Don't strip if it's part of closing */
            if !rest.starts_with('/') {
                return rest.trim_start();
            }
        }

        trimmed
    }

    /// @acp:summary "Strips HTML tags from text"
    fn strip_html(text: &str) -> String {
        HTML_TAG.replace_all(text, "").to_string()
    }

    /// @acp:summary "Processes inline tags like {@link} and {@code}"
    fn process_inline_tags(text: &str) -> (String, Vec<String>) {
        let mut links = Vec::new();

        // Extract links
        for caps in LINK_INLINE.captures_iter(text) {
            if let Some(link) = caps.get(1) {
                links.push(link.as_str().trim().to_string());
            }
        }

        // Replace inline tags with their content
        let processed = LINK_INLINE.replace_all(text, "$1");
        let processed = CODE_INLINE.replace_all(&processed, "`$1`");
        let processed = INHERIT_DOC.replace_all(&processed, "[inherited]");

        (processed.to_string(), links)
    }

    /// @acp:summary "Extracts first sentence as summary"
    fn extract_summary(text: &str) -> String {
        let mut summary = String::new();

        for line in text.lines() {
            let trimmed = line.trim();

            // Skip leading empty lines
            if trimmed.is_empty() {
                if summary.is_empty() {
                    continue;
                } else {
                    break;
                }
            }

            // Add space between lines
            if !summary.is_empty() {
                summary.push(' ');
            }

            // Look for sentence-ending punctuation
            for (i, c) in trimmed.char_indices() {
                if c == '.' || c == '!' || c == '?' {
                    let next_byte = i + c.len_utf8();
                    let rest = &trimmed[next_byte..];
                    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                        summary.push_str(&trimmed[..next_byte]);
                        return Self::strip_html(&summary);
                    }
                }
            }

            summary.push_str(trimmed);
        }

        Self::strip_html(&summary)
    }

    /// @acp:summary "Extracts code examples from <pre> blocks"
    fn extract_code_examples(text: &str) -> Vec<String> {
        CODE_BLOCK
            .captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect()
    }
}

impl Default for JavadocParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocStandardParser for JavadocParser {
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let mut extensions = self.extensions.clone();

        let lines: Vec<&str> = raw_comment.lines().collect();
        let mut content_lines = Vec::new();
        let mut current_tag: Option<String> = None;
        let mut tag_content = String::new();

        // Helper to process accumulated tag content
        let process_tag = |tag: &str, content: &str, doc: &mut ParsedDocumentation, ext: &mut JavadocExtensions| {
            let content = content.trim();
            if content.is_empty() && tag != "@deprecated" {
                return;
            }

            if let Some(caps) = PARAM_TAG.captures(&format!("{} {}", tag, content)) {
                let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let desc = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                doc.params.push((name.to_string(), None, Some(desc.to_string())));
            } else if let Some(caps) = RETURN_TAG.captures(&format!("{} {}", tag, content)) {
                let desc = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                doc.returns = Some((None, Some(desc.to_string())));
            } else if let Some(caps) = THROWS_TAG.captures(&format!("{} {}", tag, content)) {
                let exc_type = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let desc = caps.get(2).map(|m| m.as_str());
                doc.throws.push((exc_type.to_string(), desc.map(|s| s.to_string())));
            } else if let Some(caps) = AUTHOR_TAG.captures(&format!("{} {}", tag, content)) {
                let author = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                doc.author = Some(author.to_string());
                ext.authors.push(author.to_string());
            } else if let Some(caps) = SINCE_TAG.captures(&format!("{} {}", tag, content)) {
                let since = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                doc.since = Some(since.to_string());
            } else if let Some(caps) = VERSION_TAG.captures(&format!("{} {}", tag, content)) {
                let version = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                ext.version = Some(version.to_string());
            } else if let Some(caps) = SEE_TAG.captures(&format!("{} {}", tag, content)) {
                let reference = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                doc.see_refs.push(reference.to_string());
            } else if tag == "@deprecated" {
                let msg = if content.is_empty() {
                    "Deprecated".to_string()
                } else {
                    content.to_string()
                };
                doc.deprecated = Some(msg);
            }
        };

        for line in &lines {
            let stripped = Self::strip_comment_markers(line);

            // Check for new tag
            if stripped.starts_with('@') {
                // Process previous tag if any
                if let Some(ref tag) = current_tag {
                    process_tag(tag, &tag_content, &mut doc, &mut extensions);
                }

                // Start new tag
                let parts: Vec<&str> = stripped.splitn(2, char::is_whitespace).collect();
                current_tag = Some(parts[0].to_string());
                tag_content = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
            } else if current_tag.is_some() {
                // Continue current tag
                if !tag_content.is_empty() {
                    tag_content.push(' ');
                }
                tag_content.push_str(stripped);
            } else {
                // Regular content line
                content_lines.push(stripped.to_string());
            }
        }

        // Process final tag if any
        if let Some(ref tag) = current_tag {
            process_tag(tag, &tag_content, &mut doc, &mut extensions);
        }

        // Process content for summary and description
        let full_text = content_lines.join("\n");

        // Check for {@inheritDoc}
        if INHERIT_DOC.is_match(&full_text) {
            extensions.inherits_doc = true;
        }

        // Process inline tags and extract links
        let (processed_text, inline_links) = Self::process_inline_tags(&full_text);
        extensions.inline_links = inline_links.clone();

        // Add inline links to see_refs
        for link in inline_links {
            if !doc.see_refs.contains(&link) {
                doc.see_refs.push(link);
            }
        }

        // Extract code examples
        extensions.code_examples = Self::extract_code_examples(&full_text);
        for example in &extensions.code_examples {
            doc.examples.push(example.clone());
        }

        // Extract summary
        let summary = Self::extract_summary(&processed_text);
        if !summary.is_empty() {
            doc.summary = Some(summary.clone());
        }

        // Set description if there's more content
        let stripped_html = Self::strip_html(&processed_text);
        let trimmed = stripped_html.trim();
        if !trimmed.is_empty() && trimmed.len() > summary.len() {
            doc.description = Some(trimmed.to_string());
        }

        // Store extensions in custom tags
        if extensions.is_package_doc {
            doc.custom_tags.push(("package_doc".to_string(), "true".to_string()));
        }
        if extensions.is_type_doc {
            doc.custom_tags.push(("type_doc".to_string(), "true".to_string()));
        }
        if let Some(ref version) = extensions.version {
            doc.custom_tags.push(("version".to_string(), version.clone()));
        }
        if extensions.inherits_doc {
            doc.custom_tags.push(("inherits_doc".to_string(), "true".to_string()));
        }
        if extensions.authors.len() > 1 {
            doc.custom_tags.push(("multiple_authors".to_string(), "true".to_string()));
        }

        doc
    }

    fn standard_name(&self) -> &'static str {
        "javadoc"
    }

    /// @acp:summary "Converts parsed Javadoc to ACP suggestions with Java-specific handling"
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

        // Convert @throws to AI hint
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

        // Java-specific: package documentation
        if parsed.custom_tags.iter().any(|(k, v)| k == "package_doc" && v == "true") {
            suggestions.push(Suggestion::new(
                target,
                line,
                AnnotationType::Module,
                parsed.summary.as_deref().unwrap_or(target),
                SuggestionSource::Converted,
            ));
        }

        // Java-specific: type documentation (class/interface)
        if parsed.custom_tags.iter().any(|(k, v)| k == "type_doc" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "type-level documentation",
                SuggestionSource::Converted,
            ));
        }

        // Java-specific: inherits documentation
        if parsed.custom_tags.iter().any(|(k, v)| k == "inherits_doc" && v == "true") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "inherits documentation from parent",
                SuggestionSource::Converted,
            ));
        }

        // Convert @since to AI hint
        if let Some(since) = &parsed.since {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("since {}", since),
                SuggestionSource::Converted,
            ));
        }

        // Convert version to AI hint
        if let Some((_, version)) = parsed.custom_tags.iter().find(|(k, _)| k == "version") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("version {}", version),
                SuggestionSource::Converted,
            ));
        }

        // Convert examples existence to AI hint
        if !parsed.examples.is_empty() {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "has code examples",
                SuggestionSource::Converted,
            ));
        }

        // Convert author to AI hint if present
        if let Some(author) = &parsed.author {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("author: {}", author),
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
    fn test_strip_comment_markers() {
        assert_eq!(JavadocParser::strip_comment_markers("/** Hello"), "Hello");
        assert_eq!(JavadocParser::strip_comment_markers(" * Hello"), "Hello");
        assert_eq!(JavadocParser::strip_comment_markers(" */"), "");
        assert_eq!(JavadocParser::strip_comment_markers("   *   Indented"), "Indented");
    }

    #[test]
    fn test_strip_html() {
        assert_eq!(
            JavadocParser::strip_html("Hello <b>world</b>!"),
            "Hello world!"
        );
        assert_eq!(
            JavadocParser::strip_html("<p>Paragraph</p>"),
            "Paragraph"
        );
    }

    #[test]
    fn test_process_inline_tags() {
        let (processed, links) = JavadocParser::process_inline_tags(
            "See {@link String} for details about {@code format}"
        );
        assert!(processed.contains("String"));
        assert!(processed.contains("`format`"));
        assert!(links.contains(&"String".to_string()));
    }

    #[test]
    fn test_extract_summary() {
        let summary = JavadocParser::extract_summary(
            "Returns the length of this string. The length is equal to the number of characters."
        );
        assert_eq!(summary, "Returns the length of this string.");
    }

    #[test]
    fn test_parse_basic_javadoc() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Returns the character at the specified index.
 * An index ranges from 0 to length() - 1.
 */
"#);

        assert_eq!(
            doc.summary,
            Some("Returns the character at the specified index.".to_string())
        );
    }

    #[test]
    fn test_parse_with_params() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Copies characters from this string into the destination array.
 *
 * @param srcBegin index of the first character to copy
 * @param srcEnd index after the last character to copy
 * @param dst the destination array
 */
"#);

        assert_eq!(doc.params.len(), 3);
        assert_eq!(doc.params[0].0, "srcBegin");
        assert_eq!(doc.params[1].0, "srcEnd");
        assert_eq!(doc.params[2].0, "dst");
    }

    #[test]
    fn test_parse_with_return() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Returns the length of this string.
 *
 * @return the length of the sequence of characters
 */
"#);

        assert!(doc.returns.is_some());
        let (_, desc) = doc.returns.as_ref().unwrap();
        assert!(desc.as_ref().unwrap().contains("length"));
    }

    #[test]
    fn test_parse_with_throws() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Returns the character at the specified index.
 *
 * @throws IndexOutOfBoundsException if the index is out of range
 * @throws NullPointerException if the string is null
 */
"#);

        assert_eq!(doc.throws.len(), 2);
        assert_eq!(doc.throws[0].0, "IndexOutOfBoundsException");
        assert_eq!(doc.throws[1].0, "NullPointerException");
    }

    #[test]
    fn test_parse_with_exception() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Parses the string.
 *
 * @exception ParseException if parsing fails
 */
"#);

        assert_eq!(doc.throws.len(), 1);
        assert_eq!(doc.throws[0].0, "ParseException");
    }

    #[test]
    fn test_parse_with_see() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Creates a new string builder.
 *
 * @see StringBuilder
 * @see StringBuffer#append(String)
 */
"#);

        assert!(doc.see_refs.contains(&"StringBuilder".to_string()));
        assert!(doc.see_refs.contains(&"StringBuffer#append(String)".to_string()));
    }

    #[test]
    fn test_parse_with_deprecated() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Gets the date.
 *
 * @deprecated Use {@link LocalDate} instead
 */
"#);

        assert!(doc.deprecated.is_some());
        assert!(doc.deprecated.as_ref().unwrap().contains("LocalDate"));
    }

    #[test]
    fn test_parse_with_author_and_since() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * A utility class for string operations.
 *
 * @author John Doe
 * @since 1.0
 * @version 2.1
 */
"#);

        assert_eq!(doc.author, Some("John Doe".to_string()));
        assert_eq!(doc.since, Some("1.0".to_string()));
        assert!(doc.custom_tags.iter().any(|(k, v)| k == "version" && v == "2.1"));
    }

    #[test]
    fn test_parse_with_inline_link() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Returns a string similar to {@link String#valueOf(Object)}.
 */
"#);

        assert!(doc.see_refs.contains(&"String#valueOf(Object)".to_string()));
    }

    #[test]
    fn test_parse_with_code_block() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Formats a string.
 *
 * <pre>
 * String result = format("Hello %s", "World");
 * </pre>
 */
"#);

        assert!(!doc.examples.is_empty());
        assert!(doc.examples[0].contains("format"));
    }

    #[test]
    fn test_parse_with_inherit_doc() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * {@inheritDoc}
 */
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "inherits_doc" && v == "true"));
    }

    #[test]
    fn test_parse_multiline_param() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Processes input.
 *
 * @param data the input data to process,
 *             which can span multiple lines
 */
"#);

        assert_eq!(doc.params.len(), 1);
        let (_, _, desc) = &doc.params[0];
        assert!(desc.as_ref().unwrap().contains("multiple lines"));
    }

    #[test]
    fn test_parse_html_in_description() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * <p>Returns the <b>formatted</b> string.</p>
 *
 * <ul>
 *   <li>Item 1</li>
 *   <li>Item 2</li>
 * </ul>
 */
"#);

        // Summary should have HTML stripped
        assert!(doc.summary.is_some());
        let summary = doc.summary.as_ref().unwrap();
        assert!(!summary.contains("<p>"));
        assert!(!summary.contains("<b>"));
    }

    #[test]
    fn test_package_doc_parser() {
        let parser = JavadocParser::for_package();
        let doc = parser.parse(r#"
/**
 * Provides utility classes for string manipulation.
 */
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "package_doc" && v == "true"));
    }

    #[test]
    fn test_type_doc_parser() {
        let parser = JavadocParser::for_type();
        let doc = parser.parse(r#"
/**
 * A class representing a person.
 */
"#);

        assert!(doc.custom_tags.iter().any(|(k, v)| k == "type_doc" && v == "true"));
    }

    #[test]
    fn test_to_suggestions_basic() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Creates a new instance of the class.
 */
"#);

        let suggestions = parser.to_suggestions(&doc, "MyClass", 10);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Summary &&
            s.value.contains("Creates a new instance")
        ));
    }

    #[test]
    fn test_to_suggestions_deprecated() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Old method.
 * @deprecated Use newMethod instead
 */
"#);

        let suggestions = parser.to_suggestions(&doc, "oldMethod", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Deprecated
        ));
    }

    #[test]
    fn test_to_suggestions_throws() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Parses input.
 * @throws IOException if reading fails
 * @throws ParseException if parsing fails
 */
"#);

        let suggestions = parser.to_suggestions(&doc, "parse", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("throws")
        ));
    }

    #[test]
    fn test_to_suggestions_refs() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Gets the value.
 * @see OtherClass
 */
"#);

        let suggestions = parser.to_suggestions(&doc, "getValue", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::Ref &&
            s.value == "OtherClass"
        ));
    }

    #[test]
    fn test_to_suggestions_since() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * New feature.
 * @since 2.0
 */
"#);

        let suggestions = parser.to_suggestions(&doc, "feature", 1);

        assert!(suggestions.iter().any(|s|
            s.annotation_type == AnnotationType::AiHint &&
            s.value.contains("since 2.0")
        ));
    }

    #[test]
    fn test_to_suggestions_examples() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Formats output.
 * <pre>
 * format("test");
 * </pre>
 */
"#);

        let suggestions = parser.to_suggestions(&doc, "format", 1);

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
    fn test_extract_summary_multi_sentence() {
        let summary = JavadocParser::extract_summary("First sentence. Second sentence.");
        assert_eq!(summary, "First sentence.");
    }

    #[test]
    fn test_deprecated_empty() {
        let parser = JavadocParser::new();
        let doc = parser.parse(r#"
/**
 * Old method.
 * @deprecated
 */
"#);

        assert!(doc.deprecated.is_some());
        assert_eq!(doc.deprecated.as_ref().unwrap(), "Deprecated");
    }
}
