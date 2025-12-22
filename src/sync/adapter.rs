//! @acp:module "Tool Adapter Trait"
//! @acp:summary "Trait definition for tool-specific adapters"
//! @acp:domain cli
//! @acp:layer service

use std::path::{Path, PathBuf};

use crate::error::Result;
use super::tool::{Tool, MergeStrategy};

/// Result of tool detection
#[derive(Debug)]
pub struct DetectionResult {
    pub tool: Tool,
    pub detected: bool,
    pub reason: String,
    pub existing_file: Option<PathBuf>,
}

/// Context for bootstrap content generation
#[derive(Debug)]
pub struct BootstrapContext<'a> {
    pub project_root: &'a Path,
    pub tool: Tool,
}

/// Tool adapter trait - implement for each supported tool
pub trait ToolAdapter: Send + Sync {
    /// Get the tool identifier
    fn tool(&self) -> Tool;

    /// Detect if this tool is in use in the project
    fn detect(&self, project_root: &Path) -> DetectionResult;

    /// Generate bootstrap content for this tool
    fn generate(&self, context: &BootstrapContext) -> Result<String>;

    /// Validate generated content
    fn validate(&self, content: &str) -> Result<()> {
        let _ = content;
        Ok(())
    }

    /// Get the merge strategy for existing files
    fn merge_strategy(&self) -> MergeStrategy {
        MergeStrategy::Section
    }

    /// Get section markers for content preservation
    fn section_markers(&self) -> (&'static str, &'static str) {
        (
            "<!-- BEGIN ACP GENERATED CONTENT - DO NOT EDIT -->",
            "<!-- END ACP GENERATED CONTENT -->"
        )
    }
}
