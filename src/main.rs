#![forbid(unsafe_code)]
//! ACP Command Line Interface

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use console::style;

use acp::{Config, Cache};
use acp::annotate::{AnnotateLevel, ConversionSource, OutputFormat};
use acp::commands::{
    execute_annotate, execute_attempt, execute_chain, execute_check, execute_daemon,
    execute_expand, execute_index, execute_init, execute_install, execute_list_installed,
    execute_map, execute_migrate, execute_query, execute_review, execute_revert, execute_uninstall,
    execute_validate, execute_vars, execute_watch, AnnotateOptions, AttemptSubcommand,
    ChainOptions, CheckOptions, DaemonSubcommand, ExpandOptions, IndexOptions, InitOptions,
    InstallOptions, InstallTarget, MapFormat, MapOptions, MigrateOptions, QueryOptions,
    QuerySubcommand, ReviewOptions, ReviewSubcommand, RevertOptions, ValidateOptions, VarsOptions,
    WatchOptions,
};

#[derive(Parser)]
#[command(name = "acp")]
#[command(about = "AI Context Protocol - Token-efficient code documentation")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, global = true, default_value = ".acp.config.json")]
    config: PathBuf,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new ACP project
    Init {
        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,

        /// File patterns to include (can specify multiple)
        #[arg(long)]
        include: Vec<String>,

        /// File patterns to exclude (can specify multiple)
        #[arg(long)]
        exclude: Vec<String>,

        /// Cache file output path
        #[arg(long)]
        cache_path: Option<PathBuf>,

        /// Vars file output path
        #[arg(long)]
        vars_path: Option<PathBuf>,

        /// Number of parallel workers
        #[arg(long)]
        workers: Option<usize>,

        /// Skip interactive prompts (use defaults + CLI args)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Skip AI tool bootstrap (don't create CLAUDE.md, .cursorrules, etc.)
        #[arg(long)]
        no_bootstrap: bool,
    },

    /// Install ACP plugins (daemon, mcp)
    Install {
        /// Plugins to install (daemon, mcp)
        #[arg(required = true)]
        targets: Vec<String>,

        /// Force reinstall even if already installed
        #[arg(short, long)]
        force: bool,

        /// Specific version to install (default: latest)
        #[arg(long)]
        version: Option<String>,

        /// List installed plugins instead of installing
        #[arg(long)]
        list: bool,

        /// Uninstall specified plugins
        #[arg(long)]
        uninstall: bool,
    },

    /// Index the codebase and generate cache
    Index {
        /// Root directory to index
        #[arg(default_value = ".")]
        root: PathBuf,

        /// Output cache file path
        #[arg(short, long, default_value = ".acp/acp.cache.json")]
        output: PathBuf,

        /// Also generate vars file
        #[arg(long)]
        vars: bool,
    },

    /// Generate vars file from cache
    Vars {
        /// Cache file to read
        #[arg(short, long, default_value = ".acp/acp.cache.json")]
        cache: PathBuf,

        /// Output vars file path
        #[arg(short, long, default_value = ".acp/acp.vars.json")]
        output: PathBuf,
    },

    /// Query the cache
    Query {
        /// Query type
        #[command(subcommand)]
        query: QueryCommands,

        /// Cache file to query
        #[arg(short, long, default_value = ".acp/acp.cache.json")]
        cache: PathBuf,

        /// Output as JSON (default: human-readable)
        #[arg(long, global = true)]
        json: bool,
    },

    /// Expand variable references in text
    Expand {
        /// Text to expand (reads from stdin if not provided)
        text: Option<String>,

        /// Expansion mode
        #[arg(short, long, default_value = "annotated")]
        mode: String,

        /// Vars file path
        #[arg(long, default_value = ".acp/acp.vars.json")]
        vars: PathBuf,

        /// Show inheritance chains
        #[arg(long)]
        chains: bool,
    },

    /// Show variable inheritance chain
    Chain {
        /// Variable name
        name: String,

        /// Vars file path
        #[arg(long, default_value = ".acp/acp.vars.json")]
        vars: PathBuf,

        /// Show as tree
        #[arg(long)]
        tree: bool,
    },

    /// Manage troubleshooting attempts
    Attempt {
        #[command(subcommand)]
        cmd: AttemptCommands,
    },

    /// Check guardrails for a file
    Check {
        /// File to check
        file: PathBuf,

        /// Cache file
        #[arg(short, long, default_value = ".acp/acp.cache.json")]
        cache: PathBuf,
    },

    /// Revert changes
    Revert {
        /// Attempt ID to revert
        #[arg(long)]
        attempt: Option<String>,

        /// Checkpoint name to restore
        #[arg(long)]
        checkpoint: Option<String>,
    },

    /// Watch for changes and update cache
    Watch {
        /// Root directory to watch
        #[arg(default_value = ".")]
        root: PathBuf,
    },

    /// Validate cache/vars files
    Validate {
        /// File to validate
        file: PathBuf,
    },

    /// Manage the ACP daemon
    Daemon {
        #[command(subcommand)]
        cmd: DaemonCommands,
    },

    /// Generate ACP annotations from code analysis and documentation conversion
    Annotate {
        /// Path to analyze (file or directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Apply changes to files (default: preview only)
        #[arg(long)]
        apply: bool,

        /// Convert-only mode: only use doc comment conversion, disable heuristics
        #[arg(long)]
        convert: bool,

        /// Source documentation standard to convert from
        #[arg(long, value_enum, default_value = "auto")]
        from: AnnotateFrom,

        /// Annotation generation level
        #[arg(long, value_enum, default_value = "standard")]
        level: AnnotateLevelArg,

        /// Output format
        #[arg(long, value_enum, default_value = "diff")]
        format: AnnotateFormat,

        /// Filter files by glob pattern
        #[arg(long)]
        filter: Option<String>,

        /// Only annotate files (skip symbols)
        #[arg(long)]
        files_only: bool,

        /// Only annotate symbols (skip file-level)
        #[arg(long)]
        symbols_only: bool,

        /// Exit with error if coverage below threshold (CI mode)
        #[arg(long)]
        check: bool,

        /// Minimum coverage threshold for --check (default: 80%)
        #[arg(long)]
        min_coverage: Option<f32>,

        /// Number of parallel workers (default: number of CPUs)
        #[arg(long, short = 'j')]
        workers: Option<usize>,

        /// RFC-0003: Disable provenance markers in generated annotations
        #[arg(long)]
        no_provenance: bool,

        /// RFC-0003: Mark all generated annotations as needing review
        #[arg(long)]
        mark_needs_review: bool,
    },

    /// RFC-0003: Review auto-generated annotations
    Review {
        /// Review subcommand
        #[command(subcommand)]
        cmd: ReviewCommands,

        /// Filter by source origin (explicit, converted, heuristic, refined, inferred)
        #[arg(long)]
        source: Option<String>,

        /// Filter by confidence expression (e.g., "<0.7", ">=0.9")
        #[arg(long)]
        confidence: Option<String>,

        /// Cache file path
        #[arg(long, default_value = ".acp/acp.cache.json")]
        cache: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Map directory structure with annotations (RFC-001)
    Map {
        /// Path to map (file or directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum directory depth
        #[arg(long, default_value = "3")]
        depth: usize,

        /// Show inline annotations (hacks, todos)
        #[arg(long)]
        inline: bool,

        /// Output format (tree, flat, json)
        #[arg(long, value_enum, default_value = "tree")]
        format: MapFormatArg,

        /// Cache file
        #[arg(short, long, default_value = ".acp/acp.cache.json")]
        cache: PathBuf,
    },

    /// Migrate annotations to RFC-001 format
    Migrate {
        /// Add directive suffixes to annotations
        #[arg(long)]
        add_directives: bool,

        /// Paths to migrate (default: all indexed files)
        paths: Vec<PathBuf>,

        /// Preview changes without applying
        #[arg(long)]
        dry_run: bool,

        /// Interactively confirm each file
        #[arg(long, short)]
        interactive: bool,

        /// Create backup before modifying (default: true)
        #[arg(long, default_value = "true")]
        backup: bool,

        /// Cache file
        #[arg(short, long, default_value = ".acp/acp.cache.json")]
        cache: PathBuf,
    },
}

