//! @acp:module "Tool Sync"
//! @acp:summary "Sync ACP context to AI tool configuration files"
//! @acp:domain cli
//! @acp:layer service
//!
//! This module implements tool synchronization for transparent AI tool integration.
//!
//! ## Overview
//!
//! The sync system generates tool-specific configuration files that inject ACP context
//! into AI development tools automatically. Users run `acp init` once, and context flows
//! to all their tools without manual intervention.
//!
//! ## Supported Tools
//!
//! - Cursor (.cursorrules)
//! - Claude Code (CLAUDE.md)
//! - GitHub Copilot (.github/copilot-instructions.md)
//! - Continue.dev (.continue/config.json)
//! - Windsurf (.windsurfrules)
//! - Cline (.clinerules)
//! - Aider (.aider.conf.yml)
//! - Generic fallback (AGENTS.md)

pub mod tool;
pub mod adapter;
pub mod content;
pub mod merge;
pub mod adapters;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use tool::{Tool, OutputFormat, MergeStrategy};
pub use adapter::{ToolAdapter, DetectionResult, BootstrapContext};

use crate::error::Result;
use adapters::*;

/// Main sync executor - coordinates tool detection and bootstrap generation
pub struct SyncExecutor {
    adapters: HashMap<Tool, Box<dyn ToolAdapter>>,
}

impl SyncExecutor {
    /// Create a new sync executor with all built-in adapters
    pub fn new() -> Self {
        let mut adapters: HashMap<Tool, Box<dyn ToolAdapter>> = HashMap::new();

        adapters.insert(Tool::Cursor, Box::new(CursorAdapter));
        adapters.insert(Tool::ClaudeCode, Box::new(ClaudeCodeAdapter));
        adapters.insert(Tool::Copilot, Box::new(CopilotAdapter));
        adapters.insert(Tool::Continue, Box::new(ContinueAdapter));
        adapters.insert(Tool::Windsurf, Box::new(WindsurfAdapter));
        adapters.insert(Tool::Cline, Box::new(ClineAdapter));
        adapters.insert(Tool::Aider, Box::new(AiderAdapter));
        adapters.insert(Tool::Generic, Box::new(GenericAdapter));

        Self { adapters }
    }

    /// Detect which tools are in use in the project
    pub fn detect_tools(&self, project_root: &Path) -> Vec<Tool> {
        self.adapters
            .iter()
            .filter_map(|(tool, adapter)| {
                let result = adapter.detect(project_root);
                // Don't auto-include Generic - it's added separately as fallback
                if result.detected && *tool != Tool::Generic {
                    Some(*tool)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get detection results for all tools
    pub fn detect_all(&self, project_root: &Path) -> Vec<DetectionResult> {
        self.adapters
            .values()
            .map(|adapter| adapter.detect(project_root))
            .collect()
    }

    /// Bootstrap a single tool with ACP context
    pub fn bootstrap_tool(&self, tool: Tool, project_root: &Path) -> Result<BootstrapResult> {
        let adapter = self.adapters.get(&tool)
            .ok_or_else(|| crate::error::AcpError::Other(
                format!("No adapter for tool: {:?}", tool)
            ))?;

        let context = BootstrapContext {
            project_root,
            tool,
        };

        // Generate content
        let content = adapter.generate(&context)?;

        // Validate content
        adapter.validate(&content)?;

        // Determine output path
        let output_path = project_root.join(tool.output_path());

        // Create parent directories if needed
        if let Some(parent) = output_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Handle existing file
        let action = if output_path.exists() {
            let existing = std::fs::read_to_string(&output_path)?;
            let (start_marker, end_marker) = adapter.section_markers();

            let merged = if start_marker.is_empty() {
                // Special handling for JSON (Continue.dev)
                if tool == Tool::Continue {
                    merge::merge_json(&existing, &content)
                        .map_err(|e| crate::error::AcpError::Other(e.to_string()))?
                } else {
                    content.clone()
                }
            } else {
                merge::merge_content(
                    adapter.merge_strategy(),
                    &existing,
                    &content,
                    start_marker,
                    end_marker,
                )
            };

            std::fs::write(&output_path, merged)?;
            BootstrapAction::Merged
        } else {
            // New file - wrap with markers if applicable
            let (start_marker, end_marker) = adapter.section_markers();
            let final_content = if !start_marker.is_empty() {
                format!("{}\n{}\n{}", start_marker, content, end_marker)
            } else {
                content
            };

            std::fs::write(&output_path, final_content)?;
            BootstrapAction::Created
        };

        Ok(BootstrapResult {
            tool,
            output_path,
            action,
        })
    }

    /// Bootstrap all detected tools plus the generic fallback
    pub fn bootstrap_all(&self, project_root: &Path) -> Vec<Result<BootstrapResult>> {
        let mut tools = self.detect_tools(project_root);

        // Always include generic as fallback if AGENTS.md doesn't exist
        if !project_root.join("AGENTS.md").exists() {
            tools.push(Tool::Generic);
        }

        tools.into_iter()
            .map(|tool| self.bootstrap_tool(tool, project_root))
            .collect()
    }
}

impl Default for SyncExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of bootstrapping a tool
#[derive(Debug)]
pub struct BootstrapResult {
    pub tool: Tool,
    pub output_path: PathBuf,
    pub action: BootstrapAction,
}

/// Action taken during bootstrap
#[derive(Debug, PartialEq, Eq)]
pub enum BootstrapAction {
    /// File was created
    Created,
    /// Existing file was merged
    Merged,
    /// File was skipped
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sync_executor_creation() {
        let executor = SyncExecutor::new();
        assert_eq!(executor.adapters.len(), 8);
    }

    #[test]
    fn test_detect_tools_empty_project() {
        let temp = TempDir::new().unwrap();
        let executor = SyncExecutor::new();
        let detected = executor.detect_tools(temp.path());

        // Fresh project should only detect Claude if 'claude' CLI is in PATH
        // Generic is never auto-included in detect_tools
        assert!(!detected.contains(&Tool::Cursor));
        assert!(!detected.contains(&Tool::Copilot));
        assert!(!detected.contains(&Tool::Generic));
    }

    #[test]
    fn test_detect_tools_with_cursorrules() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".cursorrules"), "").unwrap();

        let executor = SyncExecutor::new();
        let detected = executor.detect_tools(temp.path());

        assert!(detected.contains(&Tool::Cursor));
    }

    #[test]
    fn test_bootstrap_creates_file() {
        let temp = TempDir::new().unwrap();
        let executor = SyncExecutor::new();

        let result = executor.bootstrap_tool(Tool::Generic, temp.path()).unwrap();

        assert_eq!(result.action, BootstrapAction::Created);
        assert!(result.output_path.exists());

        let content = std::fs::read_to_string(&result.output_path).unwrap();
        assert!(content.contains("ACP Context"));
    }

    #[test]
    fn test_bootstrap_merges_existing() {
        let temp = TempDir::new().unwrap();
        let existing_content = "# My Project\n\nSome existing content.";
        std::fs::write(temp.path().join(".cursorrules"), existing_content).unwrap();

        let executor = SyncExecutor::new();
        let result = executor.bootstrap_tool(Tool::Cursor, temp.path()).unwrap();

        assert_eq!(result.action, BootstrapAction::Merged);

        let content = std::fs::read_to_string(&result.output_path).unwrap();
        assert!(content.contains("My Project"));
        assert!(content.contains("ACP Context"));
        assert!(content.contains("BEGIN ACP GENERATED"));
    }
}
