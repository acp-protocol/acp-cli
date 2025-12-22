//! @acp:module "Git Repository"
//! @acp:summary "Repository operations via libgit2"
//! @acp:domain cli
//! @acp:layer integration

use std::path::Path;
use git2::{Repository, StatusOptions, Status};
use crate::error::{AcpError, Result};

/// File status in the git repository
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// File is tracked and unchanged
    Clean,
    /// File has been modified
    Modified,
    /// File is staged for commit
    Staged,
    /// File is untracked
    Untracked,
    /// File has conflicts
    Conflicted,
    /// File has been deleted
    Deleted,
    /// File is ignored
    Ignored,
    /// File is newly added
    New,
}

impl FileStatus {
    /// Check if the file has uncommitted changes
    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Modified | Self::Staged | Self::New | Self::Deleted | Self::Conflicted)
    }
}

/// Git repository wrapper providing common operations
pub struct GitRepository {
    repo: Repository,
}

impl GitRepository {
    /// Open a git repository from the given path (searches upward for .git)
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .map_err(|e| AcpError::Other(format!("Failed to open git repository: {}", e)))?;
        Ok(Self { repo })
    }

    /// Get the underlying git2 repository
    pub(crate) fn inner(&self) -> &Repository {
        &self.repo
    }

    /// Get the repository root path (workdir)
    pub fn root(&self) -> Result<&Path> {
        self.repo.workdir()
            .ok_or_else(|| AcpError::Other("Repository has no working directory (bare repo)".into()))
    }

    /// Get the current HEAD commit SHA (full 40-character hex)
    pub fn head_commit(&self) -> Result<String> {
        let head = self.repo.head()
            .map_err(|e| AcpError::Other(format!("Failed to get HEAD: {}", e)))?;
        let commit = head.peel_to_commit()
            .map_err(|e| AcpError::Other(format!("Failed to get HEAD commit: {}", e)))?;
        Ok(commit.id().to_string())
    }

    /// Get the short HEAD commit SHA (7 characters)
    pub fn head_commit_short(&self) -> Result<String> {
        let full = self.head_commit()?;
        Ok(full.chars().take(7).collect())
    }

    /// Get the current branch name (None if detached HEAD)
    pub fn current_branch(&self) -> Result<Option<String>> {
        let head = self.repo.head()
            .map_err(|e| AcpError::Other(format!("Failed to get HEAD: {}", e)))?;

        if head.is_branch() {
            Ok(head.shorthand().map(String::from))
        } else {
            Ok(None) // Detached HEAD
        }
    }

    /// Get the URL of a remote (e.g., "origin")
    pub fn remote_url(&self, name: &str) -> Result<Option<String>> {
        match self.repo.find_remote(name) {
            Ok(remote) => Ok(remote.url().map(String::from)),
            Err(_) => Ok(None),
        }
    }

    /// Check if a file path is tracked by git
    pub fn is_tracked(&self, path: &Path) -> bool {
        // Make path relative to repo root
        let relative_path = self.make_relative(path);

        match self.repo.status_file(relative_path.as_ref()) {
            Ok(status) => !status.contains(Status::WT_NEW) && !status.contains(Status::IGNORED),
            Err(_) => false,
        }
    }

    /// Get the status of a file
    pub fn file_status(&self, path: &Path) -> Result<FileStatus> {
        let relative_path = self.make_relative(path);

        let status = self.repo.status_file(relative_path.as_ref())
            .map_err(|e| AcpError::Other(format!("Failed to get file status: {}", e)))?;

        Ok(Self::convert_status(status))
    }

    /// Get list of modified files in the repository
    pub fn modified_files(&self) -> Result<Vec<String>> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(false)
            .include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut opts))
            .map_err(|e| AcpError::Other(format!("Failed to get repository status: {}", e)))?;

        let files: Vec<String> = statuses
            .iter()
            .filter_map(|entry| {
                let status = entry.status();
                if status.is_wt_modified() || status.is_index_modified() {
                    entry.path().map(String::from)
                } else {
                    None
                }
            })
            .collect();

        Ok(files)
    }

    /// Check if the repository has uncommitted changes
    pub fn is_dirty(&self) -> Result<bool> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(false)
            .include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut opts))
            .map_err(|e| AcpError::Other(format!("Failed to get repository status: {}", e)))?;

        Ok(statuses.iter().any(|entry| {
            let status = entry.status();
            status.is_wt_modified()
                || status.is_index_modified()
                || status.is_wt_deleted()
                || status.is_index_deleted()
                || status.is_wt_new()
                || status.is_index_new()
        }))
    }

    /// Make a path relative to the repository root
    fn make_relative<'a>(&self, path: &'a Path) -> std::borrow::Cow<'a, Path> {
        if let Ok(root) = self.root() {
            if let Ok(relative) = path.strip_prefix(root) {
                return std::borrow::Cow::Owned(relative.to_path_buf());
            }
        }
        std::borrow::Cow::Borrowed(path)
    }

    /// Convert git2 Status to our FileStatus enum
    fn convert_status(status: Status) -> FileStatus {
        if status.is_conflicted() {
            FileStatus::Conflicted
        } else if status.is_ignored() {
            FileStatus::Ignored
        } else if status.is_wt_new() {
            FileStatus::Untracked
        } else if status.is_index_new() {
            FileStatus::New
        } else if status.is_wt_deleted() || status.is_index_deleted() {
            FileStatus::Deleted
        } else if status.is_index_modified() {
            FileStatus::Staged
        } else if status.is_wt_modified() {
            FileStatus::Modified
        } else {
            FileStatus::Clean
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_open_current_repo() {
        // This test runs from within the acp-spec repo
        let cwd = env::current_dir().unwrap();
        let repo = GitRepository::open(&cwd);
        assert!(repo.is_ok(), "Should be able to open current repo");
    }

    #[test]
    fn test_head_commit() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let commit = repo.head_commit();
            assert!(commit.is_ok());
            let sha = commit.unwrap();
            assert_eq!(sha.len(), 40, "SHA should be 40 characters");
        }
    }

    #[test]
    fn test_head_commit_short() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let commit = repo.head_commit_short();
            assert!(commit.is_ok());
            let sha = commit.unwrap();
            assert_eq!(sha.len(), 7, "Short SHA should be 7 characters");
        }
    }

    #[test]
    fn test_current_branch() {
        let cwd = env::current_dir().unwrap();
        if let Ok(repo) = GitRepository::open(&cwd) {
            let branch = repo.current_branch();
            assert!(branch.is_ok());
            // Branch might be None if detached HEAD
        }
    }
}