/// Output format for map command
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default)]
enum MapFormatArg {
    #[default]
    Tree,
    Flat,
    Json,
}

/// Source documentation standard for annotation conversion
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default)]
enum AnnotateFrom {
    #[default]
    Auto,
    Jsdoc,
    Tsdoc,
    Docstring,
    Rustdoc,
    Godoc,
    Javadoc,
}

/// Annotation generation level
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default)]
enum AnnotateLevelArg {
    Minimal,
    #[default]
    Standard,
    Full,
}

/// Output format for annotation results
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default)]
enum AnnotateFormat {
    #[default]
    Diff,
    Json,
    Summary,
}

#[derive(Subcommand)]
enum AttemptCommands {
    /// Start a new attempt
    Start {
        /// Unique attempt ID
        id: String,

        /// Issue this is for
        #[arg(long, short = 'f')]
        for_issue: Option<String>,

        /// Description
        #[arg(long, short)]
        description: Option<String>,
    },

    /// List attempts
    List {
        /// Show only active attempts
        #[arg(long)]
        active: bool,

        /// Show only failed attempts
        #[arg(long)]
        failed: bool,

        /// Show history
        #[arg(long)]
        history: bool,
    },

    /// Mark attempt as failed
    Fail {
        /// Attempt ID
        id: String,

        /// Failure reason
        #[arg(long)]
        reason: Option<String>,
    },

