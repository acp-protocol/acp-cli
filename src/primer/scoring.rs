//! @acp:module "Primer Scoring"
//! @acp:summary "Multi-dimensional value calculation for section selection"
//! @acp:domain cli
//! @acp:layer logic

use super::condition::{evaluate_condition, ProjectState};
use super::types::*;

/// Calculate weighted section value
///
/// Note: Scores are unbounded weighted sums, not normalized to 0-100.
/// A section with all dimensions at 100 and all weights at 1.0 yields score 400.
/// This is intentional - it allows weight adjustments to have meaningful impact.
pub fn calculate_section_value(
    section: &Section,
    weights: &DimensionWeights,
    project_state: &ProjectState,
    dynamic_enabled: bool,
) -> f64 {
    let mut value = section.value.clone();

    // Apply dynamic modifiers if enabled
    if dynamic_enabled {
        for modifier in &section.value.modifiers {
            if evaluate_condition(&modifier.condition, project_state).unwrap_or(false) {
                apply_modifier(&mut value, modifier);
            }
        }
    }

    // Calculate weighted sum (unbounded)
    (value.safety as f64 * weights.safety)
        + (value.efficiency as f64 * weights.efficiency)
        + (value.accuracy as f64 * weights.accuracy)
        + (value.base as f64 * weights.base)
}

/// Calculate value per token for ranking
pub fn value_per_token(
    section: &Section,
    weights: &DimensionWeights,
    state: &ProjectState,
    dynamic_enabled: bool,
) -> f64 {
    let value = calculate_section_value(section, weights, state, dynamic_enabled);
    let tokens = section.tokens.estimate() as f64;
    if tokens > 0.0 {
        value / tokens
    } else {
        0.0
    }
}

fn apply_modifier(value: &mut SectionValue, modifier: &ValueModifier) {
    let dimension = modifier.dimension.as_deref().unwrap_or("all");

    match dimension {
        "safety" => apply_to_dimension(&mut value.safety, modifier),
        "efficiency" => apply_to_dimension(&mut value.efficiency, modifier),
        "accuracy" => apply_to_dimension(&mut value.accuracy, modifier),
        "base" => apply_to_dimension(&mut value.base, modifier),
        "all" | _ => {
            apply_to_dimension(&mut value.safety, modifier);
            apply_to_dimension(&mut value.efficiency, modifier);
            apply_to_dimension(&mut value.accuracy, modifier);
            apply_to_dimension(&mut value.base, modifier);
        }
    }
}

fn apply_to_dimension(dim: &mut u8, modifier: &ValueModifier) {
    if let Some(add) = modifier.add {
        if add >= 0 {
            *dim = dim.saturating_add(add as u8);
        } else {
            *dim = dim.saturating_sub((-add) as u8);
        }
    }
    if let Some(multiply) = modifier.multiply {
        *dim = ((*dim as f64) * multiply).min(255.0).max(0.0) as u8;
    }
    if let Some(set) = modifier.set {
        *dim = set.min(255).max(0) as u8;
    }
}

/// Get weights for a named preset
pub fn get_preset_weights(preset: &str) -> DimensionWeights {
    match preset {
        "safe" => DimensionWeights {
            safety: 2.5,
            efficiency: 0.8,
            accuracy: 1.0,
            base: 0.8,
        },
        "efficient" => DimensionWeights {
            safety: 1.2,
            efficiency: 2.0,
            accuracy: 0.9,
            base: 0.8,
        },
        "accurate" => DimensionWeights {
            safety: 1.2,
            efficiency: 0.9,
            accuracy: 2.0,
            base: 0.8,
        },
        "balanced" | _ => DimensionWeights::default(),
    }
}

/// Get list of available presets with their descriptions
pub fn list_presets() -> Vec<(&'static str, &'static str, DimensionWeights)> {
    vec![
        ("safe", "Prioritizes safety-critical sections", get_preset_weights("safe")),
        ("efficient", "Prioritizes efficiency-boosting sections", get_preset_weights("efficient")),
        ("accurate", "Prioritizes accuracy-improving sections", get_preset_weights("accurate")),
        ("balanced", "Default balanced weights", get_preset_weights("balanced")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_calculation_no_modifiers() {
        let section = Section {
            id: "test".to_string(),
            category: "test".to_string(),
            tokens: TokenCount::Fixed(10),
            value: SectionValue {
                safety: 100,
                efficiency: 80,
                accuracy: 60,
                base: 50,
                modifiers: vec![],
            },
            formats: SectionFormats::default(),
            ..default_section()
        };
        let weights = DimensionWeights::default(); // 1.5, 1.0, 1.0, 1.0
        let state = ProjectState::default();

        let value = calculate_section_value(&section, &weights, &state, true);
        // 100*1.5 + 80*1.0 + 60*1.0 + 50*1.0 = 150 + 80 + 60 + 50 = 340
        assert_eq!(value, 340.0);
    }

    #[test]
    fn test_preset_weights() {
        let safe = get_preset_weights("safe");
        assert!(safe.safety > safe.efficiency);

        let efficient = get_preset_weights("efficient");
        assert!(efficient.efficiency > efficient.safety);
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
