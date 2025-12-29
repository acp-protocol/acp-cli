//! @acp:module "Python Docstring Parser"
//! @acp:summary "Parses Python docstrings and converts to ACP format"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Python Docstring Parser
//!
//! Parses Python docstrings in multiple formats:
//!
//! ## Google Style
//! - Args/Parameters, Returns, Yields, Raises/Exceptions
//! - Attributes, Example/Examples, Note/Notes, Warning/Warnings
//! - See Also, Todo, References, Deprecated
//!
//! ## NumPy Style
//! - Parameters, Other Parameters, Returns, Yields, Receives
//! - Raises, Warns, See Also, Notes, References, Examples
//! - Attributes, Methods
//!
//! ## Sphinx/reST Style
//! - :param:, :type:, :returns:, :rtype:
//! - :raises:, :deprecated:, :version:, :since:
//! - :seealso:, :note:, :warning:, :example:, :todo:
//! - :var:, :ivar:, :cvar:, :meta:
//!
//! ## Plain Style
//! - First line = summary, rest = description

use std::sync::LazyLock;

use regex::Regex;

use super::{DocStandardParser, ParsedDocumentation};
use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Matches Google-style section headers"
static GOOGLE_SECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(Args|Arguments|Parameters|Returns|Return|Yields|Yield|Receives|Raises|Exceptions|Warns|Attributes|Example|Examples|Note|Notes|Warning|Warnings|See Also|Todo|Todos|References|Deprecated|Other Parameters|Keyword Args|Keyword Arguments|Methods|Class Attributes|Version|Since):\s*$"
    ).expect("Invalid Google section regex")
});

/// @acp:summary "Matches Sphinx-style tags"
static SPHINX_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^:(param|type|returns|rtype|raises|raise|var|ivar|cvar|deprecated|version|since|seealso|see|note|warning|example|todo|meta|keyword|kwarg|kwparam)(\s+\w+)?:\s*(.*)$"
    ).expect("Invalid Sphinx tag regex")
});

/// @acp:summary "Matches Google-style parameter lines (name (type): desc or name: desc)"
static GOOGLE_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)(?:\s*\(([^)]+)\))?:\s*(.*)$").expect("Invalid Google param regex")
});

/// @acp:summary "Matches NumPy-style parameter lines (name : type)"
static NUMPY_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\w+)\s*:\s*([^,\n]+)(?:,\s*optional)?$").expect("Invalid NumPy param regex")
});

/// @acp:summary "Python-specific extensions for docstrings"
#[derive(Debug, Clone, Default)]
pub struct PythonDocExtensions {
    /// Whether this is a generator function (has Yields)
    pub is_generator: bool,

    /// Whether this is an async function (has Receives for async generators)
    pub is_async_generator: bool,

    /// Class attributes (for class docstrings)
    pub class_attributes: Vec<(String, Option<String>, Option<String>)>,

    /// Instance variables
    pub instance_vars: Vec<(String, Option<String>, Option<String>)>,

    /// Class variables
    pub class_vars: Vec<(String, Option<String>, Option<String>)>,

    /// Methods (for class docstrings)
    pub methods: Vec<(String, Option<String>)>,

    /// Keyword arguments (separate from regular params)
    pub kwargs: Vec<(String, Option<String>, Option<String>)>,

    /// Version info
    pub version: Option<String>,

    /// Since version
    pub since: Option<String>,

    /// Warnings (distinct from notes)
    pub warnings: Vec<String>,

    /// Meta information
    pub meta: Vec<(String, String)>,
}

/// @acp:summary "Python docstring style"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocstringStyle {
    /// Google style with sections
    Google,
    /// NumPy style with underlined sections
    NumPy,
    /// Sphinx/reST style with :tags:
    Sphinx,
    /// Plain docstring (first line = summary)
    Plain,
}

/// @acp:summary "Parses Python docstrings"
/// @acp:lock normal
pub struct DocstringParser {
    /// Python-specific extensions parsed from docstrings
    extensions: PythonDocExtensions,
}

impl DocstringParser {
    /// @acp:summary "Creates a new docstring parser"
    pub fn new() -> Self {
        Self {
            extensions: PythonDocExtensions::default(),
        }
    }

    /// @acp:summary "Gets the parsed Python extensions"
    pub fn extensions(&self) -> &PythonDocExtensions {
        &self.extensions
    }

