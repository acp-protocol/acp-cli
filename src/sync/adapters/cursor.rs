//! Cursor IDE adapter

use std::path::Path;

use crate::error::Result;
use crate::sync::adapter::{ToolAdapter, DetectionResult, BootstrapContext};
use crate::sync::content::generate_bootstrap_markdown;
use crate::sync::tool::Tool;

/// Cursor IDE adapter - generates .cursorrules
pub struct CursorAdapter;

impl ToolAdapter for CursorAdapter {
    fn tool(&self) -> Tool {
        Tool::Cursor
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let cursorrules = project_root.join(".cursorrules");
        let cursor_dir = project_root.join(".cursor");

        DetectionResult {
            tool: Tool::Cursor,
            detected: cursorrules.exists() || cursor_dir.exists(),
            reason: if cursorrules.exists() {
                ".cursorrules exists".into()
            } else if cursor_dir.exists() {
                ".cursor/ directory exists".into()
            } else {
                "Not detected".into()
            },
            existing_file: if cursorrules.exists() {
                Some(cursorrules)
            } else {
                None
            },
        }
    }

    fn generate(&self, _context: &BootstrapContext) -> Result<String> {
        Ok(generate_bootstrap_markdown(Tool::Cursor))
    }
}
