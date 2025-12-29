//! Claude Code adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{BootstrapContext, DetectionResult, ToolAdapter};
use crate::sync::content::generate_bootstrap_markdown;
use crate::sync::tool::Tool;

/// Claude Code adapter - generates CLAUDE.md
pub struct ClaudeCodeAdapter;

impl ToolAdapter for ClaudeCodeAdapter {
    fn tool(&self) -> Tool {
        Tool::ClaudeCode
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let claude_md = project_root.join("CLAUDE.md");

        // Also check if claude CLI is in PATH
        let claude_in_path = std::process::Command::new("which")
            .arg("claude")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        DetectionResult {
            tool: Tool::ClaudeCode,
            detected: claude_md.exists() || claude_in_path,
            reason: if claude_md.exists() {
                "CLAUDE.md exists".into()
            } else if claude_in_path {
                "claude CLI in PATH".into()
            } else {
                "Not detected".into()
            },
            existing_file: if claude_md.exists() {
                Some(claude_md)
            } else {
                None
            },
        }
    }

    fn generate(&self, _context: &BootstrapContext) -> Result<String> {
        let mut content = generate_bootstrap_markdown(Tool::ClaudeCode);

        // Add Claude-specific MCP note
        content.push_str("\n## Claude Code Notes\n\n");
        content.push_str("If MCP is configured, use the `acp_*` tools directly.\n");
        content.push_str("Otherwise, read `.acp/acp.cache.json` for full codebase context.\n");

        Ok(content)
    }
}