    /// @acp:summary "Detects the docstring style"
    pub fn detect_style(raw: &str) -> DocstringStyle {
        let lines: Vec<&str> = raw.lines().collect();

        // Check each line for style indicators
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check for Sphinx tags (line by line)
            if SPHINX_TAG.is_match(trimmed) {
                return DocstringStyle::Sphinx;
            }

            // Check for Google style sections (line by line)
            if GOOGLE_SECTION.is_match(trimmed) {
                return DocstringStyle::Google;
            }

            // Check for NumPy style (sections with underlines)
            if i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if !trimmed.is_empty() && next.chars().all(|c| c == '-') && next.len() >= 3 {
                    return DocstringStyle::NumPy;
                }
            }
        }

        DocstringStyle::Plain
    }

    /// @acp:summary "Parses Google-style docstring"
    fn parse_google_style(&self, raw: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let mut summary_lines = Vec::new();
        let mut current_section: Option<String> = None;
        let mut section_content = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim();

            // Check for section header
            if let Some(caps) = GOOGLE_SECTION.captures(trimmed) {
                // Save previous section
                self.save_section(&mut doc, current_section.as_deref(), &section_content);
                section_content.clear();

                current_section = Some(caps.get(1).unwrap().as_str().to_string());
            } else if current_section.is_some() {
                section_content.push(line.to_string());
            } else if !trimmed.is_empty() {
                summary_lines.push(trimmed.to_string());
            }
        }

        // Save last section
        self.save_section(&mut doc, current_section.as_deref(), &section_content);

        // First non-empty line is summary
        if !summary_lines.is_empty() {
            doc.summary = Some(summary_lines[0].clone());
            doc.description = Some(summary_lines.join(" "));
        }

        doc
    }

    /// @acp:summary "Saves section content to parsed documentation"
    fn save_section(
        &self,
        doc: &mut ParsedDocumentation,
        section: Option<&str>,
        content: &[String],
    ) {
        let section = match section {
            Some(s) => s,
            None => return,
        };

        // Normalize indentation: find minimum leading whitespace and strip it from all lines
        // This handles Google style (uniform indentation) and NumPy style (varying indentation)
        let min_indent = content
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.len() - s.trim_start().len())
            .min()
            .unwrap_or(0);

        let normalized: Vec<String> = content
            .iter()
            .map(|s| {
                if s.len() >= min_indent {
                    s[min_indent..].to_string()
                } else {
                    s.clone()
                }
            })
            .collect();

        let raw_text = normalized.join("\n");
        let text = raw_text.trim().to_string();

        if text.is_empty() {
            return;
        }

        match section {
            "Args" | "Arguments" | "Parameters" => {
                // Parse parameter entries
                for param in self.parse_params(&text) {
                    doc.params.push(param);
                }
            }
            "Other Parameters" => {
                // Parse as additional parameters with custom tag marker
                for param in self.parse_params(&text) {
                    doc.custom_tags.push((
                        "other_param".to_string(),
                        format!("{}: {}", param.0, param.2.unwrap_or_default()),
                    ));
                }
            }
            "Keyword Args" | "Keyword Arguments" => {
                // Parse keyword arguments
                for param in self.parse_params(&text) {
                    doc.custom_tags.push((
                        "kwarg".to_string(),
                        format!("{}: {}", param.0, param.2.unwrap_or_default()),
                    ));
                }
            }
            "Returns" | "Return" => {
                doc.returns = Some((None, Some(text)));
            }
            "Yields" | "Yield" => {
                // Yields indicates a generator function
                doc.returns = Some((None, Some(format!("Yields: {}", text))));
                doc.custom_tags
                    .push(("generator".to_string(), "true".to_string()));
            }
            "Receives" => {
                // For async generators
                doc.custom_tags.push(("receives".to_string(), text));
                doc.custom_tags
                    .push(("async_generator".to_string(), "true".to_string()));
            }
            "Raises" | "Exceptions" => {
                // Handle both Google style "ExcType: description" and
                // NumPy style where ExcType is on one line and description is indented
                let mut current_exc: Option<String> = None;
                let mut current_desc = Vec::new();

                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let is_indented = line.starts_with("    ") || line.starts_with("\t");

                    if !is_indented {
                        // Save previous exception
                        if let Some(exc) = current_exc.take() {
                            let desc = if current_desc.is_empty() {
                                None
                            } else {
                                Some(current_desc.join(" "))
                            };
                            doc.throws.push((exc, desc));
                            current_desc.clear();
                        }

                        // Check for "ExcType: description" format (Google style)
                        let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                        let exc_type = parts[0].trim().to_string();
                        if !exc_type.is_empty() {
                            current_exc = Some(exc_type);
                            if let Some(desc) = parts.get(1) {
                                let d = desc.trim();
                                if !d.is_empty() {
                                    current_desc.push(d.to_string());
                                }
                            }
                        }
                    } else if current_exc.is_some() {
                        // Description continuation
                        current_desc.push(trimmed.to_string());
                    }
                }

                // Save last exception
                if let Some(exc) = current_exc {
                    let desc = if current_desc.is_empty() {
                        None
                    } else {
                        Some(current_desc.join(" "))
                    };
                    doc.throws.push((exc, desc));
                }
            }
            "Warns" => {
                // Warnings that may be raised
                for line in text.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        doc.notes.push(format!("May warn: {}", line));
                    }
                }
            }
            "Note" | "Notes" => {
                doc.notes.push(text);
            }
            "Warning" | "Warnings" => {
                doc.notes.push(format!("Warning: {}", text));
            }
            "Example" | "Examples" => {
                doc.examples.push(text);
            }
            "See Also" | "References" => {
                for ref_line in text.lines() {
                    let ref_line = ref_line.trim();
                    if !ref_line.is_empty() {
                        doc.see_refs.push(ref_line.to_string());
                    }
                }
            }
            "Todo" | "Todos" => {
                doc.todos.push(text);
            }
            "Deprecated" => {
                doc.deprecated = Some(text);
            }
            "Attributes" | "Class Attributes" => {
                // Parse attribute entries similar to params
                for attr in self.parse_params(&text) {
                    doc.custom_tags.push((
                        format!("attr:{}", attr.0),
                        format!(
                            "{}: {}",
                            attr.1.unwrap_or_default(),
                            attr.2.unwrap_or_default()
                        ),
                    ));
                }
            }
            "Methods" => {
                // Parse method summaries for class docstrings
                for line in text.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        let parts: Vec<&str> = line.splitn(2, ':').collect();
                        let method_name = parts[0].trim().to_string();
                        let desc = parts.get(1).map(|s| s.trim().to_string());
                        if !method_name.is_empty() {
                            doc.custom_tags.push((
                                format!("method:{}", method_name),
                                desc.unwrap_or_default(),
                            ));
                        }
                    }
                }
            }
            "Version" => {
                doc.custom_tags.push(("version".to_string(), text));
            }
            "Since" => {
                doc.since = Some(text);
            }
            _ => {}
        }
    }

    /// @acp:summary "Parses parameter entries from text"
    /// Supports both Google style "name (type): desc" and NumPy style "name : type"
    fn parse_params(&self, text: &str) -> Vec<(String, Option<String>, Option<String>)> {
        let mut params = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_type: Option<String> = None;
        let mut current_desc = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Check if this is a new parameter (not indented continuation)
            let is_indented = line.starts_with("    ") || line.starts_with("\t");

            if !is_indented || current_name.is_none() {
                // Save previous parameter
                if let Some(name) = current_name.take() {
                    let desc = if current_desc.is_empty() {
                        None
                    } else {
                        Some(current_desc.join(" "))
                    };
                    params.push((name, current_type.take(), desc));
                    current_desc.clear();
                }

                // Try Google style first: "name (type): description" or "name: description"
                if let Some(caps) = GOOGLE_PARAM.captures(trimmed) {
                    current_name = Some(caps.get(1).unwrap().as_str().to_string());
                    current_type = caps.get(2).map(|m| m.as_str().to_string());
                    if let Some(desc) = caps.get(3) {
                        let d = desc.as_str().trim();
                        if !d.is_empty() {
                            current_desc.push(d.to_string());
                        }
                    }
                }
                // Try NumPy style: "name : type" or "name : type, optional"
                else if let Some(caps) = NUMPY_PARAM.captures(trimmed) {
                    current_name = Some(caps.get(1).unwrap().as_str().to_string());
                    current_type = Some(caps.get(2).unwrap().as_str().trim().to_string());
                }
                // Plain name without type
                else if !trimmed.contains(':') && !trimmed.contains(' ') {
                    current_name = Some(trimmed.to_string());
                }
            } else if current_name.is_some() {
                // Continuation of description
                current_desc.push(trimmed.to_string());
            }
        }

        // Save last parameter
        if let Some(name) = current_name {
            let desc = if current_desc.is_empty() {
                None
            } else {
                Some(current_desc.join(" "))
            };
            params.push((name, current_type, desc));
        }

        params
    }

    /// @acp:summary "Parses Sphinx-style docstring"
    fn parse_sphinx_style(&self, raw: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let mut summary_lines = Vec::new();
        let mut found_tag = false;

        // For multiline tag content
        let mut current_tag: Option<(String, Option<String>)> = None;
        let mut current_content = String::new();

        for line in raw.lines() {
            let trimmed = line.trim();

            if let Some(caps) = SPHINX_TAG.captures(trimmed) {
                // Save previous multiline tag if any
                if let Some((tag, name)) = current_tag.take() {
                    self.save_sphinx_tag(&mut doc, &tag, name.as_deref(), &current_content);
                    current_content.clear();
                }

                found_tag = true;
                let tag = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let name = caps.get(2).map(|m| m.as_str().trim().to_string());
                let content = caps.get(3).map(|m| m.as_str().trim().to_string());

                // Check if this tag might have multiline content
                if let Some(c) = &content {
                    if c.is_empty() {
                        // Start collecting multiline content
                        current_tag = Some((tag.to_string(), name));
                    } else {
                        // Single line tag
                        self.save_sphinx_tag(&mut doc, tag, name.as_deref(), c);
                    }
                } else {
                    // Tag with no content
                    self.save_sphinx_tag(&mut doc, tag, name.as_deref(), "");
                }
            } else if current_tag.is_some() && (line.starts_with("    ") || line.starts_with("\t"))
            {
                // Continuation of multiline content
                current_content.push_str(trimmed);
                current_content.push('\n');
            } else if !found_tag && !trimmed.is_empty() {
                summary_lines.push(trimmed.to_string());
            }
        }

        // Save final multiline tag
        if let Some((tag, name)) = current_tag.take() {
            self.save_sphinx_tag(&mut doc, &tag, name.as_deref(), &current_content);
        }

        if !summary_lines.is_empty() {
            doc.summary = Some(summary_lines[0].clone());
            doc.description = Some(summary_lines.join(" "));
        }

        doc
    }

    /// @acp:summary "Saves a Sphinx-style tag to parsed documentation"
    fn save_sphinx_tag(
        &self,
        doc: &mut ParsedDocumentation,
        tag: &str,
        name: Option<&str>,
        content: &str,
    ) {
        let content = content.trim().to_string();

        match tag {
            "param" => {
                if let Some(n) = name {
                    doc.params.push((
                        n.to_string(),
                        None,
                        if content.is_empty() {
                            None
                        } else {
                            Some(content)
                        },
                    ));
                }
            }
            "keyword" | "kwarg" | "kwparam" => {
                if let Some(n) = name {
                    doc.custom_tags
                        .push(("kwarg".to_string(), format!("{}: {}", n, content)));
                }
            }
            "type" => {
                // Update type for matching param
                if let Some(n) = name {
                    for param in &mut doc.params {
                        if param.0 == n {
                            param.1 = Some(content.clone());
                            break;
                        }
                    }
                }
            }
            "returns" => {
                doc.returns = Some((
                    None,
                    if content.is_empty() {
                        None
                    } else {
                        Some(content)
                    },
                ));
            }
            "rtype" => {
                if let Some(ret) = &mut doc.returns {
                    ret.0 = Some(content);
                } else {
                    doc.returns = Some((Some(content), None));
                }
            }
            "raises" | "raise" => {
                if let Some(exc) = name {
                    doc.throws.push((
                        exc.to_string(),
                        if content.is_empty() {
                            None
                        } else {
                            Some(content)
                        },
                    ));
                }
            }
            "deprecated" => {
                doc.deprecated = Some(if content.is_empty() {
                    "Deprecated".to_string()
                } else {
                    content
                });
            }
            "version" => {
                doc.custom_tags.push(("version".to_string(), content));
            }
            "since" => {
                doc.since = Some(content);
            }
            "seealso" | "see" => {
                if !content.is_empty() {
                    doc.see_refs.push(content);
                }
            }
            "note" => {
                if !content.is_empty() {
                    doc.notes.push(content);
                }
            }
            "warning" => {
                if !content.is_empty() {
                    doc.notes.push(format!("Warning: {}", content));
                }
            }
            "example" => {
                if !content.is_empty() {
                    doc.examples.push(content);
                }
            }
            "todo" => {
                if !content.is_empty() {
                    doc.todos.push(content);
                }
            }
            "var" | "ivar" | "cvar" => {
                if let Some(n) = name {
                    doc.custom_tags.push((format!("{}:{}", tag, n), content));
                }
            }
            "meta" => {
                if let Some(key) = name {
                    doc.custom_tags.push((format!("meta:{}", key), content));
                }
            }
            _ => {}
        }
    }

    /// @acp:summary "Parses plain-style docstring"
    fn parse_plain_style(&self, raw: &str) -> ParsedDocumentation {
        let mut doc = ParsedDocumentation::new();
        let lines: Vec<&str> = raw
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if !lines.is_empty() {
            doc.summary = Some(lines[0].to_string());
        }

        if lines.len() > 1 {
            doc.description = Some(lines.join(" "));
        }

        doc
    }

    /// @acp:summary "Parses NumPy-style docstring"
    fn parse_numpy_style(&self, raw: &str) -> ParsedDocumentation {
        // NumPy style is similar to Google but with underlined headers
        // For now, treat it similarly
        let mut doc = ParsedDocumentation::new();
        let lines: Vec<&str> = raw.lines().collect();
        let mut summary_lines = Vec::new();
        let mut current_section: Option<String> = None;
        let mut section_content = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Check if next line is underline (indicates section header)
            if i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if !line.is_empty() && next.chars().all(|c| c == '-') && next.len() >= 3 {
                    // Save previous section
                    self.save_section(&mut doc, current_section.as_deref(), &section_content);
                    section_content.clear();
                    current_section = Some(line.to_string());
                    i += 2; // Skip header and underline
                    continue;
                }
            }

            if current_section.is_some() {
                section_content.push(lines[i].to_string());
            } else if !line.is_empty() {
                summary_lines.push(line.to_string());
            }

            i += 1;
        }

        // Save last section
        self.save_section(&mut doc, current_section.as_deref(), &section_content);

        if !summary_lines.is_empty() {
            doc.summary = Some(summary_lines[0].clone());
            doc.description = Some(summary_lines.join(" "));
        }

        doc
    }
}

