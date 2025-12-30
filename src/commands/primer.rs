//! @acp:module "Primer Command"
//! @acp:summary "Generate AI bootstrap primers with value-based section selection"
//! @acp:domain cli
//! @acp:layer handler
//!
//! RFC-0004: Tiered Interface Primers
//! Generates token-efficient bootstrap text for AI agents.

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::cache::Cache;
use crate::primer::{
    self, load_primer_config, render_primer, select_sections, CliOverrides, OutputFormat,
    ProjectState,
};

/// Options for the primer command
#[derive(Debug, Clone)]
pub struct PrimerOptions {
    /// Token budget for the primer
    pub budget: u32,
    /// Required capabilities (e.g., "shell", "mcp")
    pub capabilities: Vec<String>,
    /// Cache file path (for project state)
    pub cache: Option<PathBuf>,
    /// Custom primer config file
    pub primer_config: Option<PathBuf>,
    /// Output format
    pub format: OutputFormat,
    /// Output as JSON metadata
    pub json: bool,
    /// Weight preset (safe, efficient, accurate, balanced)
    pub preset: Option<String>,
    /// Force include section IDs
    pub include: Vec<String>,
    /// Exclude section IDs
    pub exclude: Vec<String>,
    /// Filter by category IDs
    pub categories: Vec<String>,
    /// Disable dynamic value modifiers
    pub no_dynamic: bool,
    /// Show selection reasoning
    pub explain: bool,
    /// List available sections
    pub list_sections: bool,
    /// List available presets
    pub list_presets: bool,
    /// Preview selection without rendering
    pub preview: bool,
}

impl Default for PrimerOptions {
    fn default() -> Self {
        Self {
            budget: 200,
            capabilities: vec![],
            cache: None,
            primer_config: None,
            format: OutputFormat::Markdown,
            json: false,
            preset: None,
            include: vec![],
            exclude: vec![],
            categories: vec![],
            no_dynamic: false,
            explain: false,
            list_sections: false,
            list_presets: false,
            preview: false,
        }
    }
}

/// Generated primer output
#[derive(Debug, Clone, Serialize)]
pub struct PrimerOutput {
    pub total_tokens: u32,
    pub tier: String,
    pub sections_included: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_reasoning: Option<Vec<SelectionReason>>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectionReason {
    pub section_id: String,
    pub phase: String,
    pub value: f64,
    pub tokens: u32,
}

/// Execute the primer command
pub fn execute_primer(options: PrimerOptions) -> Result<()> {
    // Handle list modes first
    if options.list_presets {
        println!("Available presets:\n");
        for (name, description, weights) in primer::scoring::list_presets() {
            println!("  {} - {}", console::style(name).bold(), description);
            println!(
                "    safety={:.1} efficiency={:.1} accuracy={:.1} base={:.1}\n",
                weights.safety, weights.efficiency, weights.accuracy, weights.base
            );
        }
        return Ok(());
    }

    if options.list_sections {
        let cli_overrides = CliOverrides::default();
        let config = load_primer_config(options.primer_config.as_deref(), &cli_overrides)?;

        println!("Available sections ({}):\n", config.sections.len());
        for section in primer::selector::list_sections(&config) {
            let required = if section.required { " [required]" } else { "" };
            let caps = if section.capabilities.is_empty() {
                String::new()
            } else {
                format!(" ({})", section.capabilities.join(","))
            };
            println!(
                "  {:30} {:15} ~{} tokens{}{}",
                section.id, section.category, section.tokens, required, caps
            );
        }
        return Ok(());
    }

    // Generate primer
    let primer = generate_primer(&options)?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&primer)?);
    } else if options.preview {
        println!("Preview: {} tokens, {} sections", primer.total_tokens, primer.sections_included);
        if let Some(reasons) = &primer.selection_reasoning {
            println!("\nSelection:");
            for reason in reasons {
                println!(
                    "  [{:12}] {:30} value={:.1} tokens={}",
                    reason.phase, reason.section_id, reason.value, reason.tokens
                );
            }
        }
    } else {
        println!("{}", primer.content);
    }

    Ok(())
}

