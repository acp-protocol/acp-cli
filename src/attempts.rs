//! @acp:module "Attempt Tracking"
//! @acp:summary "Attempt tracking and rollback system for AI troubleshooting"
//! @acp:domain cli
//! @acp:layer service
//!
//! Manages troubleshooting attempts, checkpoints, and rollbacks.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::constraints::AttemptStatus;

fn default_attempts_schema() -> String {
    "https://acp-protocol.dev/schemas/v1/attempts.schema.json".to_string()
}

/// Tracks all attempts across the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptTracker {
    /// JSON Schema URL for validation
    #[serde(rename = "$schema", default = "default_attempts_schema")]
    pub schema: String,

    /// Version
    pub version: String,

    /// When last updated
    pub updated_at: DateTime<Utc>,

    /// Active attempts by ID
    pub attempts: HashMap<String, TrackedAttempt>,

    /// Checkpoints by name
    pub checkpoints: HashMap<String, TrackedCheckpoint>,

    /// Attempt history (completed/reverted)
    pub history: Vec<AttemptHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedAttempt {
    pub id: String,
    pub for_issue: Option<String>,
    pub description: Option<String>,
    pub status: AttemptStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    
    /// Files modified in this attempt
    pub files: Vec<AttemptFile>,
    
    /// Conditions that should trigger revert
    pub revert_if: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptFile {
    pub path: String,
    pub original_hash: String,
    pub original_content: Option<String>,
    pub modified_hash: String,
    pub lines_changed: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedCheckpoint {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,

    /// Git commit SHA at checkpoint time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,

    /// File states at checkpoint
    pub files: HashMap<String, FileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub hash: String,
    pub content: Option<String>, // Only stored if small enough
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptHistoryEntry {
    pub id: String,
    pub status: AttemptStatus,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub for_issue: Option<String>,
    pub files_modified: usize,
    pub outcome: Option<String>,
}

impl AttemptTracker {
    const FILE_NAME: &'static str = ".acp/acp.attempts.json";
    const MAX_STORED_CONTENT_SIZE: usize = 100_000; // 100KB

    /// Load or create tracker
    pub fn load_or_create() -> Self {
        Self::load().unwrap_or_else(|_| Self {
            schema: default_attempts_schema(),
            version: crate::VERSION.to_string(),
            updated_at: Utc::now(),
            attempts: HashMap::new(),
            checkpoints: HashMap::new(),
            history: Vec::new(),
        })
    }

    /// Load from file
    pub fn load() -> Result<Self> {
        let content = fs::read_to_string(Self::FILE_NAME)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save to file
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(Self::FILE_NAME, content)?;
        Ok(())
    }

    /// Start a new attempt
    pub fn start_attempt(
        &mut self,
        id: &str,
        for_issue: Option<&str>,
        description: Option<&str>,
    ) -> &mut TrackedAttempt {
        let attempt = TrackedAttempt {
            id: id.to_string(),
            for_issue: for_issue.map(String::from),
            description: description.map(String::from),
            status: AttemptStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            files: Vec::new(),
            revert_if: Vec::new(),
        };

        self.attempts.insert(id.to_string(), attempt);
        self.updated_at = Utc::now();
        self.attempts.get_mut(id).unwrap()
    }

    /// Record a file modification in an attempt
    pub fn record_modification(
        &mut self,
        attempt_id: &str,
        file_path: &str,
        original_content: &str,
        new_content: &str,
    ) -> Result<()> {
        let attempt = self.attempts.get_mut(attempt_id)
            .ok_or_else(|| crate::error::AcpError::Other(
                format!("Attempt not found: {}", attempt_id)
            ))?;

        let original_hash = format!("{:x}", md5::compute(original_content));
        let modified_hash = format!("{:x}", md5::compute(new_content));

        // Store original content if small enough
        let stored_content = if original_content.len() <= Self::MAX_STORED_CONTENT_SIZE {
            Some(original_content.to_string())
        } else {
            None
        };

        attempt.files.push(AttemptFile {
            path: file_path.to_string(),
            original_hash,
            original_content: stored_content,
            modified_hash,
            lines_changed: None,
        });

        attempt.updated_at = Utc::now();
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark attempt as failed
    pub fn fail_attempt(&mut self, id: &str, reason: Option<&str>) -> Result<()> {
        if let Some(attempt) = self.attempts.get_mut(id) {
            attempt.status = AttemptStatus::Failed;
            attempt.updated_at = Utc::now();
            
            // Move to history
            self.history.push(AttemptHistoryEntry {
                id: attempt.id.clone(),
                status: AttemptStatus::Failed,
                started_at: attempt.created_at,
                ended_at: Utc::now(),
                for_issue: attempt.for_issue.clone(),
                files_modified: attempt.files.len(),
                outcome: reason.map(String::from),
            });
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark attempt as verified/successful
    pub fn verify_attempt(&mut self, id: &str) -> Result<()> {
        if let Some(attempt) = self.attempts.get_mut(id) {
            attempt.status = AttemptStatus::Verified;
            attempt.updated_at = Utc::now();
            
            // Move to history
            self.history.push(AttemptHistoryEntry {
                id: attempt.id.clone(),
                status: AttemptStatus::Verified,
                started_at: attempt.created_at,
                ended_at: Utc::now(),
                for_issue: attempt.for_issue.clone(),
                files_modified: attempt.files.len(),
                outcome: Some("Verified and kept".to_string()),
            });
            
            // Remove from active attempts
            self.attempts.remove(id);
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Revert an attempt
    pub fn revert_attempt(&mut self, id: &str) -> Result<Vec<RevertAction>> {
        let attempt = self.attempts.get(id)
            .ok_or_else(|| crate::error::AcpError::Other(
                format!("Attempt not found: {}", id)
            ))?
            .clone();

        let mut actions = Vec::new();

        for file in &attempt.files {
            if let Some(original) = &file.original_content {
                // Restore original content
                fs::write(&file.path, original)?;
                actions.push(RevertAction {
                    file: file.path.clone(),
                    action: "restored".to_string(),
                    from_hash: file.modified_hash.clone(),
                    to_hash: file.original_hash.clone(),
                });
            } else {
                // Content not stored, just mark as needing manual revert
                actions.push(RevertAction {
                    file: file.path.clone(),
                    action: "manual-revert-needed".to_string(),
                    from_hash: file.modified_hash.clone(),
                    to_hash: file.original_hash.clone(),
                });
            }
        }

        // Move to history
        self.history.push(AttemptHistoryEntry {
            id: attempt.id.clone(),
            status: AttemptStatus::Reverted,
            started_at: attempt.created_at,
            ended_at: Utc::now(),
            for_issue: attempt.for_issue.clone(),
            files_modified: attempt.files.len(),
            outcome: Some("Reverted".to_string()),
        });

        // Remove from active
        self.attempts.remove(id);
        self.updated_at = Utc::now();
        self.save()?;

        Ok(actions)
    }

    /// Create a checkpoint
    pub fn create_checkpoint(
        &mut self,
        name: &str,
        files: &[&str],
        description: Option<&str>,
    ) -> Result<()> {
        let mut file_states = HashMap::new();
        let mut file_data: Vec<(String, String, String)> = Vec::new(); // (path, content, hash)

        for file_path in files {
            if Path::new(file_path).exists() {
                let content = fs::read_to_string(file_path)?;
                let hash = format!("{:x}", md5::compute(&content));

                let stored_content = if content.len() <= Self::MAX_STORED_CONTENT_SIZE {
                    Some(content.clone())
                } else {
                    None
                };

                file_data.push((file_path.to_string(), content, hash.clone()));
                file_states.insert(file_path.to_string(), FileState {
                    hash,
                    content: stored_content,
                });
            }
        }

        // Capture git commit SHA if in a git repository
        let git_commit = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| s.len() == 40);

        self.checkpoints.insert(name.to_string(), TrackedCheckpoint {
            name: name.to_string(),
            created_at: Utc::now(),
            description: description.map(String::from),
            git_commit,
            files: file_states,
        });

        // Also track these files in the most recent active attempt
        if let Some(attempt) = self.attempts.values_mut()
            .filter(|a| a.status == AttemptStatus::Active)
            .max_by_key(|a| a.created_at)
        {
            for (path, content, hash) in file_data {
                // Check if file already tracked in this attempt
                if !attempt.files.iter().any(|f| f.path == path) {
                    let stored_content = if content.len() <= Self::MAX_STORED_CONTENT_SIZE {
                        Some(content)
                    } else {
                        None
                    };
                    attempt.files.push(AttemptFile {
                        path,
                        original_hash: hash.clone(),
                        original_content: stored_content,
                        modified_hash: hash, // Same as original until modified
                        lines_changed: None,
                    });
                    attempt.updated_at = Utc::now();
                }
            }
        }

        self.updated_at = Utc::now();
        self.save()?;
        Ok(())
    }

    /// Restore to a checkpoint
    pub fn restore_checkpoint(&mut self, name: &str) -> Result<Vec<RevertAction>> {
        let checkpoint = self.checkpoints.get(name)
            .ok_or_else(|| crate::error::AcpError::Other(
                format!("Checkpoint not found: {}", name)
            ))?
            .clone();

        let mut actions = Vec::new();

        for (path, state) in &checkpoint.files {
            if let Some(content) = &state.content {
                fs::write(path, content)?;
                actions.push(RevertAction {
                    file: path.clone(),
                    action: "restored".to_string(),
                    from_hash: "current".to_string(),
                    to_hash: state.hash.clone(),
                });
            } else {
                actions.push(RevertAction {
                    file: path.clone(),
                    action: "manual-restore-needed".to_string(),
                    from_hash: "current".to_string(),
                    to_hash: state.hash.clone(),
                });
            }
        }

        self.updated_at = Utc::now();
        Ok(actions)
    }

    /// Get all active attempts
    pub fn active_attempts(&self) -> Vec<&TrackedAttempt> {
        self.attempts.values()
            .filter(|a| a.status == AttemptStatus::Active || a.status == AttemptStatus::Testing)
            .collect()
    }

    /// Get failed attempts
    pub fn failed_attempts(&self) -> Vec<&TrackedAttempt> {
        self.attempts.values()
            .filter(|a| a.status == AttemptStatus::Failed)
            .collect()
    }

    /// Clean up failed attempts (revert all)
    pub fn cleanup_failed(&mut self) -> Result<Vec<RevertAction>> {
        let failed_ids: Vec<_> = self.failed_attempts()
            .iter()
            .map(|a| a.id.clone())
            .collect();

        let mut all_actions = Vec::new();
        for id in failed_ids {
            let actions = self.revert_attempt(&id)?;
            all_actions.extend(actions);
        }

        Ok(all_actions)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertAction {
    pub file: String,
    pub action: String,
    pub from_hash: String,
    pub to_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_attempt() {
        let mut tracker = AttemptTracker::load_or_create();
        tracker.start_attempt("test-001", Some("bug#123"), Some("Testing fix"));
        
        assert!(tracker.attempts.contains_key("test-001"));
        assert_eq!(tracker.attempts["test-001"].for_issue, Some("bug#123".to_string()));
    }
}