impl Default for DocstringParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocStandardParser for DocstringParser {
    fn parse(&self, raw_comment: &str) -> ParsedDocumentation {
        match Self::detect_style(raw_comment) {
            DocstringStyle::Google => self.parse_google_style(raw_comment),
            DocstringStyle::NumPy => self.parse_numpy_style(raw_comment),
            DocstringStyle::Sphinx => self.parse_sphinx_style(raw_comment),
            DocstringStyle::Plain => self.parse_plain_style(raw_comment),
        }
    }

    fn standard_name(&self) -> &'static str {
        "docstring"
    }

    /// @acp:summary "Converts parsed docstring to ACP suggestions with Python-specific handling"
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

        // Convert throws to AI hint
        if !parsed.throws.is_empty() {
            let throws_list: Vec<String> = parsed.throws.iter().map(|(t, _)| t.clone()).collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("raises {}", throws_list.join(", ")),
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

        // Python-specific: Convert generator hints from custom tags
        if parsed
            .custom_tags
            .iter()
            .any(|(k, v)| k == "generator" && v == "true")
        {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "generator function (uses yield)",
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert async generator hints
        if parsed
            .custom_tags
            .iter()
            .any(|(k, v)| k == "async_generator" && v == "true")
        {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                "async generator (uses yield and receives)",
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert version info
        if let Some((_, version)) = parsed.custom_tags.iter().find(|(k, _)| k == "version") {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("version: {}", version),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert since version
        if let Some(since) = &parsed.since {
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("since: {}", since),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert keyword arguments summary
        let kwargs: Vec<_> = parsed
            .custom_tags
            .iter()
            .filter(|(k, _)| k == "kwarg")
            .collect();
        if !kwargs.is_empty() {
            let kwarg_names: Vec<_> = kwargs
                .iter()
                .filter_map(|(_, v)| v.split(':').next())
                .map(|s| s.trim())
                .collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("accepts kwargs: {}", kwarg_names.join(", ")),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert class attributes summary
        let attrs: Vec<_> = parsed
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("attr:"))
            .collect();
        if !attrs.is_empty() {
            let attr_names: Vec<_> = attrs
                .iter()
                .map(|(k, _)| k.strip_prefix("attr:").unwrap_or(k))
                .collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("attributes: {}", attr_names.join(", ")),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert methods summary (for class docstrings)
        let methods: Vec<_> = parsed
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("method:"))
            .collect();
        if !methods.is_empty() {
            let method_names: Vec<_> = methods
                .iter()
                .map(|(k, _)| k.strip_prefix("method:").unwrap_or(k))
                .collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("methods: {}", method_names.join(", ")),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert instance variables
        let ivars: Vec<_> = parsed
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("ivar:"))
            .collect();
        if !ivars.is_empty() {
            let ivar_names: Vec<_> = ivars
                .iter()
                .map(|(k, _)| k.strip_prefix("ivar:").unwrap_or(k))
                .collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("instance vars: {}", ivar_names.join(", ")),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert class variables
        let cvars: Vec<_> = parsed
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("cvar:"))
            .collect();
        if !cvars.is_empty() {
            let cvar_names: Vec<_> = cvars
                .iter()
                .map(|(k, _)| k.strip_prefix("cvar:").unwrap_or(k))
                .collect();
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("class vars: {}", cvar_names.join(", ")),
                SuggestionSource::Converted,
            ));
        }

        // Python-specific: Convert meta tags
        let metas: Vec<_> = parsed
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("meta:"))
            .collect();
        for (key, value) in metas {
            let meta_key = key.strip_prefix("meta:").unwrap_or(key);
            suggestions.push(Suggestion::ai_hint(
                target,
                line,
                format!("{}: {}", meta_key, value),
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
        let truncate_at = trimmed[..max_len].rfind(' ').unwrap_or(max_len);
        format!("{}...", &trimmed[..truncate_at])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_style_google() {
        let google = "Summary line.\n\nArgs:\n    x: description";
        assert_eq!(
            DocstringParser::detect_style(google),
            DocstringStyle::Google
        );
    }

    #[test]
    fn test_detect_style_sphinx() {
        let sphinx = "Summary line.\n\n:param x: description";
        assert_eq!(
            DocstringParser::detect_style(sphinx),
            DocstringStyle::Sphinx
        );
    }

    #[test]
    fn test_detect_style_numpy() {
        let numpy = "Summary line.\n\nParameters\n----------\nx : int";
        assert_eq!(DocstringParser::detect_style(numpy), DocstringStyle::NumPy);
    }

    #[test]
    fn test_detect_style_plain() {
        let plain = "Just a simple docstring.";
        assert_eq!(DocstringParser::detect_style(plain), DocstringStyle::Plain);
    }

    #[test]
    fn test_parse_google_style() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Process a payment transaction.

Args:
    amount: The payment amount
    currency: The currency code (default: USD)

Returns:
    PaymentResult with transaction ID

Raises:
    PaymentError: If payment fails
"#,
        );

        assert_eq!(
            doc.summary,
            Some("Process a payment transaction.".to_string())
        );
        assert_eq!(doc.params.len(), 2);
        assert_eq!(doc.params[0].0, "amount");
        assert!(doc.returns.is_some());
        assert_eq!(doc.throws.len(), 1);
        assert_eq!(doc.throws[0].0, "PaymentError");
    }

    #[test]
    fn test_parse_sphinx_style() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Process a payment.

:param amount: The payment amount
:type amount: float
:returns: The result
:raises PaymentError: If payment fails
:deprecated: Use process_payment_v2 instead
"#,
        );

        assert_eq!(doc.summary, Some("Process a payment.".to_string()));
        assert_eq!(doc.params.len(), 1);
        assert_eq!(doc.params[0].0, "amount");
        assert!(doc.returns.is_some());
        assert_eq!(doc.throws.len(), 1);
        assert!(doc.deprecated.is_some());
    }

    #[test]
    fn test_parse_plain_style() {
        let parser = DocstringParser::new();
        let doc = parser.parse("Simple summary.\n\nMore details here.");

        assert_eq!(doc.summary, Some("Simple summary.".to_string()));
        assert!(doc.description.unwrap().contains("More details"));
    }

    #[test]
    fn test_parse_deprecated_section() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Old function.

Deprecated:
    Use new_function instead.
"#,
        );

        assert!(doc.deprecated.is_some());
        assert!(doc.deprecated.unwrap().contains("new_function"));
    }

    #[test]
    fn test_parse_notes_and_warnings() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Summary.

