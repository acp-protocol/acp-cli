//! Cline adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{ToolAdapter, DetectionResult, BootstrapContext};
use crate::sync::content::generate_bootstrap_markdown;
use crate::sync::tool::Tool;

/// Cline adapter - generates .clinerules
pub struct ClineAdapter;

impl ToolAdapter for ClineAdapter {
    fn tool(&self) -> Tool {
        Tool::Cline
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let rules_file = project_root.join(".clinerules");

        DetectionResult {
            tool: Tool::Cline,
            detected: rules_file.exists(),
            reason: if rules_file.exists() {
                ".clinerules exists".into()
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
        // Cline supports MCP
        let mut content = generate_bootstrap_markdown(Tool::Cline);

        content.push_str("\n## Cline MCP Integration\n\n");
        content.push_str("If MCP is configured, the `acp_*` tools are available.\n");
        content.push_str("Always call `acp_check_constraints` before modifying files.\n");

        Ok(content)
    }
}
