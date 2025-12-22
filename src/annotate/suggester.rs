//! @acp:module "Annotation Suggester"
//! @acp:summary "Generates annotation suggestions by merging heuristics and converted docs"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Annotation Suggester
//!
//! Combines multiple sources of annotation suggestions:
//! - Converted documentation (JSDoc, docstrings, etc.)
//! - Heuristics (naming patterns, visibility, path)
//! - Existing annotations (preserved with highest priority)
//!
//! Uses strict priority ordering: Explicit > Converted > Heuristic

use std::collections::HashMap;

use crate::git::GitRepository;

use super::converters::DocStandardParser;
use super::heuristics::HeuristicsEngine;
use super::{
    AnalysisResult, AnnotateLevel, AnnotationType, ConversionSource, Suggestion,
};

/// @acp:summary "Generates and merges annotation suggestions"
/// @acp:lock normal
pub struct Suggester {
    /// Annotation level for filtering suggestions
    level: AnnotateLevel,

    /// Conversion source (auto-detect or specific)
    conversion_source: ConversionSource,

    /// Whether to include heuristic suggestions
    use_heuristics: bool,

    /// Heuristics engine
    heuristics: HeuristicsEngine,
}

impl Suggester {
    /// @acp:summary "Creates a new suggester with default settings"
    pub fn new(level: AnnotateLevel) -> Self {
        Self {
            level,
            conversion_source: ConversionSource::Auto,
            use_heuristics: true,
            heuristics: HeuristicsEngine::new(),
        }
    }

    /// @acp:summary "Sets the conversion source"
    pub fn with_conversion_source(mut self, source: ConversionSource) -> Self {
        self.conversion_source = source;
        self
    }

    /// @acp:summary "Enables or disables heuristic suggestions"
    pub fn with_heuristics(mut self, enabled: bool) -> Self {
        self.use_heuristics = enabled;
        self
    }

    /// @acp:summary "Generates suggestions for an analyzed file"
    ///
    /// Processes the analysis result and generates suggestions from:
    /// 1. Converted documentation (if doc comments exist)
    /// 2. Heuristics (naming, path, visibility patterns)
    ///
    /// Suggestions are merged using strict priority ordering.
    pub fn suggest(&self, analysis: &AnalysisResult) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Process each gap
        for gap in &analysis.gaps {
            let mut gap_suggestions = Vec::new();

            // Try to convert existing doc comments
            if let Some(doc_comment) = &gap.doc_comment {
                let source = self.get_conversion_source(&analysis.language);
                if let Some(parser) = self.get_parser(source) {
                    let parsed = parser.parse(doc_comment);
                    let converted = parser.to_suggestions(&parsed, &gap.target, gap.line);
                    gap_suggestions.extend(converted);
                }
            }

            // Add heuristic suggestions
            if self.use_heuristics {
                let heuristic_suggestions = self.heuristics.suggest(
                    &gap.target,
                    gap.line,
                    gap.symbol_kind,
                    &analysis.file_path,
                );
                gap_suggestions.extend(heuristic_suggestions);
            }

            // Filter by level and merge by priority
            let filtered = self.filter_and_merge(gap_suggestions);
            suggestions.extend(filtered);
        }

