//! @acp:module "Git Integration"
//! @acp:summary "Repository operations, blame tracking, and file history via libgit2"
//! @acp:domain cli
//! @acp:layer integration
//!
//! # Git Integration
//!
//! Provides git repository operations using libgit2 (via git2 crate):
//! - Repository operations (HEAD, branch, remotes)
//! - Blame tracking (line-level authorship)
//! - File history (commits, contributors)

pub mod repository;
pub mod blame;
pub mod history;

pub use repository::{GitRepository, FileStatus};
pub use blame::{BlameInfo, LineBlame};
pub use history::{FileHistory, HistoryEntry};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Git metadata for a file in the cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileInfo {
    /// SHA of the last commit that modified this file
    pub last_commit: String,
    /// Author name of the last commit
    pub last_author: String,
    /// Timestamp of the last modification
    pub last_modified: DateTime<Utc>,
    /// Total number of commits that touched this file
    pub commit_count: usize,
    /// List of unique contributors to this file
    pub contributors: Vec<String>,
}

/// Git metadata for a symbol in the cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSymbolInfo {
    /// SHA of the last commit that modified this symbol's code
    pub last_commit: String,
    /// Author name of the last commit
    pub last_author: String,
    /// Age of the code in days (since last modification)
    pub code_age_days: u32,
}
