//! @acp:module "Primer Loader"
//! @acp:summary "Load and merge primer configurations from multiple sources"
//! @acp:domain cli
//! @acp:layer io

use anyhow::{Context, Result};
use std::path::Path;

use super::types::*;

/// CLI overrides for primer configuration
#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub preset: Option<String>,
    pub categories: Vec<String>,
    pub no_dynamic: bool,
}

/// Load primer configuration with 3-layer merging:
/// 1. Built-in defaults (primer.defaults.json)
/// 2. Project customization (.acp/primer.json)
/// 3. CLI overrides
pub fn load_primer_config(
    project_primer: Option<&Path>,
    cli: &CliOverrides,
) -> Result<PrimerConfig> {
    // 1. Load built-in defaults
    let mut config = load_builtin_defaults()?;

    // 2. Merge project customizations if present
    if let Some(path) = project_primer {
        if path.exists() {
            let project = load_project_primer(path)?;
            config = merge_configs(config, project)?;
        }
    }

    // 3. Apply CLI overrides
    config = apply_cli_overrides(config, cli)?;

    Ok(config)
}

/// Load built-in primer defaults
fn load_builtin_defaults() -> Result<PrimerConfig> {
    // Include the defaults file at compile time
    let json = include_str!("../../acp-spec/primers/primer.defaults.json");
    serde_json::from_str(json).context("Failed to parse built-in primer.defaults.json")
}

/// Load project-specific primer configuration
fn load_project_primer(path: &Path) -> Result<PrimerConfig> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read primer config from {:?}", path))?;
    serde_json::from_str(&json)
        .with_context(|| format!("Failed to parse primer config from {:?}", path))
}

/// Merge project config into base config
fn merge_configs(base: PrimerConfig, project: PrimerConfig) -> Result<PrimerConfig> {
    let mut result = base;

    // Merge sections by ID (project wins for same ID)
    for section in project.sections {
        if let Some(existing) = result.sections.iter_mut().find(|s| s.id == section.id) {
            *existing = section;
        } else {
            result.sections.push(section);
        }
    }

    // Append additional sections
    result.sections.extend(project.additional_sections);

    // Apply section overrides
    for (id, override_) in project.section_overrides {
        if let Some(section) = result.sections.iter_mut().find(|s| s.id == id) {
            apply_section_override(section, &override_);
        } else {
            tracing::warn!("Section override for unknown ID: {}", id);
        }
    }

    // Accumulate disabled sections
    result.disabled_sections.extend(project.disabled_sections);

    // Replace selection strategy if project defines phases
    if !project.selection_strategy.phases.is_empty() {
        result.selection_strategy = project.selection_strategy;
    } else {
        // Merge presets
        result
            .selection_strategy
            .presets
            .extend(project.selection_strategy.presets);

        // Update weights if changed from default
        if project.selection_strategy.weights.safety != 1.5
            || project.selection_strategy.weights.efficiency != 1.0
            || project.selection_strategy.weights.accuracy != 1.0
            || project.selection_strategy.weights.base != 1.0
        {
            result.selection_strategy.weights = project.selection_strategy.weights;
        }
    }

    Ok(result)
}

/// Apply section override to an existing section
fn apply_section_override(section: &mut Section, override_: &SectionOverride) {
    if let Some(ref value) = override_.value {
        section.value = value.clone();
    }
    if let Some(tokens) = override_.tokens {
        section.tokens = TokenCount::Fixed(tokens);
    }
    if let Some(required) = override_.required {
        section.required = required;
    }
    if let Some(ref required_if) = override_.required_if {
        section.required_if = Some(required_if.clone());
    }
    if let Some(ref formats) = override_.formats {
        section.formats = formats.clone();
    }
}

/// Apply CLI overrides to config
fn apply_cli_overrides(mut config: PrimerConfig, cli: &CliOverrides) -> Result<PrimerConfig> {
    // Add excluded sections from CLI
    config.disabled_sections.extend(cli.exclude.clone());

    // Force include sections
    for id in &cli.include {
        if let Some(section) = config.sections.iter_mut().find(|s| &s.id == id) {
            section.required = true;
        } else {
            tracing::warn!("Unknown section ID in --include: {}", id);
        }
    }

    // Filter by categories if specified
    if !cli.categories.is_empty() {
        config
            .sections
            .retain(|s| cli.categories.contains(&s.category));
    }

    // Apply preset weights if specified
    if let Some(ref preset_name) = cli.preset {
        if let Some(weights) = config.selection_strategy.presets.get(preset_name) {
            config.selection_strategy.weights = weights.clone();
        } else {
            // Use built-in presets
            config.selection_strategy.weights = super::scoring::get_preset_weights(preset_name);
        }
    }

    // Disable dynamic modifiers if requested
    if cli.no_dynamic {
        config.selection_strategy.dynamic_modifiers_enabled = false;
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_builtin_defaults() {
        let config = load_builtin_defaults().expect("Failed to load defaults");
        assert!(!config.sections.is_empty());
        assert!(config.sections.iter().any(|s| s.id == "acp-exists"));
    }

    #[test]
    fn test_cli_exclude_sections() {
        let config = load_builtin_defaults().unwrap();
        let cli = CliOverrides {
            exclude: vec!["cli-overview".to_string()],
            ..Default::default()
        };
        let result = apply_cli_overrides(config, &cli).unwrap();
        assert!(result
            .disabled_sections
            .contains(&"cli-overview".to_string()));
    }
}
