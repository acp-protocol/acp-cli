//! Continue.dev adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{ToolAdapter, DetectionResult, BootstrapContext};
use crate::sync::content::generate_bootstrap_json;
use crate::sync::tool::{Tool, MergeStrategy};

/// Continue.dev adapter - generates .continue/config.json
pub struct ContinueAdapter;

impl ToolAdapter for ContinueAdapter {
    fn tool(&self) -> Tool {
        Tool::Continue
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let continue_dir = project_root.join(".continue");
        let config_file = continue_dir.join("config.json");

        DetectionResult {
            tool: Tool::Continue,
            detected: continue_dir.exists(),
            reason: if config_file.exists() {
                ".continue/config.json exists".into()
            } else if continue_dir.exists() {
                ".continue/ directory exists".into()
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
        Ok(generate_bootstrap_json(Tool::Continue))
    }

    fn validate(&self, content: &str) -> Result<()> {
        serde_json::from_str::<serde_json::Value>(content)
            .map_err(|e| crate::error::AcpError::Other(format!("Invalid JSON: {}", e)))?;
        Ok(())
    }

    fn merge_strategy(&self) -> MergeStrategy {
        MergeStrategy::Merge
    }

    fn section_markers(&self) -> (&'static str, &'static str) {
        // JSON doesn't use comment markers - we use the _acp key instead
        ("", "")
    }
}
