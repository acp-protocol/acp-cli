//! @acp:module "Annotation Writer"
//! @acp:summary "Generates diffs and applies annotation changes to source files"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Annotation Writer
//!
//! Provides functionality for:
//! - Generating unified diffs for preview mode
//! - Applying annotations to source files
//! - Handling comment syntax for different languages
//! - Preserving existing documentation

use std::collections::HashSet;
use std::path::Path;

use similar::TextDiff;

use crate::error::Result;

use super::{AnalysisResult, FileChange, Suggestion};

/// @acp:summary "Comment style for different languages"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    /// JSDoc: /** ... */
    JsDoc,
    /// Python docstring: """..."""
    PyDocstring,
    /// Rust doc: ///
    RustDoc,
    /// Rust module doc: //!
    RustModuleDoc,
    /// Go: //
    GoDoc,
    /// Javadoc: /** ... */
    Javadoc,
}

impl CommentStyle {
    /// @acp:summary "Determines comment style from language and context"
    pub fn from_language(language: &str, is_module_level: bool) -> Self {
        match language {
            "typescript" | "javascript" => Self::JsDoc,
            "python" => Self::PyDocstring,
            "rust" => {
                if is_module_level {
                    Self::RustModuleDoc
                } else {
                    Self::RustDoc
                }
            }
            "go" => Self::GoDoc,
            "java" => Self::Javadoc,
            _ => Self::JsDoc, // Default to JSDoc style
        }
    }

    /// @acp:summary "Formats annotations into a comment block"
    pub fn format_annotations(&self, annotations: &[Suggestion], indent: &str) -> String {
        if annotations.is_empty() {
            return String::new();
        }

        match self {
            Self::JsDoc | Self::Javadoc => {
                let mut lines = vec![format!("{}/**", indent)];
                for ann in annotations {
                    lines.push(format!("{} * {}", indent, ann.to_annotation_string()));
                }
                lines.push(format!("{} */", indent));
                lines.join("\n")
            }
            Self::PyDocstring => {
                let mut lines = vec![format!("{}\"\"\"", indent)];
                for ann in annotations {
                    lines.push(format!("{}{}", indent, ann.to_annotation_string()));
                }
                lines.push(format!("{}\"\"\"", indent));
                lines.join("\n")
            }
            Self::RustDoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{}/// {}", indent, ann.to_annotation_string()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::RustModuleDoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{}//! {}", indent, ann.to_annotation_string()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::GoDoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{}// {}", indent, ann.to_annotation_string()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }

    /// @acp:summary "Formats annotations for insertion into existing doc comment"
    /// Places ACP annotations at the beginning of the comment.
    pub fn format_for_insertion(&self, annotations: &[Suggestion], indent: &str) -> Vec<String> {
        match self {
            Self::JsDoc | Self::Javadoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{} * {}", indent, ann.to_annotation_string()))
                    .collect()
            }
            Self::PyDocstring => {
                annotations
                    .iter()
                    .map(|ann| format!("{}{}", indent, ann.to_annotation_string()))
                    .collect()
            }
            Self::RustDoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{}/// {}", indent, ann.to_annotation_string()))
                    .collect()
            }
            Self::RustModuleDoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{}//! {}", indent, ann.to_annotation_string()))
                    .collect()
            }
            Self::GoDoc => {
                annotations
                    .iter()
                    .map(|ann| format!("{}// {}", indent, ann.to_annotation_string()))
                    .collect()
            }
        }
    }
}

/// @acp:summary "Writes annotations to files and generates diffs"
/// @acp:lock normal
pub struct Writer {
    /// Whether to preserve existing documentation
    preserve_existing: bool,
}

impl Writer {
    /// @acp:summary "Creates a new writer"
    pub fn new() -> Self {
        Self {
            preserve_existing: true,
        }
    }

    /// @acp:summary "Sets whether to preserve existing documentation"
    pub fn with_preserve_existing(mut self, preserve: bool) -> Self {
        self.preserve_existing = preserve;
        self
    }