Note:
    Important note here.

Warning:
    Be careful with this.
"#,
        );

        assert_eq!(doc.notes.len(), 2);
    }

    // ========================================
    // Sprint 3: Python-specific feature tests
    // ========================================

    #[test]
    fn test_parse_google_yields_generator() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Generator function that yields items.

Yields:
    int: The next item in the sequence
"#,
        );

        assert!(doc.returns.is_some());
        let (_, desc) = doc.returns.as_ref().unwrap();
        assert!(desc.as_ref().unwrap().contains("Yields"));

        // Check generator flag in custom_tags
        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "generator" && v == "true"));
    }

    #[test]
    fn test_parse_google_receives_async_generator() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Async generator that receives values.

Yields:
    str: Generated value

Receives:
    int: Value sent to generator
"#,
        );

        // Check async_generator flag
        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "async_generator" && v == "true"));
        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "receives" && !v.is_empty()));
    }

    #[test]
    fn test_parse_google_keyword_args() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function with keyword arguments.

Args:
    x: Required argument

Keyword Args:
    timeout: Connection timeout
    retries: Number of retries
"#,
        );

        assert_eq!(doc.params.len(), 1);
        assert_eq!(doc.params[0].0, "x");

        // Check kwargs in custom_tags
        let kwargs: Vec<_> = doc
            .custom_tags
            .iter()
            .filter(|(k, _)| k == "kwarg")
            .collect();
        assert_eq!(kwargs.len(), 2);
    }

    #[test]
    fn test_parse_google_other_parameters() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function with other parameters.

