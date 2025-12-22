//! @acp:module "JSDoc Parser"
//! @acp:summary "Parses JSDoc/TSDoc comments and converts to ACP format"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # JSDoc Parser
//!
//! Parses JSDoc and TSDoc documentation comments and converts them to
//! ACP annotations. Supports:
//!
//! ## JSDoc Tags
//! - @description, @summary
//! - @deprecated
//! - @see, @link
//! - @todo, @fixme
//! - @param, @returns, @throws
//! - @private, @internal, @public
//! - @module, @category
//! - @readonly
//! - @example
//!
//! ## TSDoc Extensions
//! - @alpha, @beta - Stability modifiers
//! - @packageDocumentation - Module-level docs
//! - @remarks - Extended description
//! - @defaultValue - Default value documentation
//! - @typeParam - Generic type parameters
//! - @override, @virtual, @sealed - Inheritance modifiers
//! - {@inheritDoc} - Documentation inheritance

use std::sync::LazyLock;

use regex::Regex;

use super::{DocStandardParser, ParsedDocumentation};
use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Matches JSDoc tags (anchored to line start)"
static JSDOC_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@(\w+)(?:\s+\{([^}]+)\})?\s*(.*)").expect("Invalid JSDoc tag regex")
});

/// @acp:summary "Matches inline {@link ...} references"
static JSDOC_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{@link\s+([^}]+)\}").expect("Invalid JSDoc link regex")
});

/// @acp:summary "Matches inline {@inheritDoc ...} references"
static INHERIT_DOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{@inheritDoc(?:\s+([^}]+))?\}").expect("Invalid inheritDoc regex")
});

/// @acp:summary "TSDoc-specific fields extending ParsedDocumentation"
#[derive(Debug, Clone, Default)]
pub struct TsDocExtensions {
    /// @alpha modifier - unstable API
    pub is_alpha: bool,

    /// @beta modifier - preview API
    pub is_beta: bool,

    /// @packageDocumentation - file/module level doc
    pub is_package_doc: bool,

    /// @remarks - extended description
    pub remarks: Option<String>,

    /// @privateRemarks - internal notes (not exported)
    pub private_remarks: Option<String>,

    /// @defaultValue entries
    pub default_values: Vec<(String, String)>,

    /// @typeParam entries: (name, description)
    pub type_params: Vec<(String, Option<String>)>,

    /// @override modifier
    pub is_override: bool,

    /// @virtual modifier
    pub is_virtual: bool,

    /// @sealed modifier
    pub is_sealed: bool,

    /// {@inheritDoc Target} references
    pub inherit_doc: Option<String>,

    /// @eventProperty modifier
    pub is_event_property: bool,
}

/// @acp:summary "Parses JSDoc/TSDoc documentation comments"
/// @acp:lock normal
pub struct JsDocParser {
    /// Whether to parse TSDoc-specific tags
    parse_tsdoc: bool,
}

impl JsDocParser {
    /// @acp:summary "Creates a new JSDoc parser"
    pub fn new() -> Self {
        Self { parse_tsdoc: false }
    }

    /// @acp:summary "Creates a parser with TSDoc support enabled"
    pub fn with_tsdoc() -> Self {
        Self { parse_tsdoc: true }
    }

    /// @acp:summary "Extracts inline links from text"
    fn extract_inline_links(&self, text: &str) -> Vec<String> {
        JSDOC_LINK
            .captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// @acp:summary "Extracts {@inheritDoc Target} references"
    fn extract_inherit_doc(&self, text: &str) -> Option<String> {
        INHERIT_DOC
            .captures(text)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    }

    /// @acp:summary "Checks if a tag continues multiline content"
    fn is_continuation_line(line: &str) -> bool {
        !line.is_empty() && !line.starts_with('@')
    }
}

impl Default for JsDocParser {
    fn default() -> Self {
        Self::new()
    }
}

/// @acp:summary "TSDoc parser with full TSDoc support"
/// @acp:lock normal
pub struct TsDocParser {
    /// TSDoc extensions parsed from comments
    pub extensions: TsDocExtensions,
}

impl TsDocParser {
    /// @acp:summary "Creates a new TSDoc parser"
    pub fn new() -> Self {
        Self {
            extensions: TsDocExtensions::default(),
        }
    }