        suggestions
    }

    /// @acp:summary "Generates suggestions with git-based heuristics"
    ///
    /// Extended version that includes git history analysis:
    /// - High churn detection
    /// - Single contributor warnings
    /// - Code stability assessment
    pub fn suggest_with_git(
        &self,
        analysis: &AnalysisResult,
        repo: Option<&GitRepository>,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Process each gap
        for gap in &analysis.gaps {
            let mut gap_suggestions = Vec::new();

            // Try to convert existing doc comments
            if let Some(doc_comment) = &gap.doc_comment {
                let source = self.get_conversion_source(&analysis.language);
                if let Some(parser) = self.get_parser(source) {
                    let parsed = parser.parse(doc_comment);
                    let converted = parser.to_suggestions(&parsed, &gap.target, gap.line);
                    gap_suggestions.extend(converted);
                }
            }

            // Add heuristic suggestions (including git-based if repo is provided)
            if self.use_heuristics {
                let heuristic_suggestions = self.heuristics.suggest_with_git_full(
                    &gap.target,
                    gap.line,
                    gap.symbol_kind,
                    &analysis.file_path,
                    repo,
                    gap.visibility,
                    gap.is_exported,
                );
                gap_suggestions.extend(heuristic_suggestions);
            }

            // Filter by level and merge by priority
            let filtered = self.filter_and_merge(gap_suggestions);
            suggestions.extend(filtered);
        }

        suggestions
    }

    /// @acp:summary "Gets the appropriate conversion source for a language"
    fn get_conversion_source(&self, language: &str) -> ConversionSource {
        match self.conversion_source {
            ConversionSource::Auto => ConversionSource::for_language(language),
            other => other,
        }
    }

    /// @acp:summary "Gets the appropriate doc parser for a conversion source"
    fn get_parser(&self, source: ConversionSource) -> Option<Box<dyn DocStandardParser>> {
        use super::converters::{DocstringParser, JsDocParser, RustdocParser, GodocParser, JavadocParser};

        match source {
            ConversionSource::Jsdoc | ConversionSource::Tsdoc => {
                Some(Box::new(JsDocParser::new()))
            }
            ConversionSource::Docstring => {
                Some(Box::new(DocstringParser::new()))
            }
            ConversionSource::Rustdoc => {
                Some(Box::new(RustdocParser::new()))
            }
            ConversionSource::Godoc => {
                Some(Box::new(GodocParser::new()))
            }
            ConversionSource::Javadoc => {
                Some(Box::new(JavadocParser::new()))
            }
            ConversionSource::Auto => None,
        }
    }

    /// @acp:summary "Filters suggestions by level and merges by priority"
    ///
    /// For each annotation type, only the highest-priority suggestion is kept.
    /// Priority order: Explicit > Converted > Heuristic
    fn filter_and_merge(&self, suggestions: Vec<Suggestion>) -> Vec<Suggestion> {
        // Group by (target, annotation_type)
        let mut by_key: HashMap<(String, AnnotationType), Vec<Suggestion>> = HashMap::new();

        for suggestion in suggestions {
            // Filter by level
            if !self.level.includes(suggestion.annotation_type) {
                continue;
            }

            let key = (suggestion.target.clone(), suggestion.annotation_type);
            by_key.entry(key).or_default().push(suggestion);
        }

        // For each group, keep only the highest priority suggestion
        let mut merged = Vec::new();
        for (_, mut group) in by_key {
            // Sort by source priority (lower is better)
            group.sort_by_key(|s| s.source);

            // Take the first (highest priority)
            if let Some(best) = group.into_iter().next() {
                merged.push(best);
            }
        }

        // Sort by line number for consistent output
        merged.sort_by_key(|s| (s.line, s.annotation_type.namespace()));

        merged
    }
}

impl Default for Suggester {
    fn default() -> Self {
        Self::new(AnnotateLevel::Standard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::SuggestionSource;

    #[test]
    fn test_suggester_creation() {
        let suggester = Suggester::new(AnnotateLevel::Minimal);
        assert_eq!(suggester.level, AnnotateLevel::Minimal);
        assert!(suggester.use_heuristics);
    }

    #[test]
    fn test_filter_and_merge_priority() {
        let suggester = Suggester::new(AnnotateLevel::Standard);

        let suggestions = vec![
            Suggestion::summary("target", 1, "heuristic", SuggestionSource::Heuristic),
            Suggestion::summary("target", 1, "converted", SuggestionSource::Converted),
        ];

        let merged = suggester.filter_and_merge(suggestions);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, "converted"); // Converted has higher priority
    }

    #[test]
    fn test_filter_by_level() {
        let suggester = Suggester::new(AnnotateLevel::Minimal);

        let suggestions = vec![
            Suggestion::summary("target", 1, "summary", SuggestionSource::Heuristic),
            Suggestion::domain("target", 1, "domain", SuggestionSource::Heuristic),
        ];

        let merged = suggester.filter_and_merge(suggestions);

        // Minimal level doesn't include domain
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].annotation_type, AnnotationType::Summary);
    }

    #[test]
    fn test_get_conversion_source() {
        let suggester = Suggester::new(AnnotateLevel::Standard);

        assert_eq!(
            suggester.get_conversion_source("typescript"),
            ConversionSource::Tsdoc
        );
        assert_eq!(
            suggester.get_conversion_source("python"),
            ConversionSource::Docstring
        );

        let with_specific = suggester.with_conversion_source(ConversionSource::Jsdoc);
        assert_eq!(
            with_specific.get_conversion_source("typescript"),
            ConversionSource::Jsdoc
        );
    }
}