Args:
    x: Main argument

Other Parameters:
    debug: Enable debug mode
    verbose: Verbosity level
"#,
        );

        assert_eq!(doc.params.len(), 1);

        let other_params: Vec<_> = doc
            .custom_tags
            .iter()
            .filter(|(k, _)| k == "other_param")
            .collect();
        assert_eq!(other_params.len(), 2);
    }

    #[test]
    fn test_parse_google_attributes() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
A class that does something.

Attributes:
    name (str): The name
    value (int): The value
"#,
        );

        let attrs: Vec<_> = doc
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("attr:"))
            .collect();
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn test_parse_google_methods() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
A utility class.

Methods:
    process: Processes the input
    validate: Validates the data
    cleanup: Cleans up resources
"#,
        );

        let methods: Vec<_> = doc
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("method:"))
            .collect();
        assert_eq!(methods.len(), 3);
    }

    #[test]
    fn test_parse_google_warns() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function that may emit warnings.

Warns:
    DeprecationWarning: If using old API
    UserWarning: If input is unusual
"#,
        );

        // Warns become notes
        assert!(doc.notes.len() >= 2);
        assert!(doc.notes.iter().any(|n| n.contains("DeprecationWarning")));
    }

    #[test]
    fn test_parse_google_version_since() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
New feature function.

