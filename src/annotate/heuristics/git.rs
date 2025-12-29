//! @acp:module "Git Heuristics"
//! @acp:summary "Suggests annotations based on git history and file metadata"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Git Heuristics
//!
//! Provides annotation suggestions based on git history:
//! - Lock suggestions based on file churn and contributor patterns
//! - Stability suggestions based on code age
//! - AI hints with file context information

use std::path::Path;

use chrono::Utc;

use crate::git::{FileHistory, GitRepository};

use super::super::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Git-based heuristics for annotation suggestions"
pub struct GitHeuristics {
    /// Threshold for high-churn files (commits in last 100 commits of file)
    high_churn_threshold: usize,

    /// Days without modification to consider stable
    stable_age_days: i64,

    /// Minimum contributors to not flag as single-author
    min_contributors: usize,
}

impl GitHeuristics {
    /// @acp:summary "Creates git heuristics with default thresholds"
    pub fn new() -> Self {
        Self {
            high_churn_threshold: 20,
            stable_age_days: 180, // 6 months
            min_contributors: 2,
        }
    }

    /// @acp:summary "Generates suggestions based on file's git history"
    ///
    /// Returns suggestions for:
    /// - Lock levels based on churn and contributors
    /// - Stability based on code age
    /// - AI hints with context
    pub fn suggest_for_file(
        &self,
        repo: &GitRepository,
        file_path: &Path,
        target: &str,
        line: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Get file history
        let history = match FileHistory::for_file(repo, file_path, 100) {
            Ok(h) => h,
            Err(_) => return suggestions, // Can't get history, skip git heuristics
        };

        let commit_count = history.commit_count();
        let contributors = history.contributors();
        let contributor_count = contributors.len();

        // Check for high churn
        if commit_count >= self.high_churn_threshold {
            suggestions.push(
                Suggestion::new(
                    target,
                    line,
                    AnnotationType::AiHint,
                    format!(
                        "High-churn file: {} commits by {} contributors",
                        commit_count, contributor_count
                    ),
                    SuggestionSource::Heuristic,
                )
                .with_confidence(0.7),
            );
        }

        // Check for single contributor (bus factor = 1)
        if contributor_count < self.min_contributors && commit_count > 5 {
            let author = contributors
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            suggestions.push(
                Suggestion::new(
                    target,
                    line,
                    AnnotationType::AiHint,
                    format!(
                        "Single author: {}. Consider code review for bus factor.",
                        author
                    ),
                    SuggestionSource::Heuristic,
                )
                .with_confidence(0.6),
            );
        }

        // Check code age for stability suggestions
        if let Some(latest) = history.latest() {
            let days_since_modification = (Utc::now() - latest.timestamp).num_days();

            if days_since_modification >= self.stable_age_days {
                suggestions.push(
                    Suggestion::new(
                        target,
                        line,
                        AnnotationType::Stability,
                        "stable".to_string(),
                        SuggestionSource::Heuristic,
                    )
                    .with_confidence(0.7),
                );

                suggestions.push(
                    Suggestion::new(
                        target,
                        line,
                        AnnotationType::AiHint,
                        format!(
                            "Stable code: unchanged for {} days",
                            days_since_modification
                        ),
                        SuggestionSource::Heuristic,
                    )
                    .with_confidence(0.7),
                );
            }
        }

        // Calculate churn ratio (adds + deletes) for lock suggestion
        let total_churn = history.total_lines_added() + history.total_lines_removed();
        if total_churn > 500 && commit_count > 10 {
            // High total churn suggests volatile code
            suggestions.push(
                Suggestion::new(
                    target,
                    line,
                    AnnotationType::Lock,
                    "normal".to_string(),
                    SuggestionSource::Heuristic,
                )
                .with_confidence(0.6),
            );

            suggestions.push(
                Suggestion::new(
                    target,
                    line,
                    AnnotationType::AiHint,
                    format!(
                        "Volatile code: {} lines changed across {} commits",
                        total_churn, commit_count
                    ),
                    SuggestionSource::Heuristic,
                )
                .with_confidence(0.6),
            );
        }

        suggestions
    }
}

impl Default for GitHeuristics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_git_heuristics_creation() {
        let heuristics = GitHeuristics::new();
        assert_eq!(heuristics.high_churn_threshold, 20);
        assert_eq!(heuristics.stable_age_days, 180);
    }

    #[test]
    fn test_git_heuristics_on_real_file() {
        let cwd = env::current_dir().unwrap();

        // Try to find the repo root
        let repo = match GitRepository::open(&cwd) {
            Ok(r) => r,
            Err(_) => return, // Skip if not in a git repo
        };

        let heuristics = GitHeuristics::new();

        // Test on a file that should exist
        let cargo_path = cwd.join("Cargo.toml");
        if cargo_path.exists() {
            let suggestions = heuristics.suggest_for_file(&repo, &cargo_path, "Cargo.toml", 1);

            // We should get some suggestions (or none if file is very new)
            // The test passes if it doesn't crash
            let _ = suggestions;
        }
    }
}
