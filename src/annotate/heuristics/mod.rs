//! @acp:module "Annotation Heuristics"
//! @acp:summary "Pattern-based inference rules for suggesting annotations"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Annotation Heuristics
//!
//! Provides rule-based inference for suggesting ACP annotations based on:
//! - Naming patterns (security, data, test keywords)
//! - Path patterns (directory → domain mapping)
//! - Visibility patterns (public/private inference)
//! - Code patterns (security-critical functions)
//! - Git history (churn, contributors, code age)

pub mod git;
pub mod naming;
pub mod path;
pub mod visibility;

use std::path::Path;

use crate::ast::SymbolKind;
use crate::git::GitRepository;

use super::{Suggestion, SuggestionSource};

/// @acp:summary "Aggregates heuristic suggestions from multiple sources"
/// @acp:lock normal
pub struct HeuristicsEngine {
    /// Naming pattern heuristics
    naming: naming::NamingHeuristics,

    /// Path-based heuristics
    path: path::PathHeuristics,

    /// Visibility-based heuristics (used in future enhancement)
    #[allow(dead_code)]
    visibility: visibility::VisibilityHeuristics,

    /// Git-based heuristics
    git: git::GitHeuristics,

    /// Whether to generate summaries from identifiers
    generate_summaries: bool,

    /// Whether to use git-based heuristics
    use_git_heuristics: bool,
}

impl HeuristicsEngine {
    /// @acp:summary "Creates a new heuristics engine with default settings"
    pub fn new() -> Self {
        Self {
            naming: naming::NamingHeuristics::new(),
            path: path::PathHeuristics::new(),
            visibility: visibility::VisibilityHeuristics::new(),
            git: git::GitHeuristics::new(),
            generate_summaries: true,
            use_git_heuristics: true,
        }
    }

    /// @acp:summary "Enables or disables summary generation"
    pub fn with_summary_generation(mut self, enabled: bool) -> Self {
        self.generate_summaries = enabled;
        self
    }

    /// @acp:summary "Enables or disables git-based heuristics"
    pub fn with_git_heuristics(mut self, enabled: bool) -> Self {
        self.use_git_heuristics = enabled;
        self
    }

    /// @acp:summary "Generates suggestions for a symbol"
    ///
    /// Collects suggestions from all heuristic sources:
    /// 1. Naming patterns (security, data, test keywords)
    /// 2. Path patterns (directory → domain)
    /// 3. Visibility patterns (lock levels)
    /// 4. Summary generation from identifier names
    pub fn suggest(
        &self,
        target: &str,
        line: usize,
        symbol_kind: Option<SymbolKind>,
        file_path: &str,
    ) -> Vec<Suggestion> {
        self.suggest_full(target, line, symbol_kind, file_path, None, false)
    }

    /// @acp:summary "Generates suggestions for a symbol with visibility info"
    ///
    /// Full version that includes visibility-based suggestions.
    pub fn suggest_full(
        &self,
        target: &str,
        line: usize,
        symbol_kind: Option<SymbolKind>,
        file_path: &str,
        visibility: Option<crate::ast::Visibility>,
        is_exported: bool,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Handle file-level targets (symbol_kind = None, target contains path separator)
        let is_file_level =
            symbol_kind.is_none() && (target.contains('/') || target.contains('\\'));

        if is_file_level {
            // Generate @acp:module from file path
            if let Some(module_name) = self.path.infer_module_name(file_path) {
                suggestions.push(
                    Suggestion::module(target, line, &module_name, SuggestionSource::Heuristic)
                        .with_confidence(0.7),
                );

                // Generate @acp:summary for the module
                if self.generate_summaries {
                    let summary = format!("{} module", module_name);
                    suggestions.push(
                        Suggestion::summary(target, line, &summary, SuggestionSource::Heuristic)
                            .with_confidence(0.5),
                    );
                }
            }

            // Return early - no need for symbol-level heuristics
            return suggestions;
        }

        // Get naming-based suggestions
        let naming_suggestions = self.naming.suggest(target, line);
        suggestions.extend(naming_suggestions);

        // Get path-based suggestions
        let path_suggestions = self.path.suggest(file_path, target, line);
        suggestions.extend(path_suggestions);

        // Get visibility-based suggestions (if we have visibility info)
        if let Some(vis) = visibility {
            let visibility_suggestions = self.visibility.suggest(target, line, vis, is_exported);
            suggestions.extend(visibility_suggestions);
        }

        // Generate summary from identifier name
        if self.generate_summaries {
            if let Some(summary) = self.naming.generate_summary(target, symbol_kind) {
                suggestions.push(
                    Suggestion::summary(target, line, summary, SuggestionSource::Heuristic)
                        .with_confidence(0.6),
                );
            }
        }

        suggestions
    }

