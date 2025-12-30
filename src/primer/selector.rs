//! @acp:module "Primer Selector"
//! @acp:summary "4-phase section selection algorithm with capability filtering"
//! @acp:domain cli
//! @acp:layer logic

use super::condition::{evaluate_condition, ProjectState};
use super::scoring::{calculate_section_value, value_per_token};
use super::types::*;

/// Select sections for primer output using 4-phase algorithm:
/// 1. Required sections (always include)
/// 2. Conditionally required (evaluate requiredIf)
/// 3. Safety-critical (safety >= 80, up to phase budget)
/// 4. Value-optimized (remaining budget, sorted by value-per-token)
pub fn select_sections(
    config: &PrimerConfig,
    budget: u32,
    capabilities: &[String],
    project_state: &ProjectState,
) -> Vec<SelectedSection> {
    let weights = &config.selection_strategy.weights;
    let dynamic_enabled = config.selection_strategy.dynamic_modifiers_enabled;

    let mut selected: Vec<SelectedSection> = Vec::new();
    let mut remaining_budget = budget;
    let mut excluded_by_conflict: Vec<String> = Vec::new();

    // Pre-filter: Remove disabled sections
    let mut available: Vec<&Section> = config
        .sections
        .iter()
        .filter(|s| !config.disabled_sections.contains(&s.id))
        .collect();

    // Pre-filter: Capability filtering
    if !capabilities.is_empty() {
        available.retain(|s| matches_capabilities(s, capabilities));
    }

    // Phase 1: Required sections (always include)
    let required: Vec<_> = available
        .iter()
        .filter(|s| s.required && !is_excluded(&s.id, &excluded_by_conflict))
        .cloned()
        .collect();

    for section in required {
        let tokens = get_tokens(section, project_state);
        if remaining_budget >= tokens {
            selected.push(create_selected(
                section,
                weights,
                project_state,
                dynamic_enabled,
            ));
            remaining_budget = remaining_budget.saturating_sub(tokens);
            excluded_by_conflict.extend(section.conflicts_with.clone());
        }
    }

    // Phase 2: Conditionally required (evaluate requiredIf)
    let conditionally_required: Vec<_> = available
        .iter()
        .filter(|s| {
            s.required_if.is_some()
                && !is_selected_by_id(&selected, &s.id)
                && !is_excluded(&s.id, &excluded_by_conflict)
        })
        .cloned()
        .collect();

    for section in conditionally_required {
        if let Some(ref condition) = section.required_if {
            if evaluate_condition(condition, project_state).unwrap_or(false) {
                let tokens = get_tokens(section, project_state);
                if remaining_budget >= tokens {
                    selected.push(create_selected(
                        section,
                        weights,
                        project_state,
                        dynamic_enabled,
                    ));
                    remaining_budget = remaining_budget.saturating_sub(tokens);
                    excluded_by_conflict.extend(section.conflicts_with.clone());
                }
            }
        }
    }

    // Phase 3: Safety-critical (safety >= 80)
    let safety_budget_pct = get_phase_budget_percent(config, "safety-critical");
    let safety_budget = (budget as f32 * safety_budget_pct / 100.0) as u32;
    let mut safety_spent = 0u32;

    let mut safety_sections: Vec<_> = available
        .iter()
        .filter(|s| {
            s.value.safety >= 80
                && !is_selected_by_id(&selected, &s.id)
                && !is_excluded(&s.id, &excluded_by_conflict)
        })
        .cloned()
        .collect();

    // Sort by value descending
    safety_sections.sort_by(|a, b| {
        let va = calculate_section_value(a, weights, project_state, dynamic_enabled);
        let vb = calculate_section_value(b, weights, project_state, dynamic_enabled);
        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
    });

    for section in safety_sections {
        let tokens = get_tokens(section, project_state);
        if safety_spent + tokens <= safety_budget && remaining_budget >= tokens {
            // Check dependsOn
            if deps_met(section, &selected) {
                selected.push(create_selected(
                    section,
                    weights,
                    project_state,
                    dynamic_enabled,
                ));
                remaining_budget = remaining_budget.saturating_sub(tokens);
                safety_spent += tokens;
                excluded_by_conflict.extend(section.conflicts_with.clone());
            }
        }
    }

    // Phase 4: Value-optimized (remaining budget)
    let mut remaining_sections: Vec<_> = available
        .iter()
        .filter(|s| {
            !is_selected_by_id(&selected, &s.id) && !is_excluded(&s.id, &excluded_by_conflict)
        })
        .cloned()
        .collect();

    // Sort by value-per-token descending
    remaining_sections.sort_by(|a, b| {
        let va = value_per_token(a, weights, project_state, dynamic_enabled);
        let vb = value_per_token(b, weights, project_state, dynamic_enabled);
        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
    });

    for section in remaining_sections {
        let tokens = get_tokens(section, project_state);
        if remaining_budget >= tokens {
            // Check dependsOn
            if deps_met(section, &selected) {
                selected.push(create_selected(
                    section,
                    weights,
                    project_state,
                    dynamic_enabled,
                ));
                remaining_budget = remaining_budget.saturating_sub(tokens);
                excluded_by_conflict.extend(section.conflicts_with.clone());
            }
        }
    }

    // Sort final output by priority
    selected.sort_by_key(|s| s.priority);

    selected
}