Version:
    1.2.0

Since:
    2023-01-15
"#,
        );

        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "version" && v == "1.2.0"));
        assert_eq!(doc.since, Some("2023-01-15".to_string()));
    }

    #[test]
    fn test_parse_sphinx_version_since() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
New feature.

:version: 2.0.0
:since: 1.5.0
"#,
        );

        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "version" && v == "2.0.0"));
        assert_eq!(doc.since, Some("1.5.0".to_string()));
    }

    #[test]
    fn test_parse_sphinx_seealso_note_warning() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function summary.

:seealso: other_function
:note: Important note
:warning: Be careful
"#,
        );

        assert_eq!(doc.see_refs.len(), 1);
        assert_eq!(doc.see_refs[0], "other_function");
        assert_eq!(doc.notes.len(), 2);
    }

    #[test]
    fn test_parse_sphinx_example_todo() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function summary.

:example: result = my_func(1, 2)
:todo: Add more examples
"#,
        );

        assert_eq!(doc.examples.len(), 1);
        assert!(doc.examples[0].contains("my_func"));
        assert_eq!(doc.todos.len(), 1);
    }

    #[test]
    fn test_parse_sphinx_var_ivar_cvar() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Class summary.

:ivar name: Instance variable
:cvar count: Class variable
:var value: Generic variable
"#,
        );

        assert!(doc.custom_tags.iter().any(|(k, _)| k == "ivar:name"));
        assert!(doc.custom_tags.iter().any(|(k, _)| k == "cvar:count"));
        assert!(doc.custom_tags.iter().any(|(k, _)| k == "var:value"));
    }

    #[test]
    fn test_parse_sphinx_meta() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function summary.

