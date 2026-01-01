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
    self, load_primer_config, render_primer_with_tier, select_sections, CliOverrides,
    IdeEnvironment, OutputFormat, PrimerTier, ProjectState,
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
    /// RFC-0015: Standalone mode (include foundation prompt for raw API usage)
    pub standalone: bool,
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
            standalone: false,
        }
    }
}

/// Generated primer output
#[derive(Debug, Clone, Serialize)]
pub struct PrimerOutput {
    pub total_tokens: u32,
    /// RFC-0015 tier (micro, minimal, standard, full)
    pub tier: PrimerTier,
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

    // RFC-0015: Warn when using --standalone in an IDE context
    if options.standalone {
        let ide = IdeEnvironment::detect_with_override();
        if ide.is_ide() && !matches!(ide, IdeEnvironment::ClaudeCode) {
            eprintln!(
                "{}: Using --standalone in {} context. \
                 IDE integrations typically provide their own system prompts. \
                 Consider removing --standalone or set ACP_NO_IDE_DETECT=1 to suppress.",
                console::style("warning").yellow().bold(),
                ide.name()
            );
        }
    }

    // Generate primer
    let primer = generate_primer(&options)?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&primer)?);
    } else if options.preview {
        println!(
            "Preview: {} tokens, {} sections",
            primer.total_tokens, primer.sections_included
        );
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
    let selected = select_sections(
        &config,
        options.budget,
        &options.capabilities,
        &project_state,
    );

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

    // Determine tier based on budget (RFC-0015)
    let tier = PrimerTier::from_budget(options.budget);

    // Render output with tier information
    let content = render_primer_with_tier(&selected, options.format, &project_state, Some(tier))?;

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

/// Tier level for content selection (legacy - use PrimerTier from primer::types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Minimal,
    Standard,
    Full,
}

impl Tier {
    /// Determine tier based on remaining budget (legacy mapping)
    /// Note: For RFC-0015 tier selection, use PrimerTier::from_budget instead
    pub fn from_budget(remaining: u32) -> Self {
        // Legacy mapping for backward compatibility
        if remaining < 300 {
            Tier::Minimal
        } else if remaining < 700 {
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
    fn test_primer_tier_from_budget() {
        // RFC-0015 tier thresholds
        assert_eq!(PrimerTier::from_budget(50), PrimerTier::Micro);
        assert_eq!(PrimerTier::from_budget(299), PrimerTier::Micro);
        assert_eq!(PrimerTier::from_budget(300), PrimerTier::Minimal);
        assert_eq!(PrimerTier::from_budget(449), PrimerTier::Minimal);
        assert_eq!(PrimerTier::from_budget(450), PrimerTier::Standard);
        assert_eq!(PrimerTier::from_budget(699), PrimerTier::Standard);
        assert_eq!(PrimerTier::from_budget(700), PrimerTier::Full);
        assert_eq!(PrimerTier::from_budget(1000), PrimerTier::Full);
    }

    #[test]
    fn test_legacy_tier_from_budget() {
        // Legacy mapping (for backward compatibility)
        assert_eq!(Tier::from_budget(50), Tier::Minimal);
        assert_eq!(Tier::from_budget(299), Tier::Minimal);
        assert_eq!(Tier::from_budget(300), Tier::Standard);
        assert_eq!(Tier::from_budget(699), Tier::Standard);
        assert_eq!(Tier::from_budget(700), Tier::Full);
    }

    #[test]
    fn test_generate_micro_primer() {
        let options = PrimerOptions {
            budget: 60,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        assert!(result.total_tokens <= 300 || result.sections_included >= 1);
        assert_eq!(result.tier, PrimerTier::Micro);
    }

    #[test]
    fn test_generate_minimal_primer() {
        let options = PrimerOptions {
            budget: 350,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        assert_eq!(result.tier, PrimerTier::Minimal);
        assert!(result.sections_included >= 1);
    }

    #[test]
    fn test_generate_standard_primer() {
        let options = PrimerOptions {
            budget: 500,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        assert_eq!(result.tier, PrimerTier::Standard);
        assert!(result.sections_included >= 1);
    }

    #[test]
    fn test_generate_full_primer() {
        let options = PrimerOptions {
            budget: 800,
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        assert_eq!(result.tier, PrimerTier::Full);
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
            budget: 500,
            capabilities: vec!["mcp".to_string()],
            ..Default::default()
        };

        let result = generate_primer(&options).unwrap();
        // With MCP capability, should get MCP sections only
        assert!(result.sections_included >= 1);
    }

    #[test]
    fn test_primer_tier_names() {
        assert_eq!(PrimerTier::Micro.name(), "micro");
        assert_eq!(PrimerTier::Minimal.name(), "minimal");
        assert_eq!(PrimerTier::Standard.name(), "standard");
        assert_eq!(PrimerTier::Full.name(), "full");
    }

    #[test]
    fn test_primer_tier_tokens() {
        assert_eq!(PrimerTier::Micro.cli_tokens(), 250);
        assert_eq!(PrimerTier::Micro.mcp_tokens(), 178);
        assert_eq!(PrimerTier::Standard.cli_tokens(), 600);
        assert_eq!(PrimerTier::Full.cli_tokens(), 1400);
    }
}
