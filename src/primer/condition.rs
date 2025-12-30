//! @acp:module "Primer Condition Parser"
//! @acp:summary "Parse and evaluate condition expressions for requiredIf and value modifiers"
//! @acp:domain cli
//! @acp:layer logic

use anyhow::{anyhow, Result};

/// Project state extracted from cache for condition evaluation
#[derive(Debug, Default, Clone)]
pub struct ProjectState {
    // Constraint counts
    pub frozen_count: usize,
    pub restricted_count: usize,
    pub approval_count: usize,
    pub tests_required_count: usize,
    pub docs_required_count: usize,
    pub protected_count: usize,

    // Debug/attempt counts
    pub active_attempts: usize,
    pub hacks_count: usize,
    pub expired_hacks: usize,

    // Structure counts
    pub domains_count: usize,
    pub layers_count: usize,
    pub entry_points_count: usize,
    pub variables_count: usize,

    // Dynamic data for sections
    pub frozen_files: Vec<ProtectedFile>,
    pub restricted_files: Vec<ProtectedFile>,
    pub domains: Vec<DomainInfo>,
    pub layers: Vec<LayerInfo>,
    pub entry_points: Vec<EntryPointInfo>,
    pub variables: Vec<VariableInfo>,
    pub active_attempt_list: Vec<AttemptInfo>,
    pub hacks: Vec<HackInfo>,
}

#[derive(Debug, Clone)]
pub struct ProtectedFile {
    pub path: String,
    pub level: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DomainInfo {
    pub name: String,
    pub pattern: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LayerInfo {
    pub name: String,
    pub pattern: String,
}

#[derive(Debug, Clone)]
pub struct EntryPointInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AttemptInfo {
    pub id: String,
    pub problem: String,
    pub attempt_count: usize,
}

#[derive(Debug, Clone)]
pub struct HackInfo {
    pub file: String,
    pub reason: String,
    pub expires: Option<String>,
    pub expired: bool,
}

#[derive(Debug)]
enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug)]
struct Condition {
    path: String,
    operator: Operator,
    value: i64,
}

/// Evaluate a condition expression against project state
///
/// Supports expressions like:
/// - "constraints.frozenCount > 0"
/// - "hacks.expiredCount > 0"
/// - "attempts.activeCount == 0"
pub fn evaluate_condition(expr: &str, state: &ProjectState) -> Result<bool> {
    let condition = parse_condition(expr)?;
    let actual_value = resolve_path(&condition.path, state)?;

    Ok(match condition.operator {
        Operator::Eq => actual_value == condition.value,
        Operator::Ne => actual_value != condition.value,
        Operator::Gt => actual_value > condition.value,
        Operator::Gte => actual_value >= condition.value,
        Operator::Lt => actual_value < condition.value,
        Operator::Lte => actual_value <= condition.value,
    })
}

fn parse_condition(expr: &str) -> Result<Condition> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(anyhow!(
            "Invalid condition '{}': expected 'path operator value'",
            expr
        ));
    }

    let path = parts[0].to_string();
    let operator = match parts[1] {
        "==" => Operator::Eq,
        "!=" => Operator::Ne,
        ">" => Operator::Gt,
        ">=" => Operator::Gte,
        "<" => Operator::Lt,
        "<=" => Operator::Lte,
        op => return Err(anyhow!("Unknown operator '{}' in condition", op)),
    };
    let value = parts[2]
        .parse::<i64>()
        .map_err(|_| anyhow!("Invalid numeric value '{}' in condition", parts[2]))?;

    Ok(Condition {
        path,
        operator,
        value,
    })
}

fn resolve_path(path: &str, state: &ProjectState) -> Result<i64> {
    match path {
        // Constraint paths
        "constraints.frozenCount" => Ok(state.frozen_count as i64),
        "constraints.restrictedCount" => Ok(state.restricted_count as i64),
        "constraints.approvalCount" => Ok(state.approval_count as i64),
        "constraints.testsRequiredCount" => Ok(state.tests_required_count as i64),
        "constraints.docsRequiredCount" => Ok(state.docs_required_count as i64),
        "constraints.protectedCount" => Ok(state.protected_count as i64),

        // Attempt/debug paths
        "attempts.activeCount" => Ok(state.active_attempts as i64),
        "hacks.count" => Ok(state.hacks_count as i64),
        "hacks.expiredCount" => Ok(state.expired_hacks as i64),

        // Structure paths
        "domains.count" => Ok(state.domains_count as i64),
        "layers.count" => Ok(state.layers_count as i64),
        "entryPoints.count" => Ok(state.entry_points_count as i64),
        "variables.count" => Ok(state.variables_count as i64),

        _ => Err(anyhow!("Unknown condition path: {}", path)),
    }
}

