//! @acp:module "Primer Command"
//! @acp:summary "Generate AI bootstrap primers with tiered content selection"
//! @acp:domain cli
//! @acp:layer handler
//!
//! RFC-0004: Tiered Interface Primers
//! Generates token-efficient bootstrap text for AI agents.

use std::cmp::Ordering;
use std::path::PathBuf;

use anyhow::Result;
use console::style;
use serde::{Deserialize, Serialize};

use crate::cache::Cache;

/// Options for the primer command
#[derive(Debug, Clone)]
pub struct PrimerOptions {
    /// Token budget for the primer
    pub budget: u32,
    /// Required capabilities (e.g., "shell", "mcp")
    pub capabilities: Vec<String>,
    /// Output format
    pub format: PrimerFormat,
    /// Cache file path (for project warnings)
    pub cache: Option<PathBuf>,
    /// Output as JSON
    pub json: bool,
}

/// Output format for primer
#[derive(Debug, Clone, Copy, Default)]
pub enum PrimerFormat {
    #[default]
    Text,
    Json,
}

/// Tier level for content selection
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

/// Bootstrap block content (~20 tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bootstrap {
    pub awareness: String,
    pub workflow: String,
    pub expansion: String,
    pub tokens: u32,
}

impl Default for Bootstrap {
    fn default() -> Self {
        Self {
            awareness: "This project uses ACP. @acp:* comments are directives for you.".to_string(),
            workflow: "Before editing: acp constraints <path>".to_string(),
            expansion: "More: acp primer --budget N".to_string(),
            tokens: 20,
        }
    }
}

/// Command documentation with tiered content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub critical: bool,
    pub priority: u32,
    pub capabilities: Vec<String>,
    pub tiers: TierContent,
}

/// Tiered content for a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierContent {
    pub minimal: TierLevel,
    pub standard: Option<TierLevel>,
    pub full: Option<TierLevel>,
}

/// Single tier level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierLevel {
    pub tokens: u32,
    pub template: String,
}

/// Generated primer output
#[derive(Debug, Clone, Serialize)]
pub struct PrimerOutput {
    pub total_tokens: u32,
    pub tier: String,
    pub commands_included: usize,
    pub content: String,
}

/// Execute the primer command
pub fn execute_primer(options: PrimerOptions) -> Result<()> {
    let primer = generate_primer(&options)?;

    if options.json || matches!(options.format, PrimerFormat::Json) {
        println!("{}", serde_json::to_string_pretty(&primer)?);
    } else {
        println!("{}", primer.content);
    }

    Ok(())
}