    /// Mark attempt as verified
    Verify {
        /// Attempt ID
        id: String,
    },

    /// Revert an attempt
    Revert {
        /// Attempt ID
        id: String,
    },

    /// Clean up all failed attempts
    Cleanup,

    /// Create a checkpoint
    Checkpoint {
        /// Checkpoint name
        name: String,

        /// Files to checkpoint
        #[arg(long, short)]
        files: Vec<String>,

        /// Description
        #[arg(long)]
        description: Option<String>,
    },

    /// List checkpoints
    Checkpoints,

    /// Restore to checkpoint
    Restore {
        /// Checkpoint name
        name: String,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the ACP daemon
    Start {
        /// Run in foreground mode (don't daemonize)
        #[arg(long, short = 'f')]
        foreground: bool,

        /// HTTP server port
        #[arg(long, default_value = "9222")]
        port: u16,
    },

    /// Stop the ACP daemon
    Stop,

    /// Check daemon status
    Status,

    /// Show daemon logs
    Logs {
        /// Number of lines to show
        #[arg(short = 'n', default_value = "50")]
        lines: usize,

        /// Follow log output
        #[arg(short = 'f', long)]
        follow: bool,
    },
}

#[derive(Subcommand)]
enum QueryCommands {
    /// Query a symbol
    Symbol {
        /// Symbol name
        name: String,
    },

    /// Query a file
    File {
        /// File path
        path: String,
    },

    /// Get callers of a symbol
    Callers {
        /// Symbol name
        symbol: String,
    },

    /// Get callees of a symbol
    Callees {
        /// Symbol name
        symbol: String,
    },

    /// List domains
    Domains,

    /// Query a domain
    Domain {
        /// Domain name
        name: String,
    },

    /// List hotpaths
    Hotpaths,

    /// Show stats
    Stats,

    /// RFC-0003: Show provenance statistics
    Provenance,
}

/// RFC-0003: Review subcommands
#[derive(Subcommand)]
enum ReviewCommands {
    /// List annotations needing review
    List,

    /// Mark annotations as reviewed
    Mark {
        /// Filter by file path
        #[arg(long)]
        file: Option<PathBuf>,

        /// Filter by symbol name
        #[arg(long)]
        symbol: Option<String>,

        /// Mark all matching annotations as reviewed
        #[arg(long)]
        all: bool,
    },

    /// Interactive review mode
    Interactive,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config
    let config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        Config::default()
    };

