//! GitHub Copilot adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{ToolAdapter, DetectionResult, BootstrapContext};
use crate::sync::content::generate_bootstrap_markdown;
use crate::sync::tool::Tool;

/// GitHub Copilot adapter - generates .github/copilot-instructions.md
pub struct CopilotAdapter;

impl ToolAdapter for CopilotAdapter {
    fn tool(&self) -> Tool {
        Tool::Copilot
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let github_dir = project_root.join(".github");
        let copilot_instructions = github_dir.join("copilot-instructions.md");

        DetectionResult {
            tool: Tool::Copilot,
            detected: github_dir.exists(),
            reason: if copilot_instructions.exists() {
                "copilot-instructions.md exists".into()
            } else if github_dir.exists() {
                ".github/ directory exists".into()
            } else {
                "Not detected".into()
            },
            existing_file: if copilot_instructions.exists() {
                Some(copilot_instructions)
            } else {
                None
            },
        }
    }

    fn generate(&self, _context: &BootstrapContext) -> Result<String> {
        Ok(generate_bootstrap_markdown(Tool::Copilot))
    }
}
