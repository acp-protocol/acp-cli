//! @acp:module "Sync Tool Types"
//! @acp:summary "Supported AI tool definitions and metadata"
//! @acp:domain cli
//! @acp:layer model

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported AI development tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    Cursor,
    ClaudeCode,
    Copilot,
    Continue,
    Windsurf,
    Cline,
    Aider,
    Generic,
}

impl Tool {
    /// Get all built-in tools
    pub fn all() -> &'static [Tool] {
        &[
            Tool::Cursor,
            Tool::ClaudeCode,
            Tool::Copilot,
            Tool::Continue,
            Tool::Windsurf,
            Tool::Cline,
            Tool::Aider,
            Tool::Generic,
        ]
    }

    /// Get the default output path for this tool
    pub fn output_path(&self) -> &'static str {
        match self {
            Tool::Cursor => ".cursorrules",
            Tool::ClaudeCode => "CLAUDE.md",
            Tool::Copilot => ".github/copilot-instructions.md",
            Tool::Continue => ".continue/config.json",
            Tool::Windsurf => ".windsurfrules",
            Tool::Cline => ".clinerules",
            Tool::Aider => ".aider.conf.yml",
            Tool::Generic => "AGENTS.md",
        }
    }

    /// Get the human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Tool::Cursor => "Cursor",
            Tool::ClaudeCode => "Claude Code",
            Tool::Copilot => "GitHub Copilot",
            Tool::Continue => "Continue.dev",
            Tool::Windsurf => "Windsurf",
            Tool::Cline => "Cline",
            Tool::Aider => "Aider",
            Tool::Generic => "Generic (AGENTS.md)",
        }
    }

    /// Check if this tool supports MCP
    pub fn supports_mcp(&self) -> bool {
        matches!(self, Tool::ClaudeCode | Tool::Continue | Tool::Cline)
    }

    /// Get the output format for this tool
    pub fn format(&self) -> OutputFormat {
        match self {
            Tool::Continue => OutputFormat::Json,
            Tool::Aider => OutputFormat::Yaml,
            _ => OutputFormat::Markdown,
        }
    }

    /// Parse tool name from string
    pub fn from_name(name: &str) -> Option<Tool> {
        match name.to_lowercase().as_str() {
            "cursor" => Some(Tool::Cursor),
            "claude-code" | "claudecode" | "claude" => Some(Tool::ClaudeCode),
            "copilot" | "github-copilot" => Some(Tool::Copilot),
            "continue" | "continue-dev" => Some(Tool::Continue),
            "windsurf" => Some(Tool::Windsurf),
            "cline" => Some(Tool::Cline),
            "aider" => Some(Tool::Aider),
            "generic" | "agents" => Some(Tool::Generic),
            _ => None,
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Output format types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Markdown,
    Json,
    Yaml,
}

/// Merge strategy for existing files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Replace entire file
    Replace,
    /// Replace only marked section
    Section,
    /// Append to end of file
    Append,
    /// Deep merge (for JSON/YAML)
    Merge,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_output_paths() {
        assert_eq!(Tool::Cursor.output_path(), ".cursorrules");
        assert_eq!(Tool::ClaudeCode.output_path(), "CLAUDE.md");
        assert_eq!(
            Tool::Copilot.output_path(),
            ".github/copilot-instructions.md"
        );
        assert_eq!(Tool::Generic.output_path(), "AGENTS.md");
    }

    #[test]
    fn test_tool_from_name() {
        assert_eq!(Tool::from_name("cursor"), Some(Tool::Cursor));
        assert_eq!(Tool::from_name("Claude-Code"), Some(Tool::ClaudeCode));
        assert_eq!(Tool::from_name("unknown"), None);
    }

    #[test]
    fn test_tool_mcp_support() {
        assert!(Tool::ClaudeCode.supports_mcp());
        assert!(Tool::Continue.supports_mcp());
        assert!(!Tool::Cursor.supports_mcp());
    }
}