:meta author: John Doe
:meta license: MIT
"#,
        );

        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "meta:author" && v == "John Doe"));
        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "meta:license" && v == "MIT"));
    }

    #[test]
    fn test_parse_sphinx_keyword_args() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function with kwargs.

:param x: Regular param
:keyword timeout: Timeout in seconds
:kwarg retries: Number of retries
"#,
        );

        assert_eq!(doc.params.len(), 1);

        let kwargs: Vec<_> = doc
            .custom_tags
            .iter()
            .filter(|(k, _)| k == "kwarg")
            .collect();
        assert_eq!(kwargs.len(), 2);
    }

    #[test]
    fn test_parse_numpy_comprehensive() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Calculate the distance between two points.

A longer description that spans
multiple lines.

Parameters
----------
x1 : float
    First x coordinate
y1 : float
    First y coordinate

Returns
-------
float
    The Euclidean distance

Raises
------
ValueError
    If coordinates are invalid

See Also
--------
calculate_angle : Calculates the angle between points

Examples
--------
>>> distance(0, 0, 3, 4)
5.0
"#,
        );

        assert_eq!(
            doc.summary,
            Some("Calculate the distance between two points.".to_string())
        );
        assert_eq!(doc.params.len(), 2);
        assert!(doc.returns.is_some());
        assert_eq!(doc.throws.len(), 1);
        assert!(!doc.see_refs.is_empty());
        assert!(!doc.examples.is_empty());
    }

    #[test]
    fn test_parse_numpy_yields() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Generate items.

