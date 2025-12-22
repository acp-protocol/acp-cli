//! @acp:module "Constraint Types"
//! @acp:summary "Core constraint types for AI behavioral guardrails"
//! @acp:domain cli
//! @acp:layer model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// @acp:summary "Complete constraint set for a scope (RFC-001 compliant)"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Constraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleConstraint>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation: Option<MutationConstraint>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<BehaviorModifier>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityGate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<DeprecationInfo>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Reference>,

    /// RFC-001: Aggregated self-documenting directive from annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,

    /// RFC-001: Whether directive was auto-generated
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_generated: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Constraints {
    /// Merge with another constraint set (other takes precedence)
    /// RFC-001: Aggregates directives when merging
    pub fn merge(&self, other: &Constraints) -> Constraints {
        Constraints {
            style: other.style.clone().or_else(|| self.style.clone()),
            mutation: other.mutation.clone().or_else(|| self.mutation.clone()),
            behavior: other.behavior.clone().or_else(|| self.behavior.clone()),
            quality: other.quality.clone().or_else(|| self.quality.clone()),
            deprecation: other.deprecation.clone().or_else(|| self.deprecation.clone()),
            references: {
                let mut refs = self.references.clone();
                refs.extend(other.references.clone());
                refs
            },
            // RFC-001: Aggregate directives - more specific (other) takes precedence
            directive: other.directive.clone().or_else(|| self.directive.clone()),
            auto_generated: other.directive.is_some() && other.auto_generated ||
                            other.directive.is_none() && self.auto_generated,
        }
    }

    /// Check if AI is allowed to modify based on these constraints
    pub fn can_modify(&self, operation: &str) -> ModifyPermission {
        if let Some(mutation) = &self.mutation {
            match mutation.level {
                LockLevel::Frozen => {
                    return ModifyPermission::Denied {
                        reason: "Code is frozen and cannot be modified".to_string(),
                    };
                }
                LockLevel::Restricted => {
                    if let Some(allowed) = &mutation.allowed_operations {
                        if !allowed.iter().any(|op| op == operation) {
                            return ModifyPermission::Denied {
                                reason: format!("Operation '{}' not allowed. Allowed: {:?}", operation, allowed),
                            };
                        }
                    }
                    return ModifyPermission::RequiresApproval {
                        reason: "Code is restricted".to_string(),
                    };
                }
                LockLevel::ApprovalRequired => {
                    return ModifyPermission::RequiresApproval {
                        reason: mutation.reason.clone().unwrap_or_default(),
                    };
                }
                _ => {}
            }
        }

        ModifyPermission::Allowed
    }

    /// Get requirements that must be met for changes
    pub fn get_requirements(&self) -> Vec<String> {
        let mut reqs = Vec::new();

        if let Some(mutation) = &self.mutation {
            if mutation.requires_tests {
                reqs.push("tests".to_string());
            }
            if mutation.requires_docs {
                reqs.push("documentation".to_string());
            }
            if mutation.requires_approval {
                reqs.push("approval".to_string());
            }
        }

        if let Some(quality) = &self.quality {
            if quality.tests_required {
                reqs.push("tests".to_string());
            }
            if quality.security_review {
                reqs.push("security-review".to_string());
            }
        }

        reqs.sort();
        reqs.dedup();
        reqs
    }
}

/// @acp:summary "Result of checking modification permission"
#[derive(Debug, Clone)]
pub enum ModifyPermission {
    Allowed,
    RequiresApproval { reason: String },
    Denied { reason: String },
}

/// @acp:summary "Style/formatting constraints"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConstraint {
    /// Style guide identifier (e.g., "tailwindcss-v4", "google-python")
    pub guide: String,

    /// URL to authoritative documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,

    /// Specific rules to follow
    #[serde(default)]
    pub rules: Vec<String>,

    /// Linter config file to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linter: Option<String>,
}

/// @acp:summary "Mutation/modification constraints"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationConstraint {
    /// Lock level
    #[serde(default)]
    pub level: LockLevel,

    /// Reason for restriction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Contact for questions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,

    /// Requires human approval
    #[serde(default)]
    pub requires_approval: bool,

    /// Requires tests for changes
    #[serde(default)]
    pub requires_tests: bool,

    /// Requires documentation updates
    #[serde(default)]
    pub requires_docs: bool,

    /// Maximum lines AI can change at once
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lines_changed: Option<usize>,

    /// Operations that are allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_operations: Option<Vec<String>>,

    /// Operations that are forbidden
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_operations: Option<Vec<String>>,
}

