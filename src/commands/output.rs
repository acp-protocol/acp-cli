//! @acp:module "Command Output Formatting"
//! @acp:summary "Shared output formatting utilities for CLI commands"
//! @acp:domain cli
//! @acp:layer service
//!
//! Provides tree rendering, symbol formatting, and colored output.

use console::{style, StyledObject};

use crate::cache::SymbolType;
use crate::constraints::LockLevel;

/// Tree renderer with box-drawing characters
pub struct TreeRenderer {
    pub use_unicode: bool,
}

impl Default for TreeRenderer {
    fn default() -> Self {
        Self { use_unicode: true }
    }
}

impl TreeRenderer {
    /// Branch character for intermediate items
    pub fn branch(&self) -> &'static str {
        if self.use_unicode { "├─" } else { "|-" }
    }

    /// Branch character for last item
    pub fn last_branch(&self) -> &'static str {
        if self.use_unicode { "└─" } else { "`-" }
    }

    /// Vertical continuation line
    pub fn vertical(&self) -> &'static str {
        if self.use_unicode { "│ " } else { "| " }
    }

    /// Horizontal separator line
    pub fn separator(&self, width: usize) -> String {
        if self.use_unicode {
            "━".repeat(width)
        } else {
            "=".repeat(width)
        }
    }
}

/// Format a symbol reference with type abbreviation and line number
pub fn format_symbol_ref(name: &str, symbol_type: &SymbolType, line: usize) -> String {
    let type_abbrev = match symbol_type {
        SymbolType::Function => "fn",
        SymbolType::Method => "method",
        SymbolType::Class => "class",
        SymbolType::Interface => "iface",
        SymbolType::Type => "type",
        SymbolType::Enum => "enum",
        SymbolType::Struct => "struct",
        SymbolType::Trait => "trait",
        SymbolType::Const => "const",
    };
    format!("{} ({}:{})", name, type_abbrev, line)
}

/// Format a symbol reference with line range
pub fn format_symbol_ref_range(name: &str, symbol_type: &SymbolType, lines: [usize; 2]) -> String {
    let type_abbrev = match symbol_type {
        SymbolType::Function => "fn",
        SymbolType::Method => "method",
        SymbolType::Class => "class",
        SymbolType::Interface => "iface",
        SymbolType::Type => "type",
        SymbolType::Enum => "enum",
        SymbolType::Struct => "struct",
        SymbolType::Trait => "trait",
        SymbolType::Const => "const",
    };
    format!("{} ({}:{}-{})", name, type_abbrev, lines[0], lines[1])
}

/// Format constraint level with color
pub fn format_constraint_level(level: &LockLevel) -> StyledObject<&'static str> {
    match level {
        LockLevel::Frozen => style("frozen").red().bold(),
        LockLevel::Restricted => style("restricted").yellow(),
        LockLevel::ApprovalRequired => style("approval-required").yellow(),
        LockLevel::TestsRequired => style("tests-required").cyan(),
        LockLevel::DocsRequired => style("docs-required").cyan(),
        LockLevel::ReviewRequired => style("review-required").cyan(),
        LockLevel::Normal => style("normal").green(),
        LockLevel::Experimental => style("experimental").magenta(),
    }
}

/// Format constraint level as plain string
pub fn constraint_level_str(level: &LockLevel) -> &'static str {
    match level {
        LockLevel::Frozen => "frozen",
        LockLevel::Restricted => "restricted",
        LockLevel::ApprovalRequired => "approval-required",
        LockLevel::TestsRequired => "tests-required",
        LockLevel::DocsRequired => "docs-required",
        LockLevel::ReviewRequired => "review-required",
        LockLevel::Normal => "normal",
        LockLevel::Experimental => "experimental",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_renderer() {
        let renderer = TreeRenderer::default();
        assert_eq!(renderer.branch(), "├─");
        assert_eq!(renderer.last_branch(), "└─");
        assert_eq!(renderer.vertical(), "│ ");
    }

    #[test]
    fn test_format_symbol_ref() {
        assert_eq!(
            format_symbol_ref("validate", &SymbolType::Function, 45),
            "validate (fn:45)"
        );
        assert_eq!(
            format_symbol_ref("User", &SymbolType::Class, 10),
            "User (class:10)"
        );
    }

    #[test]
    fn test_format_symbol_ref_range() {
        assert_eq!(
            format_symbol_ref_range("validate", &SymbolType::Function, [45, 89]),
            "validate (fn:45-89)"
        );
    }
}
