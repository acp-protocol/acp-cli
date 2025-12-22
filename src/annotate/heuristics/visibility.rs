//! @acp:module "Visibility Heuristics"
//! @acp:summary "Infers lock levels from symbol visibility"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Visibility Heuristics
//!
//! Analyzes symbol visibility to infer appropriate lock levels:
//! - Private → restricted (implementation detail)
//! - Protected/Internal → normal (internal API)
//! - Public exported → normal (public API)

use crate::ast::Visibility;

use crate::annotate::{Suggestion, SuggestionSource};

/// @acp:summary "Infers lock levels from symbol visibility"
/// @acp:lock normal
pub struct VisibilityHeuristics;

impl VisibilityHeuristics {
    /// @acp:summary "Creates a new visibility heuristics analyzer"
    pub fn new() -> Self {
        Self
    }

    /// @acp:summary "Generates suggestions based on visibility"
    pub fn suggest(
        &self,
        target: &str,
        line: usize,
        visibility: Visibility,
        is_exported: bool,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        match visibility {
            Visibility::Private => {
                // Private symbols are implementation details
                suggestions.push(
                    Suggestion::lock(target, line, "restricted", SuggestionSource::Heuristic)
                        .with_confidence(0.7),
                );
            }
            Visibility::Protected => {
                // Protected = internal API, needs care
                suggestions.push(
                    Suggestion::lock(target, line, "normal", SuggestionSource::Heuristic)
                        .with_confidence(0.6),
                );
                suggestions.push(
                    Suggestion::ai_hint(target, line, "internal API", SuggestionSource::Heuristic)
                        .with_confidence(0.5),
                );
            }
            Visibility::Internal | Visibility::Crate => {
                // Crate-internal in Rust, package-internal in other languages
                suggestions.push(
                    Suggestion::lock(target, line, "restricted", SuggestionSource::Heuristic)
                        .with_confidence(0.6),
                );
                suggestions.push(
                    Suggestion::ai_hint(target, line, "crate-internal", SuggestionSource::Heuristic)
                        .with_confidence(0.5),
                );
            }
            Visibility::Public => {
                // Public API - if exported, likely important
                if is_exported {
                    suggestions.push(
                        Suggestion::lock(target, line, "normal", SuggestionSource::Heuristic)
                            .with_confidence(0.5),
                    );
                }
            }
        }

        suggestions
    }

    /// @acp:summary "Determines if a symbol should be annotated based on visibility"
    pub fn should_annotate(&self, visibility: Visibility, is_exported: bool) -> bool {
        match visibility {
            // Always annotate public exported symbols
            Visibility::Public if is_exported => true,
            // Annotate protected symbols (internal API)
            Visibility::Protected => true,
            // Annotate internal/crate symbols for documentation
            Visibility::Internal | Visibility::Crate => true,
            // Skip private symbols by default
            Visibility::Private => false,
            // Skip non-exported public symbols
            Visibility::Public => false,
        }
    }

    /// @acp:summary "Suggests a lock level based on visibility and context"
    pub fn suggest_lock_level(&self, visibility: Visibility, is_security_sensitive: bool) -> &'static str {
        if is_security_sensitive {
            return "restricted";
        }

        match visibility {
            Visibility::Private => "restricted",
            Visibility::Protected | Visibility::Internal | Visibility::Crate => "normal",
            Visibility::Public => "normal",
        }
    }
}

impl Default for VisibilityHeuristics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::AnnotationType;

    #[test]
    fn test_suggest_private_visibility() {
        let heuristics = VisibilityHeuristics::new();
        let suggestions = heuristics.suggest("privateFunc", 10, Visibility::Private, false);

        let has_restricted = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Lock && s.value == "restricted"
        });
        assert!(has_restricted);
    }

    #[test]
    fn test_suggest_public_exported() {
        let heuristics = VisibilityHeuristics::new();
        let suggestions = heuristics.suggest("publicFunc", 10, Visibility::Public, true);

        let has_normal = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Lock && s.value == "normal"
        });
        assert!(has_normal);
    }

    #[test]
    fn test_suggest_internal() {
        let heuristics = VisibilityHeuristics::new();
        let suggestions = heuristics.suggest("internalFunc", 10, Visibility::Internal, false);

        let has_restricted = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Lock && s.value == "restricted"
        });
        let has_hint = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::AiHint
        });

        assert!(has_restricted);
        assert!(has_hint);
    }

    #[test]
    fn test_should_annotate() {
        let heuristics = VisibilityHeuristics::new();

        assert!(heuristics.should_annotate(Visibility::Public, true));
        assert!(heuristics.should_annotate(Visibility::Protected, false));
        assert!(heuristics.should_annotate(Visibility::Internal, false));
        assert!(!heuristics.should_annotate(Visibility::Private, false));
        assert!(!heuristics.should_annotate(Visibility::Public, false));
    }

    #[test]
    fn test_suggest_lock_level() {
        let heuristics = VisibilityHeuristics::new();

        assert_eq!(
            heuristics.suggest_lock_level(Visibility::Private, false),
            "restricted"
        );
        assert_eq!(
            heuristics.suggest_lock_level(Visibility::Public, false),
            "normal"
        );
        assert_eq!(
            heuristics.suggest_lock_level(Visibility::Public, true),
            "restricted"
        );
    }
}