    /// @acp:summary "Gets the parsed TSDoc extensions"
    pub fn extensions(&self) -> &TsDocExtensions {
        &self.extensions
    }
}

impl Default for TsDocParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocStandardParser for JsDocParser {
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let mut description_lines = Vec::new();
        let mut in_description = true;
        let mut current_example = String::new();
        let mut in_example = false;

        // For multiline tag content
        let mut current_tag: Option<String> = None;
        let mut current_tag_content = String::new();

        // TSDoc extension tracking
        let mut remarks_content = String::new();
        let mut in_remarks = false;

        for line in raw_comment.lines() {
            // Clean the line (remove leading whitespace first)
            let trimmed = line.trim();

            // Skip opening/closing JSDoc markers (standalone lines)
            if trimmed == "/**" || trimmed == "*/" || trimmed == "/*" {
                continue;
            }

            // Handle single-line JSDoc: /** content */ or /** @tag */
            let line = if trimmed.starts_with("/**") && trimmed.ends_with("*/") {
                trimmed
                    .trim_start_matches("/**")
                    .trim_end_matches("*/")
                    .trim()
            } else {
                // Remove leading * and whitespace for content lines
                trimmed
                    .trim_start_matches('*')
                    .trim()
            };

            // Skip empty lines at the start
            if line.is_empty() && description_lines.is_empty() && !in_example && !in_remarks {
                continue;
            }

            // Check for JSDoc tag
            if let Some(caps) = JSDOC_TAG.captures(line) {
                in_description = false;

                // Save current example if we were in one
                if in_example && !current_example.is_empty() {
                    doc.examples.push(current_example.trim().to_string());
                    current_example = String::new();
                    in_example = false;
                }

                // Save remarks if we were collecting them
                if in_remarks && !remarks_content.is_empty() {
                    doc.notes.push(remarks_content.trim().to_string());
                    remarks_content = String::new();
                    in_remarks = false;
                }

                // Save previous multiline tag content
                if let Some(tag) = current_tag.take() {
                    self.save_multiline_tag(&mut doc, &tag, &current_tag_content);
                    current_tag_content.clear();
                }

                let tag = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let type_info = caps.get(2).map(|m| m.as_str().to_string());
                let content = caps.get(3).map(|m| m.as_str().trim().to_string());

                match tag {
                    "description" | "desc" => {
                        if let Some(desc) = content {
                            if !desc.is_empty() {
                                doc.description = Some(desc);
                            }
                        }
                    }
                    "summary" => {
                        doc.summary = content;
                    }
                    "deprecated" => {
                        doc.deprecated = content.or(Some("Deprecated".to_string()));
                    }
                    "see" | "link" => {
                        if let Some(ref_target) = content {
                            doc.see_refs.push(ref_target);
                        }
                    }
                    "todo" | "fixme" => {
                        if let Some(msg) = content {
                            doc.todos.push(msg);
                        }
                    }
                    "param" | "arg" | "argument" => {
                        if let Some(rest) = content {
                            // Parse "name description" or just "name"
                            let parts: Vec<&str> = rest.splitn(2, |c: char| c.is_whitespace()).collect();
                            let name = parts.first().unwrap_or(&"").to_string();
                            let desc = parts.get(1).map(|s| s.trim().to_string());
                            if !name.is_empty() {
                                doc.params.push((name, type_info, desc));
                            }
                        }
                    }
                    "returns" | "return" => {
                        doc.returns = Some((type_info, content));
                    }
                    "throws" | "exception" | "raise" => {
                        let exc_type = type_info.unwrap_or_else(|| {
                            content.clone().unwrap_or_default()
                        });
                        if !exc_type.is_empty() {
                            doc.throws.push((exc_type, content));
                        }
                    }
                    "example" => {
                        in_example = true;
                        if let Some(ex) = content {
                            if !ex.is_empty() {
                                current_example.push_str(&ex);
                                current_example.push('\n');
                            }
                        }
                    }
                    "module" | "fileoverview" | "packageDocumentation" => {
                        if let Some(name) = content.clone() {
                            if !name.is_empty() {
                                doc.custom_tags.push(("module".to_string(), name));
                            }
                        }
                        // Mark as package doc for TSDoc
                        if self.parse_tsdoc && tag == "packageDocumentation" {
                            doc.custom_tags.push(("packageDocumentation".to_string(), "true".to_string()));
                        }
                    }
                    "category" | "group" => {
                        if let Some(cat) = content {
                            doc.custom_tags.push(("category".to_string(), cat));
                        }
                    }
                    "private" => {
                        doc.custom_tags.push(("visibility".to_string(), "private".to_string()));
                    }
                    "internal" => {
                        doc.custom_tags.push(("visibility".to_string(), "internal".to_string()));
                    }
                    "protected" => {
                        doc.custom_tags.push(("visibility".to_string(), "protected".to_string()));
                    }
                    "public" => {
                        doc.custom_tags.push(("visibility".to_string(), "public".to_string()));
                    }
                    "readonly" => {
                        doc.custom_tags.push(("readonly".to_string(), "true".to_string()));
                    }
                    "since" => {
                        doc.since = content;
                    }
                    "author" => {
                        doc.author = content;
                    }
                    "note" | "remark" => {
                        if let Some(note) = content {
                            doc.notes.push(note);
                        }
                    }
                    "warning" | "warn" => {
                        if let Some(warning) = content {
                            doc.notes.push(format!("Warning: {}", warning));
                        }
                    }
                    // TSDoc-specific tags
                    "alpha" => {
                        doc.custom_tags.push(("stability".to_string(), "alpha".to_string()));
                    }
                    "beta" => {
                        doc.custom_tags.push(("stability".to_string(), "beta".to_string()));
                    }
                    "remarks" => {
                        in_remarks = true;
                        if let Some(r) = content {
                            if !r.is_empty() {
                                remarks_content.push_str(&r);
                                remarks_content.push('\n');
                            }
                        }
                    }
                    "privateRemarks" => {
                        // Store but don't export (internal notes)
                        if let Some(r) = content {
                            doc.custom_tags.push(("privateRemarks".to_string(), r));
                        }
                    }
                    "defaultValue" => {
                        if let Some(val) = content {
                            doc.custom_tags.push(("defaultValue".to_string(), val));
                        }
                    }
                    "typeParam" | "typeparam" => {
                        if let Some(rest) = content {
                            // Parse "T description" or just "T"
                            let parts: Vec<&str> = rest.splitn(2, |c: char| c.is_whitespace()).collect();
                            let name = parts.first().unwrap_or(&"").to_string();
                            let desc = parts.get(1).map(|s| s.trim().to_string());
                            if !name.is_empty() {
                                doc.custom_tags.push(("typeParam".to_string(),
                                    format!("{}: {}", name, desc.unwrap_or_default())));
                            }
                        }
                    }
                    "override" => {
                        doc.custom_tags.push(("override".to_string(), "true".to_string()));
                    }
                    "virtual" => {
                        doc.custom_tags.push(("virtual".to_string(), "true".to_string()));
                    }
                    "sealed" => {
                        doc.custom_tags.push(("sealed".to_string(), "true".to_string()));
                    }
                    "eventProperty" => {
                        doc.custom_tags.push(("eventProperty".to_string(), "true".to_string()));
                    }
                    _ => {
                        // Store unknown tags (may include multiline content)
                        if let Some(val) = content.clone() {
                            if !val.is_empty() {
                                doc.custom_tags.push((tag.to_string(), val));
                            } else {
                                // Tag with no inline content - might be multiline
                                current_tag = Some(tag.to_string());
                            }
                        } else {
                            // Modifier tag with no value
                            doc.custom_tags.push((tag.to_string(), String::new()));
                        }
                    }
                }
            } else if in_example {
                // Continue collecting example content
                current_example.push_str(line);
                current_example.push('\n');
            } else if in_remarks {
                // Continue collecting remarks content
                remarks_content.push_str(line);
                remarks_content.push('\n');
            } else if current_tag.is_some() && Self::is_continuation_line(line) {
                // Continue multiline tag content
                current_tag_content.push_str(line);
                current_tag_content.push('\n');
            } else if in_description && !line.is_empty() {
                description_lines.push(line.to_string());
            }
        }