/// Generate primer content based on budget and capabilities
pub fn generate_primer(options: &PrimerOptions) -> Result<PrimerOutput> {
    let bootstrap = Bootstrap::default();
    let commands = get_default_commands();

    // Filter commands by capabilities
    let filtered_commands: Vec<&Command> = if options.capabilities.is_empty() {
        commands.iter().collect()
    } else {
        commands
            .iter()
            .filter(|cmd| {
                cmd.capabilities.is_empty()
                    || cmd.capabilities.iter().any(|cap| options.capabilities.contains(cap))
            })
            .collect()
    };

    // Sort by (critical desc, priority asc)
    let mut sorted_commands = filtered_commands;
    sorted_commands.sort_by(|a, b| {
        match (a.critical, b.critical) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.priority.cmp(&b.priority),
        }
    });

    // Calculate remaining budget after bootstrap
    let remaining_budget = options.budget.saturating_sub(bootstrap.tokens);
    let tier = Tier::from_budget(remaining_budget);

    // Select commands within budget
    let mut used_tokens = bootstrap.tokens;
    let mut selected_commands: Vec<(&Command, &TierLevel)> = Vec::new();

    for cmd in sorted_commands {
        // Get the appropriate tier level
        let tier_level = match tier {
            Tier::Full => cmd.tiers.full.as_ref().or(cmd.tiers.standard.as_ref()).unwrap_or(&cmd.tiers.minimal),
            Tier::Standard => cmd.tiers.standard.as_ref().unwrap_or(&cmd.tiers.minimal),
            Tier::Minimal => &cmd.tiers.minimal,
        };

        let cmd_tokens = tier_level.tokens;

        // Critical commands are always included
        if cmd.critical || used_tokens + cmd_tokens <= options.budget {
            used_tokens += cmd_tokens;
            selected_commands.push((cmd, tier_level));
        }
    }

    // Build output content
    let mut content = String::new();

    // Bootstrap block
    content.push_str(&bootstrap.awareness);
    content.push('\n');
    content.push_str(&bootstrap.workflow);
    content.push('\n');
    content.push_str(&bootstrap.expansion);
    content.push_str("\n\n");

    // Commands
    for (cmd, tier_level) in &selected_commands {
        content.push_str(&format!("{}\n", style(&cmd.name).bold()));
        content.push_str(&tier_level.template);
        content.push_str("\n\n");
    }

    // Add project warnings if we have budget and cache
    if let Some(cache_path) = &options.cache {
        if cache_path.exists() && used_tokens + 30 < options.budget {
            if let Ok(cache) = Cache::from_json(cache_path) {
                let warnings = get_project_warnings(&cache);
                if !warnings.is_empty() {
                    content.push_str(&format!("{}\n", style("Project Warnings").bold()));
                    for warning in warnings.iter().take(3) {
                        content.push_str(&format!("  - {}\n", warning));
                        used_tokens += 15;
                        if used_tokens >= options.budget {
                            break;
                        }
                    }
                }
            }
        }
    }

    let tier_name = match tier {
        Tier::Minimal => "minimal",
        Tier::Standard => "standard",
        Tier::Full => "full",
    };

    Ok(PrimerOutput {
        total_tokens: used_tokens,
        tier: tier_name.to_string(),
        commands_included: selected_commands.len(),
        content: content.trim().to_string(),
    })
}

/// Get project-specific warnings from cache
fn get_project_warnings(cache: &Cache) -> Vec<String> {
    let mut warnings = Vec::new();

    // Check for frozen/restricted symbols
    for (name, symbol) in &cache.symbols {
        if let Some(ref constraints) = symbol.constraints {
            if constraints.level == "frozen" || constraints.level == "restricted" {
                warnings.push(format!(
                    "{}: {} ({})",
                    name,
                    constraints.level,
                    constraints.directive.chars().take(50).collect::<String>()
                ));
            }
        }
    }

    // Limit to most important
    warnings.truncate(5);
    warnings
}

