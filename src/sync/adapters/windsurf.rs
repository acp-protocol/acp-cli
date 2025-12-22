//! Windsurf adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{ToolAdapter, DetectionResult, BootstrapContext};
use crate::sync::content::generate_bootstrap_markdown;
use crate::sync::tool::Tool;

/// Windsurf adapter - generates .windsurfrules
pub struct WindsurfAdapter;

impl ToolAdapter for WindsurfAdapter {
    fn tool(&self) -> Tool {
        Tool::Windsurf
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let rules_file = project_root.join(".windsurfrules");
        let windsurf_dir = project_root.join(".windsurf");

        DetectionResult {
            tool: Tool::Windsurf,
            detected: rules_file.exists() || windsurf_dir.exists(),
            reason: if rules_file.exists() {
                ".windsurfrules exists".into()
            } else if windsurf_dir.exists() {
                ".windsurf/ directory exists".into()
            } else {
                "Not detected".into()
            },
            existing_file: if rules_file.exists() {
                Some(rules_file)
            } else {
                None
            },
        }
    }

    fn generate(&self, _context: &BootstrapContext) -> Result<String> {
        Ok(generate_bootstrap_markdown(Tool::Windsurf))
    }
}