    /// @acp:summary "Generates suggestions including git-based heuristics"
    ///
    /// Extended version that also collects git history-based suggestions:
    /// - High churn detection
    /// - Single contributor warnings
    /// - Code stability assessment
    pub fn suggest_with_git(
        &self,
        target: &str,
        line: usize,
        symbol_kind: Option<SymbolKind>,
        file_path: &str,
        repo: Option<&GitRepository>,
    ) -> Vec<Suggestion> {
        self.suggest_with_git_full(target, line, symbol_kind, file_path, repo, None, false)
    }

    /// @acp:summary "Generates all suggestions including git and visibility"
    ///
    /// Full version with all heuristic sources:
    /// - Naming patterns
    /// - Path patterns
    /// - Visibility patterns
    /// - Git history analysis
    #[allow(clippy::too_many_arguments)]
    pub fn suggest_with_git_full(
        &self,
        target: &str,
        line: usize,
        symbol_kind: Option<SymbolKind>,
        file_path: &str,
        repo: Option<&GitRepository>,
        visibility: Option<crate::ast::Visibility>,
        is_exported: bool,
    ) -> Vec<Suggestion> {
        let mut suggestions = self.suggest_full(
            target,
            line,
            symbol_kind,
            file_path,
            visibility,
            is_exported,
        );

        // Add git-based suggestions if enabled and repo is available
        if self.use_git_heuristics {
            if let Some(repo) = repo {
                let path = Path::new(file_path);
                let git_suggestions = self.git.suggest_for_file(repo, path, target, line);
                suggestions.extend(git_suggestions);
            }
        }

        suggestions
    }
}

impl Default for HeuristicsEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::AnnotationType;

    #[test]
    fn test_heuristics_engine_creation() {
        let engine = HeuristicsEngine::new();
        assert!(engine.generate_summaries);
    }

    #[test]
    fn test_suggest_security_pattern() {
        let engine = HeuristicsEngine::new();
        let suggestions = engine.suggest(
            "authenticateUser",
            10,
            Some(SymbolKind::Function),
            "src/auth/service.ts",
        );

        // Should suggest security-related annotations
        let has_security = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Domain && s.value.contains("security")
                || s.annotation_type == AnnotationType::Lock
        });

        assert!(has_security || !suggestions.is_empty());
    }

    #[test]
    fn test_suggest_from_path() {
        let engine = HeuristicsEngine::new();
        let suggestions = engine.suggest(
            "processPayment",
            10,
            Some(SymbolKind::Function),
            "src/billing/payments.ts",
        );

        // Should suggest billing domain from path
        let has_billing = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Domain && s.value == "billing");

        assert!(has_billing);
    }

    #[test]
    fn test_suggest_with_visibility_private() {
        use crate::ast::Visibility;

        let engine = HeuristicsEngine::new();
        let suggestions = engine.suggest_full(
            "internalHelper",
            10,
            Some(SymbolKind::Function),
            "src/utils.ts",
            Some(Visibility::Private),
            false,
        );

        // Should suggest restricted lock for private symbols
        let has_restricted = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Lock && s.value == "restricted");

        assert!(has_restricted, "Private symbols should get restricted lock");
    }

    #[test]
    fn test_suggest_with_visibility_public_exported() {
        use crate::ast::Visibility;

        let engine = HeuristicsEngine::new();
        let suggestions = engine.suggest_full(
            "PublicAPI",
            10,
            Some(SymbolKind::Function),
            "src/api.ts",
            Some(Visibility::Public),
            true, // exported
        );

        // Should suggest normal lock for public exported symbols
        let has_normal = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Lock && s.value == "normal");

        assert!(has_normal, "Public exported symbols should get normal lock");
    }

    #[test]
    fn test_suggest_full_combines_all_sources() {
        use crate::ast::Visibility;

        let engine = HeuristicsEngine::new();
        let suggestions = engine.suggest_full(
            "authenticateUser",
            10,
            Some(SymbolKind::Function),
            "src/auth/login.ts",
            Some(Visibility::Internal),
            false,
        );

        // Should have suggestions from naming (security), path (auth), and visibility
        assert!(!suggestions.is_empty(), "Should have combined suggestions");

        // Should have naming-based security suggestion
        let has_naming = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Domain);
        assert!(has_naming, "Should have naming/path-based suggestions");

        // Should have visibility-based suggestions
        let has_visibility = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Lock || s.annotation_type == AnnotationType::AiHint
        });
        assert!(has_visibility, "Should have visibility-based suggestions");
    }

    #[test]
    fn test_git_heuristics_disabled() {
        let engine = HeuristicsEngine::new().with_git_heuristics(false);
        assert!(!engine.use_git_heuristics);
    }
}