/// @acp:summary "Lock level for code modification"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LockLevel {
    /// Cannot be modified under any circumstances
    Frozen,
    /// Requires explicit permission for any change
    Restricted,
    /// Changes need approval before applying
    ApprovalRequired,
    /// Changes must include tests
    TestsRequired,
    /// Changes must update docs
    DocsRequired,
    /// Changes require code review
    ReviewRequired,
    /// Normal - can be modified freely
    #[default]
    Normal,
    /// Experimental - track all changes
    Experimental,
}

/// @acp:summary "AI behavior modifiers"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorModifier {
    /// General approach
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approach: Option<Approach>,

    /// What to optimize for
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,

    /// Must explain all changes
    #[serde(default)]
    pub explain: bool,

    /// Make incremental changes
    #[serde(default)]
    pub step_by_step: bool,

    /// Verify changes before finalizing
    #[serde(default)]
    pub verify: bool,

    /// Ask permission before each change
    #[serde(default)]
    pub ask_first: bool,
}

/// @acp:summary "AI approach strategy"
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Approach {
    Conservative,
    Aggressive,
    Minimal,
    Comprehensive,
}

/// @acp:summary "Optimization priority"
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Correctness,
    Performance,
    Readability,
    Security,
    Compatibility,
}

/// @acp:summary "Quality requirements"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    #[serde(default)]
    pub tests_required: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_coverage: Option<f64>,

    #[serde(default)]
    pub security_review: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_complexity: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<String>,

    #[serde(default)]
    pub browser_support: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance_budget: Option<PerformanceBudget>,
}

/// @acp:summary "Performance budget constraints"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBudget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_time_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u64>,
}

/// @acp:summary "Deprecation information"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecationInfo {
    #[serde(default)]
    pub deprecated: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub removal_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_guide: Option<String>,

    #[serde(default)]
    pub action: DeprecationAction,
}

/// @acp:summary "Deprecation action to take"
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeprecationAction {
    #[default]
    Warn,
    SuggestMigration,
    AutoMigrate,
    Block,
}

/// @acp:summary "Reference to documentation"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether AI should fetch and read this
    #[serde(default)]
    pub fetch: bool,
}

/// @acp:summary "Experimental/hack marker"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HackMarker {
    pub id: String,

    #[serde(rename = "type")]
    pub hack_type: HackType,

    pub file: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,

    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    pub reason: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_code: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert_instructions: Option<String>,
}

impl HackMarker {
    pub fn is_expired(&self) -> bool {
        self.expires.map(|e| e < Utc::now()).unwrap_or(false)
    }
}

/// @acp:summary "Type of hack/workaround"
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HackType {
    Hack,
    Workaround,
    Debug,
    Experiment,
    Temporary,
    TestOnly,
}

/// @acp:summary "Debug session for tracking AI troubleshooting"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub problem: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hypothesis: Option<String>,

    #[serde(default)]
    pub attempts: Vec<DebugAttempt>,

    #[serde(default)]
    pub status: DebugStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl DebugSession {
    pub fn new(id: impl Into<String>, problem: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            started_at: Utc::now(),
            problem: problem.into(),
            hypothesis: None,
            attempts: Vec::new(),
            status: DebugStatus::Active,
            resolution: None,
            resolved_at: None,
        }
    }

    pub fn add_attempt(&mut self, hypothesis: impl Into<String>, change: impl Into<String>) -> usize {
        let attempt_id = self.attempts.len() + 1;
        self.attempts.push(DebugAttempt {
            attempt_id,
            timestamp: Utc::now(),
            hypothesis: hypothesis.into(),
            change: change.into(),
            files_modified: Vec::new(),
            diff: None,
            result: DebugResult::Unknown,
            observations: None,
            keep: false,
            reverted: false,
        });
        attempt_id
    }

    pub fn record_result(&mut self, attempt_id: usize, result: DebugResult, observations: Option<String>) {
        if let Some(attempt) = self.attempts.iter_mut().find(|a| a.attempt_id == attempt_id) {
            attempt.result = result;
            attempt.observations = observations;

            if result == DebugResult::Success {
                attempt.keep = true;
            }
        }
    }

    pub fn revert_attempt(&mut self, attempt_id: usize) -> Option<&DebugAttempt> {
        if let Some(attempt) = self.attempts.iter_mut().find(|a| a.attempt_id == attempt_id) {
            attempt.reverted = true;
            attempt.keep = false;
            return Some(attempt);
        }
        None
    }

    pub fn resolve(&mut self, resolution: impl Into<String>) {
        self.status = DebugStatus::Resolved;
        self.resolution = Some(resolution.into());
        self.resolved_at = Some(Utc::now());
    }

    pub fn get_kept_attempts(&self) -> Vec<&DebugAttempt> {
        self.attempts.iter().filter(|a| a.keep && !a.reverted).collect()
    }

    pub fn get_reverted_attempts(&self) -> Vec<&DebugAttempt> {
        self.attempts.iter().filter(|a| a.reverted).collect()
    }
}