    /// @acp:summary "Plans changes to apply to a file"
    ///
    /// Groups suggestions by target and line, creating FileChange entries
    /// that can be used for diff generation or application.
    pub fn plan_changes(
        &self,
        file_path: &Path,
        suggestions: &[Suggestion],
        analysis: &AnalysisResult,
    ) -> Result<Vec<FileChange>> {
        let mut changes: Vec<FileChange> = Vec::new();
        let path_str = file_path.to_string_lossy().to_string();

        // Group suggestions by target
        let mut by_target: std::collections::HashMap<String, Vec<&Suggestion>> =
            std::collections::HashMap::new();

        for suggestion in suggestions {
            by_target
                .entry(suggestion.target.clone())
                .or_default()
                .push(suggestion);
        }

        // Create FileChange for each target
        for (target, target_suggestions) in by_target {
            if target_suggestions.is_empty() {
                continue;
            }

            let line = target_suggestions[0].line;
            let is_file_level = target_suggestions[0].is_file_level();

            let mut change = FileChange::new(&path_str, line);

            if !is_file_level {
                change = change.with_symbol(&target);
            }

            // Find existing doc comment for this target
            if let Some(gap) = analysis.gaps.iter().find(|g| g.target == target) {
                if gap.doc_comment.is_some() {
                    if let Some((start, end)) = gap.doc_comment_range {
                        // Use the actual doc comment line range
                        change = change.with_existing_doc(start, end);
                    } else if line > 1 {
                        // Fallback: assume doc comment is just the line before symbol
                        change = change.with_existing_doc(line - 1, line - 1);
                    }
                }
            }

            // Add all suggestions
            for suggestion in target_suggestions {
                change.add_annotation(suggestion.clone());
            }

            changes.push(change);
        }

        // Sort by line number (descending for bottom-up application)
        changes.sort_by(|a, b| b.line.cmp(&a.line));

        Ok(changes)
    }

    /// @acp:summary "Generates a unified diff for preview"
    pub fn generate_diff(&self, file_path: &Path, changes: &[FileChange]) -> Result<String> {
        let original = std::fs::read_to_string(file_path)?;
        let modified = self.apply_to_content(&original, changes, &self.detect_language(file_path))?;

        let diff = generate_unified_diff(
            &file_path.to_string_lossy(),
            &original,
            &modified,
        );

        Ok(diff)
    }

