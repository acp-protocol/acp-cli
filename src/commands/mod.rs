//! @acp:module "Commands"
//! @acp:summary "CLI command implementations"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Provides implementations for all CLI commands.
//! Each command is in its own submodule for maintainability.

pub mod annotate;
pub mod attempt;
pub mod chain;
pub mod check;
pub mod daemon;
pub mod expand;
pub mod index;
pub mod init;
pub mod install;
pub mod map;
pub mod migrate;
pub mod output;
pub mod query;
pub mod review;
pub mod revert;
pub mod validate;
pub mod vars;
pub mod watch;

pub use annotate::{execute_annotate, AnnotateOptions};
pub use attempt::{execute_attempt, AttemptSubcommand};
pub use chain::{execute_chain, ChainOptions};
pub use check::{execute_check, CheckOptions};
pub use daemon::{execute_daemon, DaemonSubcommand};
pub use expand::{execute_expand, ExpandOptions};
pub use index::{execute_index, IndexOptions};
pub use init::{execute_init, InitOptions};
pub use install::{execute_install, execute_list_installed, execute_uninstall, InstallOptions, InstallTarget};
pub use map::{execute_map, MapBuilder, MapFormat, MapOptions};
pub use migrate::{execute_migrate, DirectiveDefaults, MigrateOptions, MigrationScanner};
pub use output::{format_constraint_level, format_symbol_ref, format_symbol_ref_range, TreeRenderer};
pub use query::{execute_query, ConfidenceFilter, QueryOptions, QuerySubcommand};
pub use review::{execute_review, ReviewOptions, ReviewSubcommand};
pub use revert::{execute_revert, RevertOptions};
pub use validate::{execute_validate, ValidateOptions};
pub use vars::{execute_vars, VarsOptions};
pub use watch::{execute_watch, WatchOptions};