/// Check if section matches requested capabilities
fn matches_capabilities(section: &Section, requested: &[String]) -> bool {
    // Section matches if:
    // 1. It has no capability requirements, OR
    // 2. ANY of its capabilities match requested (for `capabilities`), OR
    // 3. ALL of its capabilities_all match requested

    if section.capabilities.is_empty() && section.capabilities_all.is_empty() {
        return true;
    }

    let any_match = section.capabilities.is_empty()
        || section.capabilities.iter().any(|c| requested.contains(c));

    let all_match = section.capabilities_all.is_empty()
        || section
            .capabilities_all
            .iter()
            .all(|c| requested.contains(c));

    any_match && all_match
}

fn is_selected_by_id(selected: &[SelectedSection], id: &str) -> bool {
    selected.iter().any(|s| s.id == id)
}

fn is_excluded(id: &str, excluded: &[String]) -> bool {
    excluded.contains(&id.to_string())
}

fn deps_met(section: &Section, selected: &[SelectedSection]) -> bool {
    section
        .depends_on
        .iter()
        .all(|dep| is_selected_by_id(selected, dep))
}

fn get_tokens(section: &Section, state: &ProjectState) -> u32 {
    match &section.tokens {
        TokenCount::Fixed(n) => *n,
        TokenCount::Dynamic(_) => {
            // Estimate based on data source
            if let Some(ref data) = section.data {
                let item_tokens = data.item_tokens.unwrap_or(10);
                let max_items = data.max_items.unwrap_or(10);
                let estimated_items = estimate_data_count(&data.source, state);
                (estimated_items as u32 * item_tokens).min(max_items as u32 * item_tokens)
            } else {
                50 // Default estimate
            }
        }
    }
}

fn estimate_data_count(source: &str, state: &ProjectState) -> usize {
    match source {
        "cache.constraints.by_lock_level" => state.frozen_count + state.restricted_count,
        "cache.domains" => state.domains_count,
        "cache.layers" => state.layers_count,
        "cache.entryPoints" => state.entry_points_count,
        "cache.hacks" | "hacks" => state.hacks_count,
        "attempts.active" | "cache.attempts" => state.active_attempts,
        "vars.variables" | "cache.variables" => state.variables_count,
        _ => 5, // Default estimate
    }
}

fn get_phase_budget_percent(config: &PrimerConfig, phase_name: &str) -> f32 {
    config
        .selection_strategy
        .phases
        .iter()
        .find(|p| p.name == phase_name)
        .and_then(|p| p.budget_percent)
        .unwrap_or(40.0) // Default 40% for safety-critical
}

fn create_selected(
    section: &Section,
    weights: &DimensionWeights,
    state: &ProjectState,
    dynamic_enabled: bool,
) -> SelectedSection {
    SelectedSection {
        id: section.id.clone(),
        priority: section.priority.unwrap_or(100),
        tokens: get_tokens(section, state),
        value: calculate_section_value(section, weights, state, dynamic_enabled),
        section: section.clone(),
    }
}

/// List all available sections from config
pub fn list_sections(config: &PrimerConfig) -> Vec<SectionInfo> {
    config
        .sections
        .iter()
        .map(|s| SectionInfo {
            id: s.id.clone(),
            name: s.name.clone().unwrap_or_else(|| s.id.clone()),
            category: s.category.clone(),
            tokens: s.tokens.estimate(),
            required: s.required,
            capabilities: s.capabilities.clone(),
        })
        .collect()
}

/// Section info for listing
#[derive(Debug)]
pub struct SectionInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub tokens: u32,
    pub required: bool,
    pub capabilities: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_filtering_shell_only() {
        let section = Section {
            id: "test".to_string(),
            capabilities: vec!["shell".to_string()],
            ..default_section()
        };
        assert!(matches_capabilities(&section, &["shell".to_string()]));
        assert!(!matches_capabilities(&section, &["mcp".to_string()]));
    }

    #[test]
    fn test_capability_filtering_no_requirements() {
        let section = Section {
            id: "test".to_string(),
            capabilities: vec![],
            capabilities_all: vec![],
            ..default_section()
        };
        assert!(matches_capabilities(&section, &["shell".to_string()]));
        assert!(matches_capabilities(&section, &["mcp".to_string()]));
        assert!(matches_capabilities(&section, &[]));
    }

    fn default_section() -> Section {
        Section {
            id: String::new(),
            category: String::new(),
            tokens: TokenCount::Fixed(0),
            value: SectionValue::default(),
            name: None,
            description: None,
            priority: None,
            required: false,
            required_if: None,
            capabilities: vec![],
            capabilities_all: vec![],
            depends_on: vec![],
            conflicts_with: vec![],
            replaces: vec![],
            tags: vec![],
            formats: SectionFormats::default(),
            data: None,
        }
    }
}
