//! @acp:module "Primer Dynamic Data"
//! @acp:summary "Populate dynamic sections from cache and project state"
//! @acp:domain cli
//! @acp:layer data

use super::condition::ProjectState;
use super::renderer::DynamicItem;
use super::types::{DataFilter, SectionData};

/// Get dynamic data for a section based on its data configuration
pub fn get_dynamic_data(data_config: &SectionData, state: &ProjectState) -> Vec<DynamicItem> {
    let items = match data_config.source.as_str() {
        // Constraint sources
        "cache.constraints.by_lock_level" => get_protected_files(state, &data_config.filter),

        // Structure sources
        "cache.domains" => get_domains(state),
        "cache.layers" => get_layers(state),
        "cache.entryPoints" => get_entry_points(state),

        // Debug sources
        "cache.hacks" | "hacks" => get_hacks(state, &data_config.filter),
        "attempts.active" | "cache.attempts" => get_active_attempts(state),

        // Variable sources
        "vars.variables" | "cache.variables" => get_variables(state),

        source => {
            tracing::warn!("Unknown data source: {}", source);
            Vec::new()
        }
    };

    // Apply max_items limit
    if let Some(max) = data_config.max_items {
        items.into_iter().take(max).collect()
    } else {
        items
    }
}

fn get_protected_files(state: &ProjectState, filter: &Option<DataFilter>) -> Vec<DynamicItem> {
    let mut items: Vec<DynamicItem> = Vec::new();

    // Add frozen files
    for file in &state.frozen_files {
        items.push(DynamicItem::ProtectedFile(file.clone()));
    }

    // Add restricted files
    for file in &state.restricted_files {
        items.push(DynamicItem::ProtectedFile(file.clone()));
    }

    // Apply filter if present
    if let Some(DataFilter::Array(levels)) = filter {
        items.retain(|item| {
            if let DynamicItem::ProtectedFile(pf) = item {
                levels.contains(&pf.level)
            } else {
                false
            }
        });
    }

    items
}

fn get_domains(state: &ProjectState) -> Vec<DynamicItem> {
    state
        .domains
        .iter()
        .map(|d| DynamicItem::Domain(d.clone()))
        .collect()
}

fn get_layers(state: &ProjectState) -> Vec<DynamicItem> {
    state
        .layers
        .iter()
        .map(|l| DynamicItem::Layer(l.clone()))
        .collect()
}

fn get_entry_points(state: &ProjectState) -> Vec<DynamicItem> {
    state
        .entry_points
        .iter()
        .map(|e| DynamicItem::EntryPoint(e.clone()))
        .collect()
}

fn get_variables(state: &ProjectState) -> Vec<DynamicItem> {
    state
        .variables
        .iter()
        .map(|v| DynamicItem::Variable(v.clone()))
        .collect()
}

fn get_active_attempts(state: &ProjectState) -> Vec<DynamicItem> {
    state
        .active_attempt_list
        .iter()
        .map(|a| DynamicItem::Attempt(a.clone()))
        .collect()
}

fn get_hacks(state: &ProjectState, filter: &Option<DataFilter>) -> Vec<DynamicItem> {
    let mut items: Vec<DynamicItem> = state
        .hacks
        .iter()
        .map(|h| DynamicItem::Hack(h.clone()))
        .collect();

    // Apply filter for expired = false if specified
    if let Some(DataFilter::Object(obj)) = filter {
        if let Some(expired_val) = obj.get("expired") {
            if expired_val == &serde_json::Value::Bool(false) {
                items.retain(|item| {
                    if let DynamicItem::Hack(h) = item {
                        !h.expired
                    } else {
                        true
                    }
                });
            }
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primer::condition::ProtectedFile;

    #[test]
    fn test_protected_files_filter() {
        let state = ProjectState {
            frozen_files: vec![ProtectedFile {
                path: "a.rs".to_string(),
                level: "frozen".to_string(),
                reason: None,
            }],
            restricted_files: vec![ProtectedFile {
                path: "b.rs".to_string(),
                level: "restricted".to_string(),
                reason: None,
            }],
            ..Default::default()
        };

        // No filter - get all
        let all = get_protected_files(&state, &None);
        assert_eq!(all.len(), 2);

        // Filter for frozen only
        let filter = Some(DataFilter::Array(vec!["frozen".to_string()]));
        let frozen_only = get_protected_files(&state, &filter);
        assert_eq!(frozen_only.len(), 1);
    }

    #[test]
    fn test_max_items_limit() {
        let state = ProjectState {
            domains: vec![
                crate::primer::condition::DomainInfo {
                    name: "a".to_string(),
                    pattern: "a/*".to_string(),
                    description: None,
                },
                crate::primer::condition::DomainInfo {
                    name: "b".to_string(),
                    pattern: "b/*".to_string(),
                    description: None,
                },
                crate::primer::condition::DomainInfo {
                    name: "c".to_string(),
                    pattern: "c/*".to_string(),
                    description: None,
                },
            ],
            domains_count: 3,
            ..Default::default()
        };

        let data = SectionData {
            source: "cache.domains".to_string(),
            max_items: Some(2),
            fields: vec![],
            filter: None,
            sort_by: None,
            sort_order: None,
            item_tokens: None,
            empty_behavior: None,
        };

        let items = get_dynamic_data(&data, &state);
        assert_eq!(items.len(), 2);
    }
}
