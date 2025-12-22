//! @acp:module "Git History"
//! @acp:summary "File commit history and contributor tracking"
//! @acp:domain cli
//! @acp:layer integration

use std::path::Path;
use chrono::{DateTime, TimeZone, Utc};
use git2::{Oid, DiffOptions};
use serde::{Deserialize, Serialize};

use super::repository::GitRepository;
use crate::error::{AcpError, Result};

/// A single entry in file history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Commit SHA
    pub commit: String,
    /// Short commit SHA (7 chars)
    pub commit_short: String,
    /// Author name
    pub author: String,
    /// Author email
    pub author_email: String,
    /// Commit timestamp
    pub timestamp: DateTime<Utc>,
    /// Commit message (first line)
    pub message: String,
    /// Lines added in this commit (for this file)
    pub lines_added: usize,
    /// Lines removed in this commit (for this file)
    pub lines_removed: usize,
}

/// File history containing commits that touched a specific file
#[derive(Debug, Clone)]
pub struct FileHistory {
    /// Ordered list of history entries (newest first)
    commits: Vec<HistoryEntry>,
    /// Path of the file
    path: String,
}

impl FileHistory {
    /// Get the commit history for a file
    ///
    /// # Arguments
    /// * `repo` - The git repository
    /// * `path` - Path to the file
    /// * `limit` - Maximum number of commits to retrieve (0 = unlimited)
    pub fn for_file(repo: &GitRepository, path: &Path, limit: usize) -> Result<Self> {
        let relative_path = Self::make_relative_path(repo, path)?;

        let mut revwalk = repo.inner().revwalk()
            .map_err(|e| AcpError::Other(format!("Failed to create revwalk: {}", e)))?;

        // Start from HEAD
        revwalk.push_head()
            .map_err(|e| AcpError::Other(format!("Failed to push HEAD: {}", e)))?;

        // Sort by time (newest first)
        revwalk.set_sorting(git2::Sort::TIME)
            .map_err(|e| AcpError::Other(format!("Failed to set sorting: {}", e)))?;

        let mut commits = Vec::new();
        let mut count = 0;

        for oid_result in revwalk {
            if limit > 0 && count >= limit {
                break;
            }

            let oid = oid_result
                .map_err(|e| AcpError::Other(format!("Failed to get commit oid: {}", e)))?;

            // Check if this commit touched our file
            if let Some(entry) = Self::commit_touches_file(repo, oid, &relative_path)? {
                commits.push(entry);
                count += 1;
            }
        }

        Ok(Self {
            commits,
            path: relative_path,
        })
    }

    /// Get the number of commits
    pub fn commit_count(&self) -> usize {
        self.commits.len()
    }

    /// Get unique contributors
    pub fn contributors(&self) -> Vec<String> {
        let mut authors: Vec<String> = self.commits
            .iter()
            .map(|c| c.author.clone())
            .collect();

        authors.sort();
        authors.dedup();
        authors
    }

