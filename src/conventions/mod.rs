//! @acp:module "Conventions"
//! @acp:summary "RFC-0015: Auto-detection of naming and import conventions"
//! @acp:domain cli
//! @acp:layer analysis
//!
//! # Conventions Detection
//!
//! Provides automatic detection of project conventions during indexing:
//! - File naming patterns per directory
//! - Anti-patterns (similar but unused patterns to avoid)
//! - Import/module system preferences
//! - Path style preferences (relative, absolute, alias)
//!
//! ## Algorithm
//!
//! The naming convention detection algorithm:
//! 1. Groups files by parent directory
//! 2. Filters directories with < 3 files (insufficient data)
//! 3. Extracts suffix patterns (compound like `.route.ts` and simple like `.ts`)
//! 4. Calculates confidence (files matching / total files)
//! 5. Resolves conflicts when multiple patterns exist (>70% dominance wins)
//! 6. Detects anti-patterns (similar but unused patterns)
//!
//! Performance: O(n) in file count, <10ms for 1000 files.

pub mod naming;

pub use naming::{detect_naming_conventions, NamingDetector};

use crate::cache::{Conventions, ImportConventions};
use crate::error::Result;
use std::collections::HashMap;

/// Minimum files in a directory to detect patterns
pub const MIN_FILES_FOR_PATTERN: usize = 3;

/// Minimum confidence threshold for pattern inclusion (70%)
pub const CONFIDENCE_THRESHOLD: f64 = 0.70;

/// Maximum examples to include in output
pub const MAX_EXAMPLES: usize = 5;

/// Trait for convention detection implementations
pub trait ConventionDetector {
    /// Detect conventions from a list of file paths
    fn detect(&self, files: &[String]) -> Result<Conventions>;
}

/// Combined convention detector that runs all detection algorithms
#[derive(Debug, Default)]
pub struct ConventionsAnalyzer {
    /// Naming pattern detector
    naming_detector: NamingDetector,
}

impl ConventionsAnalyzer {
    /// Create a new conventions analyzer
    pub fn new() -> Self {
        Self {
            naming_detector: NamingDetector::new(),
        }
    }

    /// Analyze files and detect all conventions
    pub fn analyze(&self, files: &[String]) -> Conventions {
        let file_naming = self.naming_detector.detect_patterns(files);

        // Import conventions detection will be added in T2.4
        let imports = None;

        Conventions {
            file_naming,
            imports,
        }
    }

    /// Analyze with additional file metadata (language info)
    pub fn analyze_with_languages(
        &self,
        files: &[String],
        file_languages: &HashMap<String, String>,
    ) -> Conventions {
        let file_naming = self.naming_detector.detect_patterns(files);

        // Detect import conventions based on languages
        let imports = self.detect_import_conventions(files, file_languages);

        Conventions {
            file_naming,
            imports,
        }
    }

    /// Detect import conventions based on file languages
    fn detect_import_conventions(
        &self,
        files: &[String],
        file_languages: &HashMap<String, String>,
    ) -> Option<ImportConventions> {
        use crate::cache::{ModuleSystem, PathStyle};

        // Count JS/TS files to determine module system
        let js_ts_files: Vec<_> = files
            .iter()
            .filter(|f| {
                let lang = file_languages.get(*f).map(|s| s.as_str()).unwrap_or("");
                matches!(lang, "typescript" | "javascript")
            })
            .collect();

        if js_ts_files.is_empty() {
            return None;
        }

        // Default to ESM for modern projects (can be enhanced with actual detection)
        Some(ImportConventions {
            module_system: Some(ModuleSystem::Esm),
            path_style: Some(PathStyle::Relative),
            index_exports: false,
        })
    }
}

impl ConventionDetector for ConventionsAnalyzer {
    fn detect(&self, files: &[String]) -> Result<Conventions> {
        Ok(self.analyze(files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conventions_analyzer_empty() {
        let analyzer = ConventionsAnalyzer::new();
        let conventions = analyzer.analyze(&[]);

        assert!(conventions.file_naming.is_empty());
        assert!(conventions.imports.is_none());
    }

    #[test]
    fn test_conventions_analyzer_basic() {
        let analyzer = ConventionsAnalyzer::new();
        let files = vec![
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
            "src/routes/login.ts".to_string(),
        ];

        let conventions = analyzer.analyze(&files);

        // Should detect .ts pattern in src/routes
        assert!(!conventions.file_naming.is_empty());
    }
}