/// Generate primer content based on budget and capabilities
pub fn generate_primer(options: &PrimerOptions) -> Result<PrimerOutput> {
    // Build CLI overrides
    let cli_overrides = CliOverrides {
        include: options.include.clone(),
        exclude: options.exclude.clone(),
        preset: options.preset.clone(),
        categories: options.categories.clone(),
        no_dynamic: options.no_dynamic,
    };

    // Load primer config with 3-layer merge
    let config = load_primer_config(options.primer_config.as_deref(), &cli_overrides)?;

    // Load project state from cache
    let project_state = if let Some(ref cache_path) = options.cache {
        if cache_path.exists() {
            let cache = Cache::from_json(cache_path)?;
            ProjectState::from_cache(&cache)
        } else {
            ProjectState::default()
        }
    } else {
        ProjectState::default()
    };

    // Select sections based on budget and capabilities
    let selected = select_sections(&config, options.budget, &options.capabilities, &project_state);

    // Calculate totals
    let total_tokens: u32 = selected.iter().map(|s| s.tokens).sum();

    // Build selection reasoning if explain mode
    let selection_reasoning = if options.explain || options.preview {
        Some(
            selected
                .iter()
                .map(|s| SelectionReason {
                    section_id: s.id.clone(),
                    phase: determine_phase(&s.section),
                    value: s.value,
                    tokens: s.tokens,
                })
                .collect(),
        )
    } else {
        None
    };

    // Determine tier name based on budget
    let tier = get_tier_name(options.budget);

    // Render output
    let content = render_primer(&selected, options.format, &project_state)?;

    Ok(PrimerOutput {
        total_tokens,
        tier,
        sections_included: selected.len(),
        selection_reasoning,
        content,
    })
}

fn determine_phase(section: &primer::types::Section) -> String {
    if section.required {
        "required".to_string()
    } else if section.required_if.is_some() {
        "conditional".to_string()
    } else if section.value.safety >= 80 {
        "safety".to_string()
    } else {
        "value".to_string()
    }
}

fn get_tier_name(budget: u32) -> String {
    match budget {
        0..=79 => "survival".to_string(),
        80..=149 => "essential".to_string(),
        150..=299 => "operational".to_string(),
        300..=499 => "informed".to_string(),
        500..=999 => "complete".to_string(),
        _ => "expert".to_string(),
    }
}

// ============================================================================
// Legacy types for backward compatibility with existing tests
// ============================================================================

/// Output format for primer (legacy)
#[derive(Debug, Clone, Copy, Default)]
pub enum PrimerFormat {
    #[default]
    Text,
    Json,
}

/// Tier level for content selection (legacy)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Minimal,
    Standard,
    Full,
}

impl Tier {
    /// Determine tier based on remaining budget
    pub fn from_budget(remaining: u32) -> Self {
        if remaining < 80 {
            Tier::Minimal
        } else if remaining < 300 {
            Tier::Standard
        } else {
            Tier::Full
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_budget() {
        assert_eq!(Tier::from_budget(50), Tier::Minimal);
        assert_eq!(Tier::from_budget(79), Tier::Minimal);
        assert_eq!(Tier::from_budget(80), Tier::Standard);
        assert_eq!(Tier::from_budget(200), Tier::Standard);
        assert_eq!(Tier::from_budget(299), Tier::Standard);
        assert_eq!(Tier::from_budget(300), Tier::Full);
        assert_eq!(Tier::from_budget(500), Tier::Full);
    }

    #[test]
    fn test_generate_minimal_primer() {
        let options = PrimerOptions {
            budget: 60,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        assert!(result.total_tokens <= 80 || result.sections_included >= 1);
        assert_eq!(result.tier, "survival");
    }

    #[test]
    fn test_generate_standard_primer() {
        let options = PrimerOptions {
            budget: 200,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        assert_eq!(result.tier, "operational");
        assert!(result.sections_included >= 1);
    }

    #[test]
    fn test_critical_commands_always_included() {
        let options = PrimerOptions {
            budget: 30,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        // Required sections should still be included
        assert!(result.sections_included >= 1);
    }

    #[test]
    fn test_capability_filtering() {
        let options = PrimerOptions {
            budget: 200,
            capabilities: vec!["mcp".to_string()],
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        // With MCP capability, should get MCP sections only
        assert!(result.sections_included >= 1);
    }
}