/// @acp:summary "Debug session status"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DebugStatus {
    #[default]
    Active,
    Paused,
    Resolved,
    Abandoned,
}

/// @acp:summary "A single debug attempt"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugAttempt {
    pub attempt_id: usize,
    pub timestamp: DateTime<Utc>,
    pub hypothesis: String,
    pub change: String,

    #[serde(default)]
    pub files_modified: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,

    #[serde(default)]
    pub result: DebugResult,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<String>,

    #[serde(default)]
    pub keep: bool,

    #[serde(default)]
    pub reverted: bool,
}

/// @acp:summary "Debug attempt result"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DebugResult {
    Success,
    Failure,
    Partial,
    #[default]
    Unknown,
}

/// @acp:summary "Constraint index in cache"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConstraintIndex {
    /// Constraints by file path
    #[serde(default)]
    pub by_file: HashMap<String, Constraints>,

    /// Active hack markers
    #[serde(default)]
    pub hacks: Vec<HackMarker>,

    /// Active debug sessions
    #[serde(default)]
    pub debug_sessions: Vec<DebugSession>,

    /// Files by lock level
    #[serde(default)]
    pub by_lock_level: HashMap<String, Vec<String>>,
}

impl ConstraintIndex {
    /// Get effective constraints for a file (with inheritance)
    pub fn get_effective(&self, file: &str, project_defaults: &Constraints) -> Constraints {
        let file_constraints = self.by_file.get(file).cloned().unwrap_or_default();
        project_defaults.merge(&file_constraints)
    }

    /// Get all expired hacks
    pub fn get_expired_hacks(&self) -> Vec<&HackMarker> {
        self.hacks.iter().filter(|h| h.is_expired()).collect()
    }

    /// Get active debug sessions
    pub fn get_active_debug_sessions(&self) -> Vec<&DebugSession> {
        self.debug_sessions.iter().filter(|s| s.status == DebugStatus::Active).collect()
    }

    /// Get all frozen files
    pub fn get_frozen_files(&self) -> Vec<&str> {
        self.by_lock_level.get("frozen")
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get all restricted files
    pub fn get_restricted_files(&self) -> Vec<&str> {
        self.by_lock_level.get("restricted")
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_merge() {
        let base = Constraints {
            style: Some(StyleConstraint {
                guide: "google-typescript".to_string(),
                reference: None,
                rules: vec![],
                linter: None,
            }),
            mutation: None,
            behavior: None,
            quality: None,
            deprecation: None,
            references: vec![],
            directive: Some("Base directive".to_string()),
            auto_generated: false,
        };

        let override_constraints = Constraints {
            style: None,
            mutation: Some(MutationConstraint {
                level: LockLevel::Restricted,
                reason: Some("Security".to_string()),
                contact: None,
                requires_approval: true,
                requires_tests: false,
                requires_docs: false,
                max_lines_changed: None,
                allowed_operations: None,
                forbidden_operations: None,
            }),
            behavior: None,
            quality: None,
            deprecation: None,
            references: vec![],
            directive: Some("Override directive".to_string()),
            auto_generated: false,
        };

        let merged = base.merge(&override_constraints);

        // Style should be preserved from base
        assert!(merged.style.is_some());
        assert_eq!(merged.style.unwrap().guide, "google-typescript");

        // Mutation should come from override
        assert!(merged.mutation.is_some());
        assert_eq!(merged.mutation.unwrap().level, LockLevel::Restricted);
    }

    #[test]
    fn test_debug_session() {
        let mut session = DebugSession::new("test-123", "Something is broken");

        let attempt1 = session.add_attempt("Maybe it's X", "Changed X");
        session.record_result(attempt1, DebugResult::Failure, Some("Nope, not X".to_string()));

        let attempt2 = session.add_attempt("Maybe it's Y", "Changed Y");
        session.record_result(attempt2, DebugResult::Success, Some("Yes, Y was the issue!".to_string()));

        // Revert the failed attempt
        session.revert_attempt(attempt1);

        assert_eq!(session.get_kept_attempts().len(), 1);
        assert_eq!(session.get_reverted_attempts().len(), 1);

        session.resolve("Fixed by changing Y");
        assert_eq!(session.status, DebugStatus::Resolved);
    }
}
