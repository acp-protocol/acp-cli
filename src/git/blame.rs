//! @acp:module "Git Blame"
//! @acp:summary "Line-level blame tracking for authorship information"
//! @acp:domain cli
//! @acp:layer integration

use std::collections::HashMap;
use std::path::Path;
use chrono::{DateTime, TimeZone, Utc};
use git2::{BlameOptions, Oid};
use serde::{Deserialize, Serialize};

use super::repository::GitRepository;
use crate::error::{AcpError, Result};

/// Blame information for a single line
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineBlame {
    /// Commit SHA that last modified this line
    pub commit: String,
    /// Short commit SHA (7 chars)
    pub commit_short: String,
    /// Author name
    pub author: String,
    /// Author email
    pub author_email: String,
    /// Timestamp of the commit
    pub timestamp: DateTime<Utc>,
    /// Commit summary (first line of message)
    pub summary: String,
}

/// Blame information for an entire file
#[derive(Debug, Clone)]
pub struct BlameInfo {
    /// Line number (1-indexed) to blame info mapping
    lines: HashMap<usize, LineBlame>,
    /// Path of the file
    path: String,
}

impl BlameInfo {
    /// Get blame information for a file
    pub fn for_file(repo: &GitRepository, path: &Path) -> Result<Self> {
        let relative_path = Self::make_relative_path(repo, path)?;

        let mut opts = BlameOptions::new();
        opts.track_copies_same_commit_moves(true)
            .track_copies_same_commit_copies(true);

        let blame = repo.inner().blame_file(Path::new(&relative_path), Some(&mut opts))
            .map_err(|e| AcpError::Other(format!("Failed to get blame for {}: {}", relative_path, e)))?;

        let mut lines = HashMap::new();

        for hunk in blame.iter() {
            let sig = hunk.final_signature();
            let commit_id = hunk.final_commit_id();

            // Get commit message summary
            let summary = Self::get_commit_summary(repo, commit_id)?;

            // Convert time to DateTime<Utc>
            let timestamp = Self::git_time_to_datetime(sig.when());

            let line_blame = LineBlame {
                commit: commit_id.to_string(),
                commit_short: commit_id.to_string().chars().take(7).collect(),
                author: sig.name().unwrap_or("Unknown").to_string(),
                author_email: sig.email().unwrap_or("").to_string(),
                timestamp,
                summary: summary.clone(),
            };

            // Add entry for each line in the hunk
            let start_line = hunk.final_start_line();
            let num_lines = hunk.lines_in_hunk();

            for offset in 0..num_lines {
                lines.insert(start_line + offset, line_blame.clone());
            }
        }

        Ok(Self {
            lines,
            path: relative_path,
        })
    }

    /// Get blame info for a specific line (1-indexed)
    pub fn get_line(&self, line: usize) -> Option<&LineBlame> {
        self.lines.get(&line)
    }

    /// Get blame info for a range of lines (1-indexed, inclusive)
    pub fn for_lines(&self, start: usize, end: usize) -> Vec<&LineBlame> {
        (start..=end)
            .filter_map(|line| self.lines.get(&line))
            .collect()
    }

    /// Get the most recent blame for a line range (the newest modification)
    pub fn last_modified(&self, start: usize, end: usize) -> Option<&LineBlame> {
        (start..=end)
            .filter_map(|line| self.lines.get(&line))
            .max_by_key(|blame| blame.timestamp)
    }

    /// Get unique contributors for a line range
    pub fn contributors(&self, start: usize, end: usize) -> Vec<String> {
        let mut authors: Vec<String> = (start..=end)
            .filter_map(|line| self.lines.get(&line))
            .map(|blame| blame.author.clone())
            .collect();

        authors.sort();
        authors.dedup();
        authors
    }

    /// Calculate code age in days for a line range (based on oldest line)
    pub fn code_age_days(&self, start: usize, end: usize) -> Option<u32> {
        let oldest = (start..=end)
            .filter_map(|line| self.lines.get(&line))
            .min_by_key(|blame| blame.timestamp)?;

        let now = Utc::now();
        let duration = now.signed_duration_since(oldest.timestamp);
        Some(duration.num_days().max(0) as u32)
    }

    /// Get total number of lines with blame info
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get the file path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Helper: make path relative to repo root
    fn make_relative_path(repo: &GitRepository, path: &Path) -> Result<String> {
        let root = repo.root()?;

        let relative = if path.is_absolute() {
            path.strip_prefix(root)
                .map_err(|_| AcpError::Other(format!(
                    "Path {} is not within repository root {}",
                    path.display(),
                    root.display()
                )))?
                .to_path_buf()
        } else {
            // Path is relative - need to make it relative to repo root
            // Get current working directory and resolve the path
            let cwd = std::env::current_dir()
                .map_err(|e| AcpError::Other(format!("Failed to get current directory: {}", e)))?;
            let absolute = cwd.join(path);
            absolute.strip_prefix(root)
                .map_err(|_| AcpError::Other(format!(
                    "Path {} is not within repository root {}",
                    absolute.display(),
                    root.display()
                )))?
                .to_path_buf()
        };

        Ok(relative.to_string_lossy().to_string())
    }

    /// Helper: get commit summary from commit ID
    fn get_commit_summary(repo: &GitRepository, oid: Oid) -> Result<String> {
        let commit = repo.inner().find_commit(oid)
            .map_err(|e| AcpError::Other(format!("Failed to find commit {}: {}", oid, e)))?;

        Ok(commit.summary().unwrap_or("").to_string())
    }

    /// Helper: convert git2 time to chrono DateTime
    fn git_time_to_datetime(time: git2::Time) -> DateTime<Utc> {
        // git2::Time gives us seconds since epoch and offset in minutes
        Utc.timestamp_opt(time.seconds(), 0)
            .single()
            .unwrap_or_else(Utc::now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_blame_existing_file() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            // Try to blame Cargo.toml which should exist
            let cargo_path = cwd.join("cli/Cargo.toml");
            if cargo_path.exists() {
                let blame = BlameInfo::for_file(&repo, &cargo_path);
                assert!(blame.is_ok(), "Should be able to blame existing file");

                let info = blame.unwrap();
                assert!(info.line_count() > 0, "Should have blame info for lines");
            }
        }
    }

    #[test]
    fn test_line_blame() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let cargo_path = cwd.join("cli/Cargo.toml");
            if cargo_path.exists() {
                if let Ok(info) = BlameInfo::for_file(&repo, &cargo_path) {
                    // Line 1 should have blame info
                    let line1 = info.get_line(1);
                    assert!(line1.is_some(), "First line should have blame info");

                    if let Some(blame) = line1 {
                        assert!(!blame.author.is_empty());
                        assert_eq!(blame.commit.len(), 40);
                        assert_eq!(blame.commit_short.len(), 7);
                    }
                }
            }
        }
    }

    #[test]
    fn test_contributors() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let cargo_path = cwd.join("cli/Cargo.toml");
            if cargo_path.exists() {
                if let Ok(info) = BlameInfo::for_file(&repo, &cargo_path) {
                    let contributors = info.contributors(1, 10);
                    assert!(!contributors.is_empty(), "Should have at least one contributor");
                }
            }
        }
    }
}
