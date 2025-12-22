//! @acp:module "Guardrail Enforcer"
//! @acp:summary "Enforces guardrails on proposed changes"
//! @acp:domain cli
//! @acp:layer service

use serde::{Deserialize, Serialize};

use super::guardrails::FileGuardrails;

/// @acp:summary "Result of checking guardrails against proposed changes"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailCheck {
    pub passed: bool,
    pub violations: Vec<Violation>,
    pub warnings: Vec<Warning>,
    pub required_actions: Vec<RequiredAction>,
}

/// @acp:summary "A guardrail violation"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
}

/// @acp:summary "A guardrail warning"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub rule: String,
    pub message: String,
}

/// @acp:summary "A required action before changes can proceed"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredAction {
    pub action: String,
    pub reason: String,
}

/// @acp:summary "Severity level of a violation"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// @acp:summary "Enforces guardrails on proposed changes"
pub struct GuardrailEnforcer;

impl GuardrailEnforcer {
    /// Check if AI should modify this file
    pub fn can_modify(guardrails: &FileGuardrails) -> GuardrailCheck {
        let mut check = GuardrailCheck {
            passed: true,
            violations: vec![],
            warnings: vec![],
            required_actions: vec![],
        };

        // Check readonly
        if guardrails.ai_behavior.readonly {
            check.passed = false;
            check.violations.push(Violation {
                rule: "ai-readonly".to_string(),
                message: format!(
                    "File is marked as AI-readonly{}",
                    guardrails.ai_behavior.readonly_reason
                        .as_ref()
                        .map(|r| format!(": {}", r))
                        .unwrap_or_default()
                ),
                severity: Severity::Error,
            });
        }

        // Check ai-careful
        for careful in &guardrails.ai_behavior.careful {
            check.warnings.push(Warning {
                rule: "ai-careful".to_string(),
                message: format!("Extra caution required: {}", careful),
            });
        }

        // Check ai-ask
        for ask in &guardrails.ai_behavior.ask_before {
            check.required_actions.push(RequiredAction {
                action: "ask-user".to_string(),
                reason: format!("Must ask before: {}", ask),
            });
        }

        // Check review requirements
        for review in &guardrails.review.required {
            check.required_actions.push(RequiredAction {
                action: "flag-for-review".to_string(),
                reason: format!("Requires {} review", review),
            });
        }

        check
    }

    /// Check if proposed changes violate constraints
    pub fn check_changes(
        guardrails: &FileGuardrails,
        proposed_content: &str,
    ) -> GuardrailCheck {
        let mut check = Self::can_modify(guardrails);

        // Check forbids
        for forbidden in &guardrails.constraints.forbids {
            if proposed_content.contains(forbidden) {
                check.passed = false;
                check.violations.push(Violation {
                    rule: "forbids".to_string(),
                    message: format!("Contains forbidden pattern: {}", forbidden),
                    severity: Severity::Error,
                });
            }
        }

        // Check test requirements
        if !guardrails.constraints.test_required.is_empty() {
            check.required_actions.push(RequiredAction {
                action: "write-tests".to_string(),
                reason: format!(
                    "Tests required: {}",
                    guardrails.constraints.test_required.join(", ")
                ),
            });
        }

        check
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraints::guardrails::AIBehavior;

    #[test]
    fn test_enforcer_readonly() {
        let guardrails = FileGuardrails {
            ai_behavior: AIBehavior {
                readonly: true,
                readonly_reason: Some("test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let check = GuardrailEnforcer::can_modify(&guardrails);
        assert!(!check.passed);
        assert_eq!(check.violations.len(), 1);
    }
}
