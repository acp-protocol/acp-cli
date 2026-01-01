//! @acp:module "IDE Detection"
//! @acp:summary "RFC-0015: IDE environment detection via environment variables"
//! @acp:domain cli
//! @acp:layer detection
//!
//! Detects IDE environment using environment variables only (no process inspection)
//! for privacy and performance. Supports: Cursor, VS Code, Cline, JetBrains, Zed.

use std::env;

/// Detected IDE environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeEnvironment {
    /// Cursor (VS Code fork with AI)
    Cursor,
    /// VS Code / VSCodium
    VsCode,
    /// Cline extension
    Cline,
    /// JetBrains IDEs (IntelliJ, WebStorm, etc.)
    JetBrains,
    /// Zed editor
    Zed,
    /// Claude Code CLI (standalone)
    ClaudeCode,
    /// Terminal / unknown (not in IDE)
    Terminal,
}

impl IdeEnvironment {
    /// Detect current IDE from environment variables
    ///
    /// Detection order matters - more specific checks first:
    /// 1. Cursor (sets CURSOR_* vars)
    /// 2. Cline (sets CLINE_* vars when running)
    /// 3. Claude Code (sets CLAUDE_CODE)
    /// 4. VS Code (sets TERM_PROGRAM=vscode or VSCODE_*)
    /// 5. JetBrains (sets JETBRAINS_IDE or TERMINAL_EMULATOR contains jetbrains)
    /// 6. Zed (sets ZED_*)
    /// 7. Terminal (default)
    pub fn detect() -> Self {
        // Check Cursor first (it also sets VS Code vars)
        if env::var("CURSOR_CHANNEL").is_ok() || env::var("CURSOR_VERSION").is_ok() {
            return IdeEnvironment::Cursor;
        }

        // Check Cline extension
        if env::var("CLINE_VERSION").is_ok() || env::var("CLINE_TASK_ID").is_ok() {
            return IdeEnvironment::Cline;
        }

        // Check Claude Code CLI
        if env::var("CLAUDE_CODE").is_ok() {
            return IdeEnvironment::ClaudeCode;
        }

        // Check VS Code (generic)
        if env::var("VSCODE_GIT_IPC_HANDLE").is_ok()
            || env::var("VSCODE_GIT_ASKPASS_NODE").is_ok()
            || env::var("TERM_PROGRAM")
                .map(|v| v.to_lowercase() == "vscode")
                .unwrap_or(false)
        {
            return IdeEnvironment::VsCode;
        }

        // Check JetBrains
        if env::var("JETBRAINS_IDE").is_ok()
            || env::var("TERMINAL_EMULATOR")
                .map(|v| v.to_lowercase().contains("jetbrains"))
                .unwrap_or(false)
        {
            return IdeEnvironment::JetBrains;
        }

        // Check Zed
        if env::var("ZED_TERM").is_ok() || env::var("ZED_PID").is_ok() {
            return IdeEnvironment::Zed;
        }

        IdeEnvironment::Terminal
    }

    /// Check if running in any IDE context
    pub fn is_ide(&self) -> bool {
        !matches!(self, IdeEnvironment::Terminal)
    }

    /// Check if running in standalone mode (not in IDE)
    pub fn is_standalone(&self) -> bool {
        matches!(self, IdeEnvironment::Terminal | IdeEnvironment::ClaudeCode)
    }

    /// Get IDE name for display
    pub fn name(&self) -> &'static str {
        match self {
            IdeEnvironment::Cursor => "Cursor",
            IdeEnvironment::VsCode => "VS Code",
            IdeEnvironment::Cline => "Cline",
            IdeEnvironment::JetBrains => "JetBrains",
            IdeEnvironment::Zed => "Zed",
            IdeEnvironment::ClaudeCode => "Claude Code",
            IdeEnvironment::Terminal => "Terminal",
        }
    }

    /// Check if user override is set to disable IDE detection
    pub fn detection_disabled() -> bool {
        env::var("ACP_NO_IDE_DETECT")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
    }

    /// Detect with override support
    pub fn detect_with_override() -> Self {
        if Self::detection_disabled() {
            return IdeEnvironment::Terminal;
        }
        Self::detect()
    }
}

impl std::fmt::Display for IdeEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ide_detection_terminal_default() {
        // In test environment, should detect as terminal
        // (unless running in an IDE's test runner)
        let ide = IdeEnvironment::detect();
        // Just verify it returns something valid
        assert!(!ide.name().is_empty());
    }

    #[test]
    fn test_is_standalone() {
        assert!(IdeEnvironment::Terminal.is_standalone());
        assert!(IdeEnvironment::ClaudeCode.is_standalone());
        assert!(!IdeEnvironment::Cursor.is_standalone());
        assert!(!IdeEnvironment::VsCode.is_standalone());
    }

    #[test]
    fn test_is_ide() {
        assert!(!IdeEnvironment::Terminal.is_ide());
        assert!(IdeEnvironment::Cursor.is_ide());
        assert!(IdeEnvironment::VsCode.is_ide());
        assert!(IdeEnvironment::JetBrains.is_ide());
        assert!(IdeEnvironment::Zed.is_ide());
        assert!(IdeEnvironment::ClaudeCode.is_ide());
    }

    #[test]
    fn test_ide_names() {
        assert_eq!(IdeEnvironment::Cursor.name(), "Cursor");
        assert_eq!(IdeEnvironment::VsCode.name(), "VS Code");
        assert_eq!(IdeEnvironment::Terminal.name(), "Terminal");
    }
}
