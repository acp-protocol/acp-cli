//! Aider adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{BootstrapContext, DetectionResult, ToolAdapter};
use crate::sync::content::generate_bootstrap_yaml;
use crate::sync::tool::{MergeStrategy, Tool};

/// Aider adapter - generates .aider.conf.yml
pub struct AiderAdapter;

impl ToolAdapter for AiderAdapter {
    fn tool(&self) -> Tool {
        Tool::Aider
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let config_file = project_root.join(".aider.conf.yml");

        DetectionResult {
            tool: Tool::Aider,
            detected: config_file.exists(),
            reason: if config_file.exists() {
                ".aider.conf.yml exists".into()
            } else {
                "Not detected".into()
            },
            existing_file: if config_file.exists() {
                Some(config_file)
            } else {
                None
            },
        }
    }

    fn generate(&self, _context: &BootstrapContext) -> Result<String> {
        Ok(generate_bootstrap_yaml(Tool::Aider))
    }

    fn validate(&self, content: &str) -> Result<()> {
        serde_yaml::from_str::<serde_yaml::Value>(content)
            .map_err(|e| crate::error::AcpError::Other(format!("Invalid YAML: {}", e)))?;
        Ok(())
    }

    fn merge_strategy(&self) -> MergeStrategy {
        MergeStrategy::Section
    }

    fn section_markers(&self) -> (&'static str, &'static str) {
        (
            "# BEGIN ACP GENERATED CONTENT - DO NOT EDIT",
            "# END ACP GENERATED CONTENT",
        )
    }
}