        // Save final example
        if in_example && !current_example.is_empty() {
            doc.examples.push(current_example.trim().to_string());
        }

        // Save final remarks
        if in_remarks && !remarks_content.is_empty() {
            doc.notes.push(remarks_content.trim().to_string());
        }

        // Save final multiline tag
        if let Some(tag) = current_tag.take() {
            self.save_multiline_tag(&mut doc, &tag, &current_tag_content);
        }

        // First line of description becomes summary (if not already set)
        if doc.summary.is_none() && !description_lines.is_empty() {
            doc.summary = Some(description_lines[0].clone());
        }

        // Join description lines
        if !description_lines.is_empty() && doc.description.is_none() {
            doc.description = Some(description_lines.join(" "));
        }

        // Extract inline links from description
        if let Some(desc) = &doc.description {
            let links = self.extract_inline_links(desc);
            doc.see_refs.extend(links);

            // Extract {@inheritDoc} references
            if let Some(inherit) = self.extract_inherit_doc(desc) {
                doc.custom_tags.push(("inheritDoc".to_string(), inherit));
            }
        }

        doc
    }

    fn standard_name(&self) -> &'static str {
        if self.parse_tsdoc {
            "tsdoc"
        } else {
            "jsdoc"
        }
    }

    fn to_suggestions(
        &self,
        parsed: &ParsedDocumentation,
        target: &str,
        line: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Convert summary
        if let Some(summary) = &parsed.summary {
            let truncated = super::truncate_summary(summary, 100);
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

        // TSDoc-specific conversions
        if self.parse_tsdoc {
            // Convert @alpha/@beta to @acp:stability
            if let Some((_, stability)) = parsed.custom_tags.iter().find(|(k, _)| k == "stability") {
                suggestions.push(Suggestion::new(
                    target,
                    line,
                    AnnotationType::Stability,
                    stability,
                    SuggestionSource::Converted,
                ));
            }

            // Convert @defaultValue to AI hint
            for (key, value) in &parsed.custom_tags {
                if key == "defaultValue" {
                    suggestions.push(Suggestion::ai_hint(
                        target,
                        line,
                        format!("default: {}", value),
                        SuggestionSource::Converted,
                    ));
                }
                if key == "typeParam" {
                    suggestions.push(Suggestion::ai_hint(
                        target,
                        line,
                        format!("type param {}", value),
                        SuggestionSource::Converted,
                    ));
                }
                if key == "override" && value == "true" {
                    suggestions.push(Suggestion::ai_hint(
                        target,
                        line,
                        "overrides parent",
                        SuggestionSource::Converted,
                    ));
                }
                if key == "sealed" && value == "true" {
                    suggestions.push(Suggestion::lock(
                        target,
                        line,
                        "strict",
                        SuggestionSource::Converted,
                    ));
                }
            }
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

        // Convert notes/warnings/remarks to AI hints
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

impl JsDocParser {
    /// @acp:summary "Saves multiline tag content to the appropriate field"
    fn save_multiline_tag(&self, doc: &mut ParsedDocumentation, tag: &str, content: &str) {
        let content = content.trim().to_string();
        if content.is_empty() {
            return;
        }

        match tag {
            "description" | "desc" => {
                doc.description = Some(content);
            }
            "example" => {
                doc.examples.push(content);
            }
            _ => {
                doc.custom_tags.push((tag.to_string(), content));
            }
        }
    }
}

impl DocStandardParser for TsDocParser {
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation {
        // Delegate to JsDocParser with TSDoc mode enabled
        let parser = JsDocParser::with_tsdoc();
        parser.parse(raw_comment)
    }

    fn standard_name(&self) -> &'static str {
        "tsdoc"
    }

    fn to_suggestions(
        &self,
        parsed: &ParsedDocumentation,
        target: &str,
        line: usize,
    ) -> Vec<Suggestion> {
        // Delegate to JsDocParser with TSDoc mode enabled
        let parser = JsDocParser::with_tsdoc();
        parser.to_suggestions(parsed, target, line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_jsdoc() {
        let parser = JsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * Validates a user session.
             * @param {string} token - The JWT token
             * @returns {Promise<User>} The user object
             * @deprecated Use validateSessionV2 instead
             */
        "#);

        assert_eq!(doc.summary, Some("Validates a user session.".to_string()));
        assert_eq!(doc.deprecated, Some("Use validateSessionV2 instead".to_string()));
        assert_eq!(doc.params.len(), 1);
        assert_eq!(doc.params[0].0, "token");
        assert!(doc.returns.is_some());
    }

    #[test]
    fn test_parse_module_jsdoc() {
        let parser = JsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * @module Authentication
             * @category Security
             */
        "#);

        assert_eq!(doc.get_module(), Some("Authentication"));
        assert_eq!(doc.get_category(), Some("Security"));
    }

    #[test]
    fn test_parse_visibility_tags() {
        let parser = JsDocParser::new();

        let private_doc = parser.parse("/** @private */");
        assert_eq!(private_doc.get_visibility(), Some("private"));

        let internal_doc = parser.parse("/** @internal */");
        assert_eq!(internal_doc.get_visibility(), Some("internal"));
    }

    #[test]
    fn test_parse_see_and_link() {
        let parser = JsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * See {@link OtherClass} for more info.
             * @see AnotherClass
             */
        "#);

        assert!(doc.see_refs.contains(&"OtherClass".to_string()));
        assert!(doc.see_refs.contains(&"AnotherClass".to_string()));
    }

    #[test]
    fn test_parse_throws() {
        let parser = JsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * @throws {Error} When something goes wrong
             * @throws {ValidationError} When validation fails
             */
        "#);

        assert_eq!(doc.throws.len(), 2);
        assert_eq!(doc.throws[0].0, "Error");
        assert_eq!(doc.throws[1].0, "ValidationError");
    }

    #[test]
    fn test_parse_todo() {
        let parser = JsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * @todo Add proper error handling
             * @fixme This is broken
             */
        "#);

        assert_eq!(doc.todos.len(), 2);
        assert!(doc.todos[0].contains("error handling"));
    }

    #[test]
    fn test_parse_example() {
        let parser = JsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * Example function
             * @example
             * const result = myFunc();
             * console.log(result);
             */
        "#);

        assert!(!doc.examples.is_empty());
        assert!(doc.examples[0].contains("myFunc"));
    }

    #[test]
    fn test_parse_readonly() {
        let parser = JsDocParser::new();
        let doc = parser.parse("/** @readonly */");
        assert!(doc.is_readonly());
    }

    // ===== TSDoc-specific tests =====

    #[test]
    fn test_parse_tsdoc_alpha() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Experimental API
             * @alpha
             */
        "#);

        let has_alpha = doc.custom_tags.iter().any(|(k, v)| k == "stability" && v == "alpha");
        assert!(has_alpha);
    }

    #[test]
    fn test_parse_tsdoc_beta() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Preview API
             * @beta
             */
        "#);

        let has_beta = doc.custom_tags.iter().any(|(k, v)| k == "stability" && v == "beta");
        assert!(has_beta);
    }

    #[test]
    fn test_parse_tsdoc_package_documentation() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * @packageDocumentation
             * This is the main module.
             */
        "#);

        let has_pkg_doc = doc.custom_tags.iter().any(|(k, v)| k == "packageDocumentation" && v == "true");
        assert!(has_pkg_doc);
    }

    #[test]
    fn test_parse_tsdoc_remarks() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Brief summary.
             * @remarks
             * This is a longer explanation
             * that spans multiple lines.
             */
        "#);

        assert!(!doc.notes.is_empty());
        assert!(doc.notes[0].contains("longer explanation"));
    }

    #[test]
    fn test_parse_tsdoc_default_value() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * The timeout in milliseconds.
             * @defaultValue 5000
             */
        "#);

        let has_default = doc.custom_tags.iter().any(|(k, v)| k == "defaultValue" && v == "5000");
        assert!(has_default);
    }

    #[test]
    fn test_parse_tsdoc_type_param() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * A generic container.
             * @typeParam T The type of contained value
             */
        "#);

        let has_type_param = doc.custom_tags.iter().any(|(k, v)| k == "typeParam" && v.contains("T:"));
        assert!(has_type_param);
    }

    #[test]
    fn test_parse_tsdoc_override() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Overrides parent implementation.
             * @override
             */
        "#);

        let has_override = doc.custom_tags.iter().any(|(k, v)| k == "override" && v == "true");
        assert!(has_override);
    }

    #[test]
    fn test_parse_tsdoc_sealed() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * This class cannot be extended.
             * @sealed
             */
        "#);

        let has_sealed = doc.custom_tags.iter().any(|(k, v)| k == "sealed" && v == "true");
        assert!(has_sealed);
    }

    #[test]
    fn test_parse_tsdoc_virtual() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Can be overridden by subclasses.
             * @virtual
             */
        "#);

        let has_virtual = doc.custom_tags.iter().any(|(k, v)| k == "virtual" && v == "true");
        assert!(has_virtual);
    }

    #[test]
    fn test_parse_inherit_doc() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * {@inheritDoc ParentClass.method}
             */
        "#);

        let has_inherit = doc.custom_tags.iter().any(|(k, _)| k == "inheritDoc");
        assert!(has_inherit);
    }

    #[test]
    fn test_tsdoc_to_suggestions_alpha() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Experimental feature.
             * @alpha
             */
        "#);

        let suggestions = parser.to_suggestions(&doc, "myFunction", 10);
        let has_stability = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Stability && s.value == "alpha"
        });
        assert!(has_stability);
    }

    #[test]
    fn test_tsdoc_to_suggestions_sealed() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Locked class.
             * @sealed
             */
        "#);

        let suggestions = parser.to_suggestions(&doc, "MyClass", 10);
        let has_strict_lock = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Lock && s.value == "strict"
        });
        assert!(has_strict_lock);
    }

    #[test]
    fn test_tsdoc_to_suggestions_default_value() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Timeout setting.
             * @defaultValue 3000
             */
        "#);

        let suggestions = parser.to_suggestions(&doc, "timeout", 10);
        let has_default_hint = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::AiHint && s.value.contains("default:")
        });
        assert!(has_default_hint);
    }

    #[test]
    fn test_tsdoc_parser_delegation() {
        let parser = TsDocParser::new();
        let doc = parser.parse(r#"
            /**
             * API function.
             * @beta
             * @param {string} name The name
             */
        "#);

        assert_eq!(doc.summary, Some("API function.".to_string()));
        assert_eq!(doc.params.len(), 1);
        let has_beta = doc.custom_tags.iter().any(|(k, v)| k == "stability" && v == "beta");
        assert!(has_beta);
    }

    #[test]
    fn test_multiline_remarks() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Summary line.
             *
             * @remarks
             * First line of remarks.
             * Second line of remarks.
             * Third line with more detail.
             *
             * @param x A parameter
             */
        "#);

        assert!(!doc.notes.is_empty());
        let remarks = &doc.notes[0];
        assert!(remarks.contains("First line"));
        assert!(remarks.contains("Third line"));
    }

    #[test]
    fn test_complex_tsdoc() {
        let parser = JsDocParser::with_tsdoc();
        let doc = parser.parse(r#"
            /**
             * Processes user authentication.
             *
             * @remarks
             * This function handles OAuth2 and JWT tokens.
             * It's designed for high-throughput scenarios.
             *
             * @typeParam T The credential type
             * @param credentials User credentials
             * @returns The authenticated user
             * @throws {AuthError} When authentication fails
             * @beta
             * @see OAuthProvider
             *
             * @example
             * const user = await authenticate(creds);
             * console.log(user.name);
             */
        "#);

        assert_eq!(doc.summary, Some("Processes user authentication.".to_string()));
        assert!(!doc.notes.is_empty());
        assert!(doc.notes[0].contains("OAuth2"));
        assert_eq!(doc.params.len(), 1);
        assert!(doc.returns.is_some());
        assert_eq!(doc.throws.len(), 1);
        assert!(doc.see_refs.contains(&"OAuthProvider".to_string()));
        assert!(!doc.examples.is_empty());

        let has_beta = doc.custom_tags.iter().any(|(k, v)| k == "stability" && v == "beta");
        assert!(has_beta);

        let has_type_param = doc.custom_tags.iter().any(|(k, _)| k == "typeParam");
        assert!(has_type_param);
    }
}
