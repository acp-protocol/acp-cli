#![forbid(unsafe_code)]

//! @acp:module "ACP Library"
//! @acp:summary "Token-efficient code documentation and indexing for AI systems"
//! @acp:domain cli
//! @acp:layer api
//! @acp:stability stable
//!
//! # ACP - AI Context Protocol
//!
//! Token-efficient code documentation and indexing for AI systems.
//!
//! ## Features
//!
//! - **Fast Parsing**: Uses tree-sitter for accurate AST parsing
//! - **JSON Output**: Queryable with jq for O(1) lookups
//! - **Variable System**: Token-efficient macros with inheritance
//! - **Multi-language**: TypeScript, JavaScript, Rust, Python, Go, Java
//!
//! ## Example
//!
//! ```rust,no_run
//! use acp::{Indexer, Config};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Config::default();
//!     let indexer = Indexer::new(config)?;
//!
//!     // Index codebase
//!     let cache = indexer.index(".").await?;
//!
//!     // Write JSON output
//!     cache.write_json(".acp.cache.json")?;
//!
//!     Ok(())
//! }
//! ```

pub mod annotate;
pub mod ast;
pub mod bridge;
pub mod cache;
pub mod commands;
pub mod config;
pub mod constraints;
pub mod error;
pub mod expand;
pub mod git;
pub mod index;
pub mod parse;
pub mod query;
pub mod scan;
pub mod schema;
pub mod vars;
pub mod watch;
pub mod attempts;
pub mod sync;

// Re-exports
pub use annotate::{
    Analyzer as AnnotationAnalyzer, Suggester as AnnotationSuggester,
    Writer as AnnotationWriter, AnnotateLevel, ConversionSource, OutputFormat,
    Suggestion, AnalysisResult, FileChange,
};
pub use cache::{Cache, CacheBuilder, Language};
pub use config::Config;
pub use constraints::{
    Constraints, ConstraintIndex,
    StyleConstraint, MutationConstraint, BehaviorModifier, QualityGate,
    HackMarker, DebugSession, DebugAttempt,
    LockLevel, DebugStatus, DebugResult,
    GuardrailEnforcer, FileGuardrails, GuardrailParser,
};
pub use error::{AcpError, Result};
pub use git::{GitRepository, BlameInfo, FileHistory, GitFileInfo, GitSymbolInfo};
pub use ast::{AstParser, ExtractedSymbol, SymbolKind, Visibility, Import, FunctionCall};
pub use index::Indexer;
pub use parse::Parser;
pub use query::Query;
pub use scan::{scan_project, ProjectScan};
pub use vars::{VarResolver, VarExpander};
pub use attempts::AttemptTracker;
pub use sync::{SyncExecutor, Tool as SyncTool, BootstrapResult, BootstrapAction};
pub use bridge::{BridgeConfig, BridgeMerger, BridgeResult, FormatDetector};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