    /// @acp:summary "Applies changes to file content"
    fn apply_to_content(
        &self,
        content: &str,
        changes: &[FileChange],
        language: &str,
    ) -> Result<String> {
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Sort changes by line (descending) to apply from bottom to top
        let mut sorted_changes = changes.to_vec();
        sorted_changes.sort_by(|a, b| b.line.cmp(&a.line));

        for change in &sorted_changes {
            let is_module_level = change.symbol_name.is_none();
            let style = CommentStyle::from_language(language, is_module_level);

            // Detect indentation from the target line
            let indent = if change.line > 0 && change.line <= lines.len() {
                let target_line = &lines[change.line - 1];
                let trimmed = target_line.trim_start();
                &target_line[..target_line.len() - trimmed.len()]
            } else {
                ""
            };

            if change.existing_doc_start.is_some() {
                // Insert into existing doc comment
                // Place ACP annotations after the opening line
                let insert_line = change.existing_doc_start.unwrap();
                let doc_end = change.existing_doc_end.unwrap_or(insert_line + 20);

                // Check for existing @acp: annotations in the doc comment range
                let existing_in_range: HashSet<String> = lines
                    [insert_line.saturating_sub(1)..doc_end.min(lines.len())]
                    .iter()
                    .filter_map(|line| {
                        if line.contains("@acp:") {
                            // Extract the annotation type and value for comparison
                            // e.g., "@acp:summary \"something\"" -> "@acp:summary"
                            let trimmed = line.trim();
                            if let Some(start) = trimmed.find("@acp:") {
                                let ann_part = &trimmed[start..];
                                // Get just the annotation type (e.g., "@acp:summary")
                                let type_end = ann_part
                                    .find(|c: char| c.is_whitespace() || c == '"')
                                    .unwrap_or(ann_part.len());
                                Some(ann_part[..type_end].to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                // Filter out annotations that already exist (by type)
                let new_annotations: Vec<_> = change
                    .annotations
                    .iter()
                    .filter(|ann| {
                        let ann_type = format!("@acp:{}", ann.annotation_type.namespace());
                        !existing_in_range.contains(&ann_type)
                    })
                    .cloned()
                    .collect();

                if new_annotations.is_empty() {
                    continue; // Nothing new to add, skip this change
                }

                let annotation_lines = style.format_for_insertion(&new_annotations, indent);

                for (i, ann_line) in annotation_lines.into_iter().enumerate() {
                    let insert_at = insert_line + i; // After the opening line
                    if insert_at <= lines.len() {
                        lines.insert(insert_at, ann_line);
                    }
                }
            } else {
                // Create new doc comment before the target line
                let comment_block = style.format_annotations(&change.annotations, indent);

                if !comment_block.is_empty() {
                    let insert_at = if change.line > 0 {
                        change.line - 1
                    } else {
                        0
                    };

                    // Insert comment block lines
                    for (i, line) in comment_block.lines().enumerate() {
                        lines.insert(insert_at + i, line.to_string());
                    }
                }
            }
        }

        Ok(lines.join("\n"))
    }

    /// @acp:summary "Applies changes to a file on disk"
    pub fn apply_changes(&self, file_path: &Path, changes: &[FileChange]) -> Result<()> {
        let content = std::fs::read_to_string(file_path)?;
        let language = self.detect_language(file_path);
        let modified = self.apply_to_content(&content, changes, &language)?;

        std::fs::write(file_path, modified)?;
        Ok(())
    }

    /// @acp:summary "Detects language from file extension"
    fn detect_language(&self, path: &Path) -> String {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext {
                "ts" | "tsx" => "typescript",
                "js" | "jsx" | "mjs" | "cjs" => "javascript",
                "py" | "pyi" => "python",
                "rs" => "rust",
                "go" => "go",
                "java" => "java",
                _ => "unknown",
            })
            .unwrap_or("unknown")
            .to_string()
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

/// @acp:summary "Generates a unified diff between original and modified content"
pub fn generate_unified_diff(file_path: &str, original: &str, modified: &str) -> String {
    let diff = TextDiff::from_lines(original, modified);

    // Use the built-in unified diff formatter
    diff.unified_diff()
        .context_radius(3)
        .header(&format!("a/{}", file_path), &format!("b/{}", file_path))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::SuggestionSource;

    #[test]
    fn test_comment_style_from_language() {
        assert_eq!(
            CommentStyle::from_language("typescript", false),
            CommentStyle::JsDoc
        );
        assert_eq!(
            CommentStyle::from_language("python", false),
            CommentStyle::PyDocstring
        );
        assert_eq!(
            CommentStyle::from_language("rust", false),
            CommentStyle::RustDoc
        );
        assert_eq!(
            CommentStyle::from_language("rust", true),
            CommentStyle::RustModuleDoc
        );
    }

    #[test]
    fn test_format_annotations_jsdoc() {
        let annotations = vec![
            Suggestion::summary("test", 1, "Test summary", SuggestionSource::Heuristic),
            Suggestion::domain("test", 1, "authentication", SuggestionSource::Heuristic),
        ];

        let formatted = CommentStyle::JsDoc.format_annotations(&annotations, "");

        assert!(formatted.contains("/**"));
        assert!(formatted.contains("@acp:summary \"Test summary\""));
        assert!(formatted.contains("@acp:domain authentication"));
        assert!(formatted.contains(" */"));
    }

    #[test]
    fn test_format_annotations_rust() {
        let annotations = vec![
            Suggestion::summary("test", 1, "Test summary", SuggestionSource::Heuristic),
        ];

        let formatted = CommentStyle::RustDoc.format_annotations(&annotations, "");
        assert!(formatted.contains("/// @acp:summary \"Test summary\""));

        let formatted_module = CommentStyle::RustModuleDoc.format_annotations(&annotations, "");
        assert!(formatted_module.contains("//! @acp:summary \"Test summary\""));
    }

    #[test]
    fn test_generate_unified_diff() {
        let original = "line 1\nline 2\nline 3";
        let modified = "line 1\nnew line\nline 2\nline 3";

        let diff = generate_unified_diff("test.txt", original, modified);

        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("+++ b/test.txt"));
        assert!(diff.contains("+new line"));
    }
}