    /// Get all history entries
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.commits
    }

    /// Get the most recent commit
    pub fn latest(&self) -> Option<&HistoryEntry> {
        self.commits.first()
    }

    /// Get the oldest commit
    pub fn oldest(&self) -> Option<&HistoryEntry> {
        self.commits.last()
    }

    /// Get the file path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get total lines added across all commits
    pub fn total_lines_added(&self) -> usize {
        self.commits.iter().map(|c| c.lines_added).sum()
    }

    /// Get total lines removed across all commits
    pub fn total_lines_removed(&self) -> usize {
        self.commits.iter().map(|c| c.lines_removed).sum()
    }

    /// Check if a commit touches the specified file and return entry if so
    fn commit_touches_file(
        repo: &GitRepository,
        oid: Oid,
        path: &str,
    ) -> Result<Option<HistoryEntry>> {
        let commit = repo.inner().find_commit(oid)
            .map_err(|e| AcpError::Other(format!("Failed to find commit: {}", e)))?;

        // Get the commit's tree
        let tree = commit.tree()
            .map_err(|e| AcpError::Other(format!("Failed to get commit tree: {}", e)))?;

        // Check if the file exists in this commit's tree
        let file_in_tree = tree.get_path(Path::new(path)).is_ok();

        // For the first commit or merge commits, check differently
        let parent_count = commit.parent_count();

        if parent_count == 0 {
            // Initial commit - if file exists, it was added here
            if file_in_tree {
                return Ok(Some(Self::create_entry(repo, &commit, path, true)?));
            }
            return Ok(None);
        }

        // Get parent tree
        let parent = commit.parent(0)
            .map_err(|e| AcpError::Other(format!("Failed to get parent commit: {}", e)))?;
        let parent_tree = parent.tree()
            .map_err(|e| AcpError::Other(format!("Failed to get parent tree: {}", e)))?;

        // Check if file changed between parent and this commit
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(path);

        let diff = repo.inner().diff_tree_to_tree(
            Some(&parent_tree),
            Some(&tree),
            Some(&mut diff_opts),
        ).map_err(|e| AcpError::Other(format!("Failed to diff trees: {}", e)))?;

        // If there are no deltas for this file, it wasn't changed
        if diff.deltas().len() == 0 {
            return Ok(None);
        }

        // File was changed in this commit
        Ok(Some(Self::create_entry(repo, &commit, path, false)?))
    }

    /// Create a history entry from a commit
    fn create_entry(
        repo: &GitRepository,
        commit: &git2::Commit,
        path: &str,
        is_initial: bool,
    ) -> Result<HistoryEntry> {
        let sig = commit.author();
        let timestamp = Self::git_time_to_datetime(sig.when());

        // Calculate lines added/removed
        let (lines_added, lines_removed) = if is_initial {
            // For initial commit, count all lines as added
            Self::count_file_lines(repo, commit, path)?
        } else {
            Self::calculate_diff_stats(repo, commit, path)?
        };

        Ok(HistoryEntry {
            commit: commit.id().to_string(),
            commit_short: commit.id().to_string().chars().take(7).collect(),
            author: sig.name().unwrap_or("Unknown").to_string(),
            author_email: sig.email().unwrap_or("").to_string(),
            timestamp,
            message: commit.summary().unwrap_or("").to_string(),
            lines_added,
            lines_removed,
        })
    }

    /// Count lines in a file for initial commit
    fn count_file_lines(
        repo: &GitRepository,
        commit: &git2::Commit,
        path: &str,
    ) -> Result<(usize, usize)> {
        let tree = commit.tree()
            .map_err(|e| AcpError::Other(format!("Failed to get tree: {}", e)))?;

        let entry = tree.get_path(Path::new(path))
            .map_err(|e| AcpError::Other(format!("Failed to get tree entry: {}", e)))?;

        let blob = repo.inner().find_blob(entry.id())
            .map_err(|e| AcpError::Other(format!("Failed to get blob: {}", e)))?;

        let content = std::str::from_utf8(blob.content()).unwrap_or("");
        let line_count = content.lines().count();

        Ok((line_count, 0))
    }

    /// Calculate diff stats for a commit
    fn calculate_diff_stats(
        repo: &GitRepository,
        commit: &git2::Commit,
        path: &str,
    ) -> Result<(usize, usize)> {
        let tree = commit.tree()
            .map_err(|e| AcpError::Other(format!("Failed to get tree: {}", e)))?;

        let parent = commit.parent(0)
            .map_err(|e| AcpError::Other(format!("Failed to get parent: {}", e)))?;
        let parent_tree = parent.tree()
            .map_err(|e| AcpError::Other(format!("Failed to get parent tree: {}", e)))?;

        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(path);

        let diff = repo.inner().diff_tree_to_tree(
            Some(&parent_tree),
            Some(&tree),
            Some(&mut diff_opts),
        ).map_err(|e| AcpError::Other(format!("Failed to create diff: {}", e)))?;

        let stats = diff.stats()
            .map_err(|e| AcpError::Other(format!("Failed to get diff stats: {}", e)))?;

        Ok((stats.insertions(), stats.deletions()))
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

    /// Helper: convert git2 time to chrono DateTime
    fn git_time_to_datetime(time: git2::Time) -> DateTime<Utc> {
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
    fn test_file_history() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let cargo_path = cwd.join("cli/Cargo.toml");
            if cargo_path.exists() {
                let history = FileHistory::for_file(&repo, &cargo_path, 10);
                assert!(history.is_ok(), "Should get file history");

                let info = history.unwrap();
                assert!(info.commit_count() > 0, "Should have at least one commit");
            }
        }
    }

    #[test]
    fn test_contributors() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let cargo_path = cwd.join("cli/Cargo.toml");
            if cargo_path.exists() {
                if let Ok(history) = FileHistory::for_file(&repo, &cargo_path, 100) {
                    let contributors = history.contributors();
                    assert!(!contributors.is_empty(), "Should have contributors");
                }
            }
        }
    }

    #[test]
    fn test_latest_commit() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let cargo_path = cwd.join("cli/Cargo.toml");
            if cargo_path.exists() {
                if let Ok(history) = FileHistory::for_file(&repo, &cargo_path, 10) {
                    let latest = history.latest();
                    assert!(latest.is_some(), "Should have a latest commit");

                    if let Some(entry) = latest {
                        assert_eq!(entry.commit.len(), 40);
                        assert!(!entry.author.is_empty());
                    }
                }
            }
        }
    }
}