impl ProjectState {
    /// Create ProjectState from cache data
    pub fn from_cache(cache: &crate::cache::Cache) -> Self {
        let mut state = ProjectState::default();

        // Extract constraint counts
        if let Some(constraints) = &cache.constraints {
            let by_level = &constraints.by_lock_level;
            state.frozen_count = by_level
                .get("frozen")
                .map(|v: &Vec<String>| v.len())
                .unwrap_or(0);
            state.restricted_count = by_level
                .get("restricted")
                .map(|v: &Vec<String>| v.len())
                .unwrap_or(0);
            state.approval_count = by_level
                .get("approval-required")
                .map(|v: &Vec<String>| v.len())
                .unwrap_or(0);
            state.tests_required_count = by_level
                .get("tests-required")
                .map(|v: &Vec<String>| v.len())
                .unwrap_or(0);
            state.docs_required_count = by_level
                .get("docs-required")
                .map(|v: &Vec<String>| v.len())
                .unwrap_or(0);

            state.protected_count = state.frozen_count + state.restricted_count;

            // Build protected files list
            if let Some(frozen) = by_level.get("frozen") {
                for path in frozen {
                    state.frozen_files.push(ProtectedFile {
                        path: path.clone(),
                        level: "frozen".to_string(),
                        reason: None, // Would need to look up in by_file
                    });
                }
            }
            if let Some(restricted) = by_level.get("restricted") {
                for path in restricted {
                    state.restricted_files.push(ProtectedFile {
                        path: path.clone(),
                        level: "restricted".to_string(),
                        reason: None,
                    });
                }
            }

            // Extract hacks info
            state.hacks_count = constraints.hacks.len();
            state.expired_hacks = constraints.hacks.iter().filter(|h| h.is_expired()).count();
            for hack in &constraints.hacks {
                state.hacks.push(HackInfo {
                    file: hack.file.clone(),
                    reason: hack.reason.clone(),
                    expires: hack.expires.map(|e| e.to_rfc3339()),
                    expired: hack.is_expired(),
                });
            }

            // Extract active debug sessions
            state.active_attempts = constraints
                .debug_sessions
                .iter()
                .filter(|s| s.status == crate::constraints::DebugStatus::Active)
                .count();
            for session in &constraints.debug_sessions {
                if session.status == crate::constraints::DebugStatus::Active {
                    state.active_attempt_list.push(AttemptInfo {
                        id: session.id.clone(),
                        problem: session.problem.clone(),
                        attempt_count: session.attempts.len(),
                    });
                }
            }
        }

        // Extract domain info (domains is a HashMap, not Option)
        state.domains_count = cache.domains.len();
        for (name, domain) in &cache.domains {
            state.domains.push(DomainInfo {
                name: name.clone(),
                pattern: domain.files.first().cloned().unwrap_or_default(), // Use first file as pattern
                description: domain.description.clone(),
            });
        }

        // Note: Variables are not stored in cache, they come from a separate vars file
        // This would require loading the vars file separately if needed

        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_greater_than() {
        let state = ProjectState {
            frozen_count: 5,
            ..Default::default()
        };
        assert!(evaluate_condition("constraints.frozenCount > 0", &state).unwrap());
        assert!(!evaluate_condition("constraints.frozenCount > 10", &state).unwrap());
    }

    #[test]
    fn test_condition_equals_zero() {
        let state = ProjectState::default();
        assert!(evaluate_condition("hacks.count == 0", &state).unwrap());
    }

    #[test]
    fn test_condition_less_than_or_equal() {
        let state = ProjectState {
            active_attempts: 3,
            ..Default::default()
        };
        assert!(evaluate_condition("attempts.activeCount <= 5", &state).unwrap());
        assert!(!evaluate_condition("attempts.activeCount <= 2", &state).unwrap());
    }

    #[test]
    fn test_condition_unknown_path_errors() {
        let state = ProjectState::default();
        assert!(evaluate_condition("unknown.path > 0", &state).is_err());
    }

    #[test]
    fn test_condition_invalid_syntax_errors() {
        let state = ProjectState::default();
        assert!(evaluate_condition("invalid", &state).is_err());
        assert!(evaluate_condition("path >", &state).is_err());
    }
}