    // Check for config requirement (most commands require .acp.config.json)
    let requires_config = !matches!(
        cli.command,
        Commands::Init { .. } | Commands::Install { .. } | Commands::Validate { .. } | Commands::Daemon { .. }
    );
    if requires_config {
        let config_path = PathBuf::from(".acp.config.json");
        if !config_path.exists() {
            eprintln!("{} No .acp.config.json found in project root", style("✗").red());
            eprintln!("  Run 'acp init' to initialize the project");
            eprintln!("  Use 'acp init --help' for configuration options");
            std::process::exit(1);
        }
    }

    match cli.command {
        Commands::Init { force, include, exclude, cache_path, vars_path, workers, yes, no_bootstrap } => {
            let options = InitOptions {
                force,
                include,
                exclude,
                cache_path,
                vars_path,
                workers,
                yes,
                no_bootstrap,
            };
            execute_init(options)?;
        }

        Commands::Install { targets, force, version, list, uninstall } => {
            if list {
                execute_list_installed()?;
            } else if uninstall {
                let install_targets: Vec<InstallTarget> = targets
                    .iter()
                    .map(|t| t.parse::<InstallTarget>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e: String| anyhow::anyhow!(e))?;
                execute_uninstall(install_targets)?;
            } else {
                let install_targets: Vec<InstallTarget> = targets
                    .iter()
                    .map(|t| t.parse::<InstallTarget>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e: String| anyhow::anyhow!(e))?;
                let options = InstallOptions {
                    targets: install_targets,
                    force,
                    version,
                };
                execute_install(options)?;
            }
        }

        Commands::Index { root, output, vars } => {
            let options = IndexOptions { root, output, vars };
            execute_index(options, config).await?;
        }

        Commands::Vars { cache, output } => {
            let options = VarsOptions { cache, output };
            execute_vars(options)?;
        }

        Commands::Query { query, cache, json } => {
            let options = QueryOptions {
                cache,
                json,
                source: None,
                confidence: None,
                needs_review: false,
            };
            let subcommand = match query {
                QueryCommands::Symbol { name } => QuerySubcommand::Symbol { name },
                QueryCommands::File { path } => QuerySubcommand::File { path },
                QueryCommands::Callers { symbol } => QuerySubcommand::Callers { symbol },
                QueryCommands::Callees { symbol } => QuerySubcommand::Callees { symbol },
                QueryCommands::Domains => QuerySubcommand::Domains,
                QueryCommands::Domain { name } => QuerySubcommand::Domain { name },
                QueryCommands::Hotpaths => QuerySubcommand::Hotpaths,
                QueryCommands::Stats => QuerySubcommand::Stats,
                QueryCommands::Provenance => QuerySubcommand::Provenance,
            };
            execute_query(options, subcommand)?;
        }

        Commands::Expand { text, mode, vars, chains } => {
            let options = ExpandOptions { text, mode, vars, chains };
            execute_expand(options)?;
        }

        Commands::Chain { name, vars, tree } => {
            let options = ChainOptions { name, vars, tree };
            execute_chain(options)?;
        }

        Commands::Watch { root } => {
            let options = WatchOptions { root };
            execute_watch(options, config)?;
        }

        Commands::Attempt { cmd } => {
            let subcommand = match cmd {
                AttemptCommands::Start { id, for_issue, description } => {
                    AttemptSubcommand::Start { id, for_issue, description }
                }
                AttemptCommands::List { active, failed, history } => {
                    AttemptSubcommand::List { active, failed, history }
                }
                AttemptCommands::Fail { id, reason } => {
                    AttemptSubcommand::Fail { id, reason }
                }
                AttemptCommands::Verify { id } => AttemptSubcommand::Verify { id },
                AttemptCommands::Revert { id } => AttemptSubcommand::Revert { id },
                AttemptCommands::Cleanup => AttemptSubcommand::Cleanup,
                AttemptCommands::Checkpoint { name, files, description } => {
                    AttemptSubcommand::Checkpoint { name, files, description }
                }
                AttemptCommands::Checkpoints => AttemptSubcommand::Checkpoints,
                AttemptCommands::Restore { name } => AttemptSubcommand::Restore { name },
            };
            execute_attempt(subcommand)?;
        }

        Commands::Check { file, cache } => {
            let options = CheckOptions { file, cache };
            execute_check(options)?;
        }

        Commands::Revert { attempt, checkpoint } => {
            let options = RevertOptions { attempt, checkpoint };
            execute_revert(options)?;
        }

        Commands::Validate { file } => {
            let options = ValidateOptions { file };
            execute_validate(options)?;
        }

        Commands::Daemon { cmd } => {
            let subcommand = match cmd {
                DaemonCommands::Start { foreground, port } => {
                    DaemonSubcommand::Start { foreground, port }
                }
                DaemonCommands::Stop => DaemonSubcommand::Stop,
                DaemonCommands::Status => DaemonSubcommand::Status,
                DaemonCommands::Logs { lines, follow } => {
                    DaemonSubcommand::Logs { lines, follow }
                }
            };
            execute_daemon(subcommand)?;
        }

        Commands::Annotate {
            path,
            apply,
            convert,
            from,
            level,
            format,
            filter,
            files_only,
            symbols_only,
            check,
            min_coverage,
            workers,
            no_provenance,
            mark_needs_review,
        } => {
            // Convert CLI enums to library types
            let annotate_level = match level {
                AnnotateLevelArg::Minimal => AnnotateLevel::Minimal,
                AnnotateLevelArg::Standard => AnnotateLevel::Standard,
                AnnotateLevelArg::Full => AnnotateLevel::Full,
            };

            let conversion_source = match from {
                AnnotateFrom::Auto => ConversionSource::Auto,
                AnnotateFrom::Jsdoc => ConversionSource::Jsdoc,
                AnnotateFrom::Tsdoc => ConversionSource::Tsdoc,
                AnnotateFrom::Docstring => ConversionSource::Docstring,
                AnnotateFrom::Rustdoc => ConversionSource::Rustdoc,
                AnnotateFrom::Godoc => ConversionSource::Godoc,
                AnnotateFrom::Javadoc => ConversionSource::Javadoc,
            };

            let output_format = match format {
                AnnotateFormat::Diff => OutputFormat::Diff,
                AnnotateFormat::Json => OutputFormat::Json,
                AnnotateFormat::Summary => OutputFormat::Summary,
            };

            let options = AnnotateOptions {
                path,
                apply,
                convert,
                from: conversion_source,
                level: annotate_level,
                format: output_format,
                filter,
                files_only,
                symbols_only,
                check,
                min_coverage,
                workers,
                verbose: cli.verbose,
                no_provenance,
                mark_needs_review,
            };

            execute_annotate(options, config)?;
        }

        Commands::Review { cmd, source, confidence, cache, json } => {
            let options = ReviewOptions {
                cache,
                source: source.and_then(|s| s.parse().ok()),
                confidence,
                json,
            };
            let subcommand = match cmd {
                ReviewCommands::List => ReviewSubcommand::List,
                ReviewCommands::Mark { file, symbol, all } => {
                    ReviewSubcommand::Mark { file, symbol, all }
                }
                ReviewCommands::Interactive => ReviewSubcommand::Interactive,
            };
            execute_review(options, subcommand)?;
        }

        Commands::Map { path, depth, inline, format, cache } => {
            let cache_data = Cache::from_json(&cache)?;

            let map_format = match format {
                MapFormatArg::Tree => MapFormat::Tree,
                MapFormatArg::Flat => MapFormat::Flat,
                MapFormatArg::Json => MapFormat::Json,
            };

            let options = MapOptions {
                depth,
                show_inline: inline,
                format: map_format,
            };

            execute_map(&cache_data, &path, options)?;
        }

        Commands::Migrate { add_directives, paths, dry_run, interactive, backup, cache } => {
            if !add_directives {
                eprintln!("{} Currently only --add-directives is supported", style("!").yellow());
                eprintln!("  Run: acp migrate --add-directives");
                std::process::exit(1);
            }

            let cache_data = Cache::from_json(&cache)?;

            let options = MigrateOptions {
                paths,
                dry_run,
                interactive,
                backup,
            };

            execute_migrate(&cache_data, options)?;
        }
    }

    Ok(())
}

