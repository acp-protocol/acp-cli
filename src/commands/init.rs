//! @acp:module "Init Command"
//! @acp:summary "Initialize a new ACP project"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Implements `acp init` command for project initialization.

use std::path::PathBuf;

use anyhow::Result;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect};

use crate::config::Config;
use crate::scan::scan_project;
use crate::sync::{SyncExecutor, Tool as SyncTool};

/// Options for the init command
#[derive(Debug, Clone, Default)]
pub struct InitOptions {
    /// Force overwrite existing config
    pub force: bool,
    /// File patterns to include
    pub include: Vec<String>,
    /// File patterns to exclude
    pub exclude: Vec<String>,
    /// Cache file output path
    pub cache_path: Option<PathBuf>,
    /// Vars file output path
    pub vars_path: Option<PathBuf>,
    /// Number of parallel workers
    pub workers: Option<usize>,
    /// Skip interactive prompts
    pub yes: bool,
    /// Skip AI tool bootstrap
    pub no_bootstrap: bool,
}

/// Execute the init command
pub fn execute_init(options: InitOptions) -> Result<()> {
    let config_path = PathBuf::from(".acp.config.json");

    if config_path.exists() && !options.force {
        eprintln!(
            "{} Config file already exists. Use --force to overwrite.",
            style("✗").red()
        );
        std::process::exit(1);
    }

    let mut config = Config::default();

    // Interactive mode if no CLI options and not using --yes
    let interactive = !options.yes
        && options.include.is_empty()
        && options.exclude.is_empty()
        && options.cache_path.is_none()
        && options.vars_path.is_none()
        && options.workers.is_none();

    if interactive {
        run_interactive_init(&mut config)?;
    } else {
        apply_cli_options(&mut config, &options);
    }

    // Create .acp directory
    let acp_dir = PathBuf::from(".acp");
    if !acp_dir.exists() {
        std::fs::create_dir(&acp_dir)?;
        println!("{} Created .acp/ directory", style("✓").green());
    }

    // Write config
    config.save(&config_path)?;
    println!(
        "{} Created {}",
        style("✓").green(),
        config_path.display()
    );

    // Bootstrap AI tool files
    if !options.no_bootstrap {
        bootstrap_ai_tools(interactive)?;
    }

    // Print next steps
    println!("\n{}", style("Next steps:").bold());
    println!(
        "  1. Run {} to index your codebase",
        style("acp index").cyan()
    );
    println!("  2. AI tools will read context from generated files");

    Ok(())
}

fn run_interactive_init(config: &mut Config) -> Result<()> {
    println!("{} ACP Project Setup\n", style("→").cyan());

    // Scan project to detect languages
    println!("{} Scanning project...", style("→").dim());
    let scan = scan_project(".");

    if scan.languages.is_empty() {
        println!("{} No supported languages detected\n", style("⚠").yellow());
    } else {
        println!("{} Detected languages:", style("✓").green());
        for lang in &scan.languages {
            println!(
                "    {} ({} files)",
                style(lang.name).cyan(),
                lang.file_count
            );
        }
        println!();

        // Auto-populate include patterns from detected languages
        let mut include_patterns: Vec<String> = vec![];
        for lang in &scan.languages {
            include_patterns.extend(lang.patterns.iter().map(|s| s.to_string()));
        }
        config.include = include_patterns;

        // Ask to confirm or modify
        let use_detected = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Use detected languages?")
            .default(true)
            .interact()?;

        if !use_detected {
            select_languages_manually(config)?;
        }
    }

    // Custom excludes
    let add_excludes = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Add custom exclude patterns? (node_modules, dist, etc. already excluded)")
        .default(false)
        .interact()?;

    if add_excludes {
        let custom: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter patterns (comma-separated)")
            .interact_text()?;
        config
            .exclude
            .extend(custom.split(',').map(|s| s.trim().to_string()));
    }

    Ok(())
}

fn select_languages_manually(config: &mut Config) -> Result<()> {
    let all_languages = [
        ("TypeScript/TSX", vec!["**/*.ts", "**/*.tsx"]),
        (
            "JavaScript/JSX",
            vec!["**/*.js", "**/*.jsx", "**/*.mjs"],
        ),
        ("Rust", vec!["**/*.rs"]),
        ("Python", vec!["**/*.py"]),
        ("Go", vec!["**/*.go"]),
        ("Java", vec!["**/*.java"]),
    ];

    let items: Vec<&str> = all_languages.iter().map(|(name, _)| *name).collect();
    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select languages to index")
        .items(&items)
        .interact()?;

    config.include = selections
        .iter()
        .flat_map(|&idx| all_languages[idx].1.iter().map(|s| s.to_string()))
        .collect();

    Ok(())
}

fn apply_cli_options(config: &mut Config, options: &InitOptions) {
    if !options.include.is_empty() {
        config.include = options.include.clone();
    }
    if !options.exclude.is_empty() {
        config.exclude.extend(options.exclude.iter().cloned());
    }
    // Output paths are handled via Config helper methods
    // cache_path and vars_path can be passed to commands directly
}

fn bootstrap_ai_tools(interactive: bool) -> Result<()> {
    let sync = SyncExecutor::new();
    let project_root = PathBuf::from(".");
    let detected = sync.detect_tools(&project_root);

    if !detected.is_empty() {
        println!("\n{} Detected AI tools:", style("✓").green());
        for tool in &detected {
            println!(
                "    {} ({})",
                style(tool.name()).cyan(),
                tool.output_path()
            );
        }

        // In interactive mode, confirm; in non-interactive, just do it
        let should_bootstrap = if interactive {
            Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Bootstrap detected tools with ACP context?")
                .default(true)
                .interact()?
        } else {
            true
        };

        if should_bootstrap {
            println!();
            for tool in detected {
                match sync.bootstrap_tool(tool, &project_root) {
                    Ok(result) => {
                        let action = match result.action {
                            crate::sync::BootstrapAction::Created => "Created",
                            crate::sync::BootstrapAction::Merged => "Updated",
                            crate::sync::BootstrapAction::Skipped => "Skipped",
                        };
                        println!(
                            "{} {} {}",
                            style("✓").green(),
                            action,
                            result.output_path.display()
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed {}: {}",
                            style("✗").red(),
                            tool.output_path(),
                            e
                        );
                    }
                }
            }
        }
    }

    // Always create AGENTS.md as fallback if it doesn't exist
    let agents_md = project_root.join("AGENTS.md");
    if !agents_md.exists() {
        match sync.bootstrap_tool(SyncTool::Generic, &project_root) {
            Ok(result) => {
                println!(
                    "{} Created {} (universal fallback)",
                    style("✓").green(),
                    result.output_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to create AGENTS.md: {}",
                    style("✗").red(),
                    e
                );
            }
        }
    }

    Ok(())
}