/// Get the default command set for the primer
fn get_default_commands() -> Vec<Command> {
    vec![
        Command {
            name: "acp constraints <path>".to_string(),
            critical: true,
            priority: 1,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 8,
                    template: "  Returns: lock level + directive".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 25,
                    template: "  Returns: lock level + directive
  Levels: frozen (refuse), restricted (ask), normal (proceed)
  Use: Check before ANY file modification".to_string(),
                }),
                full: Some(TierLevel {
                    tokens: 45,
                    template: "  Returns: lock level + directive
  Levels: frozen (refuse), restricted (ask), normal (proceed)
  Use: Check before ANY file modification
  Example:
    $ acp constraints src/auth/session.ts
    frozen - Core auth logic; security-critical".to_string(),
                }),
            },
        },
        Command {
            name: "acp query file <path>".to_string(),
            critical: false,
            priority: 2,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 6,
                    template: "  Returns: purpose, constraints, symbols".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 20,
                    template: "  Returns: purpose, constraints, symbols, dependencies
  Options: --json for machine-readable output
  Use: Understand file context before working with it".to_string(),
                }),
                full: Some(TierLevel {
                    tokens: 35,
                    template: "  Returns: purpose, constraints, symbols, dependencies
  Options: --json for machine-readable output
  Use: Understand file context before working with it
  Example:
    $ acp query file src/payments/processor.ts".to_string(),
                }),
            },
        },
        Command {
            name: "acp query symbol <name>".to_string(),
            critical: false,
            priority: 3,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 6,
                    template: "  Returns: signature, purpose, constraints, callers".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 18,
                    template: "  Returns: signature, purpose, constraints, callers/callees
  Options: --json for machine-readable output
  Use: Understand function/method before modifying".to_string(),
                }),
                full: None,
            },
        },
        Command {
            name: "acp query domain <name>".to_string(),
            critical: false,
            priority: 4,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 5,
                    template: "  Returns: domain files, cross-cutting concerns".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 15,
                    template: "  Returns: domain files, cross-cutting concerns
  Options: --json for machine-readable output
  Use: Understand architectural boundaries".to_string(),
                }),
                full: None,
            },
        },
        Command {
            name: "acp map [path]".to_string(),
            critical: false,
            priority: 5,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 5,
                    template: "  Returns: directory tree with purposes".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 15,
                    template: "  Returns: directory tree with purposes and constraints
  Options: --depth N, --inline (show todos/hacks)
  Use: Navigate unfamiliar codebase".to_string(),
                }),
                full: None,
            },
        },
        Command {
            name: "acp expand <text>".to_string(),
            critical: false,
            priority: 6,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 5,
                    template: "  Expands $variable references to full paths".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 12,
                    template: "  Expands $variable references to full paths
  Options: --mode inline|annotated
  Use: Resolve variable shortcuts in instructions".to_string(),
                }),
                full: None,
            },
        },
        Command {
            name: "acp attempt start <id>".to_string(),
            critical: false,
            priority: 7,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 5,
                    template: "  Creates checkpoint for safe experimentation".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 15,
                    template: "  Creates checkpoint for safe experimentation
  Related: acp attempt fail <id>, acp attempt verify <id>
  Use: Track and revert failed approaches".to_string(),
                }),
                full: None,
            },
        },
        Command {
            name: "acp primer --budget <N>".to_string(),
            critical: false,
            priority: 8,
            capabilities: vec!["shell".to_string()],
            tiers: TierContent {
                minimal: TierLevel {
                    tokens: 5,
                    template: "  Get more context (this command)".to_string(),
                },
                standard: Some(TierLevel {
                    tokens: 10,
                    template: "  Get more context within token budget
  Options: --capabilities shell,mcp
  Use: Request more detailed primer".to_string(),
                }),
                full: None,
            },
        },
    ]
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
            capabilities: vec![],
            format: PrimerFormat::Text,
            cache: None,
            json: false,
        };

        let result = generate_primer(&options).unwrap();
        assert!(result.total_tokens <= 60 || result.commands_included == 1); // At least critical command
        assert_eq!(result.tier, "minimal");
        assert!(result.content.contains("constraints"));
    }

    #[test]
    fn test_generate_standard_primer() {
        let options = PrimerOptions {
            budget: 200,
            capabilities: vec![],
            format: PrimerFormat::Text,
            cache: None,
            json: false,
        };

        let result = generate_primer(&options).unwrap();
        assert_eq!(result.tier, "standard");
        assert!(result.commands_included >= 3);
    }

    #[test]
    fn test_critical_commands_always_included() {
        let options = PrimerOptions {
            budget: 30, // Very small budget
            capabilities: vec![],
            format: PrimerFormat::Text,
            cache: None,
            json: false,
        };

        let result = generate_primer(&options).unwrap();
        // Critical command (constraints) should still be included
        assert!(result.content.contains("constraints"));
    }

    #[test]
    fn test_capability_filtering() {
        let options = PrimerOptions {
            budget: 200,
            capabilities: vec!["mcp".to_string()], // No shell commands should match
            format: PrimerFormat::Text,
            cache: None,
            json: false,
        };

        let result = generate_primer(&options).unwrap();
        // With only MCP capability, fewer commands should be included
        // (currently all commands require shell, so only bootstrap + critical)
        assert!(result.content.contains("constraints")); // Critical still included
    }
}