Yields
------
int
    The next number
"#,
        );

        assert!(doc
            .custom_tags
            .iter()
            .any(|(k, v)| k == "generator" && v == "true"));
    }

    #[test]
    fn test_parse_numpy_attributes() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
A data container class.

Attributes
----------
data : array-like
    The stored data
shape : tuple
    Shape of the data
"#,
        );

        let attrs: Vec<_> = doc
            .custom_tags
            .iter()
            .filter(|(k, _)| k.starts_with("attr:"))
            .collect();
        assert_eq!(attrs.len(), 2);
    }

    // ========================================
    // to_suggestions conversion tests
    // ========================================

    #[test]
    fn test_to_suggestions_basic() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Process data efficiently.

Args:
    data: Input data

Returns:
    Processed result
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "process", 10);

        // Should have summary
        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Summary
                && s.value.contains("Process data")));
    }

    #[test]
    fn test_to_suggestions_deprecated() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Old function.

Deprecated:
    Use new_function instead
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "old_func", 5);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Deprecated));
    }

    #[test]
    fn test_to_suggestions_raises() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
May raise errors.

Raises:
    ValueError: Bad value
    TypeError: Wrong type
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "risky", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint && s.value.contains("raises")));
    }

    #[test]
    fn test_to_suggestions_generator() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Generate numbers.

Yields:
    int: Next number
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "gen", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint && s.value.contains("generator")));
    }

    #[test]
    fn test_to_suggestions_version_since() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
New feature.

:version: 1.0.0
:since: 0.9.0
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "feature", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint && s.value.contains("version:")));
        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint && s.value.contains("since:")));
    }

    #[test]
    fn test_to_suggestions_kwargs() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function with kwargs.

Keyword Args:
    timeout: Timeout value
    retries: Retry count
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "func", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint
                && s.value.contains("accepts kwargs")));
    }

    #[test]
    fn test_to_suggestions_attributes() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Data class.

Attributes:
    name: The name
    value: The value
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "DataClass", 1);

        assert!(suggestions.iter().any(
            |s| s.annotation_type == AnnotationType::AiHint && s.value.contains("attributes:")
        ));
    }

    #[test]
    fn test_to_suggestions_methods() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Utility class.

Methods:
    process: Process data
    validate: Validate input
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "UtilClass", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint && s.value.contains("methods:")));
    }

    #[test]
    fn test_to_suggestions_instance_class_vars() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Class with variables.

:ivar name: Instance variable
:cvar count: Class variable
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "MyClass", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint
                && s.value.contains("instance vars:")));
        assert!(suggestions.iter().any(
            |s| s.annotation_type == AnnotationType::AiHint && s.value.contains("class vars:")
        ));
    }

    #[test]
    fn test_to_suggestions_meta() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function with meta.

:meta author: Jane Doe
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "func", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::AiHint && s.value.contains("author:")));
    }

    #[test]
    fn test_to_suggestions_see_refs() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function with references.

See Also:
    other_function
    related_module
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "func", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Ref));
    }

    #[test]
    fn test_to_suggestions_todos() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Work in progress.

Todo:
    Finish implementation
"#,
        );

        let suggestions = parser.to_suggestions(&doc, "func", 1);

        assert!(suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Hack));
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
    fn test_multiline_sphinx_content() {
        let parser = DocstringParser::new();
        let doc = parser.parse(
            r#"
Function summary.

:note: This is a note that spans
    multiple lines and should be
    combined into one.
:param x: A parameter
"#,
        );

        assert!(!doc.notes.is_empty());
        assert_eq!(doc.params.len(), 1);
    }
}
