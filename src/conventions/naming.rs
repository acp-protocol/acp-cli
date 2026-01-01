//! @acp:module "Naming Conventions"
//! @acp:summary "RFC-0015: File naming pattern detection per directory"
//! @acp:domain cli
//! @acp:layer analysis
//!
//! # Naming Convention Detection Algorithm
//!
//! Detects file naming patterns in each directory:
//!
//! 1. **Group** files by parent directory
//! 2. **Filter** directories with < 3 files (insufficient data)
//! 3. **Extract** suffix patterns (compound like `.route.ts` and simple like `.ts`)
//! 4. **Calculate** confidence (files matching / total files)
//! 5. **Resolve** conflicts when multiple patterns exist (>70% dominance wins)
//! 6. **Detect** anti-patterns (similar but unused patterns)

use crate::cache::FileNamingConvention;
use std::collections::HashMap;
use std::path::Path;

use super::{CONFIDENCE_THRESHOLD, MAX_EXAMPLES, MIN_FILES_FOR_PATTERN};

/// Naming pattern detector
#[derive(Debug, Default)]
pub struct NamingDetector {
    /// Known compound suffixes to detect (in priority order)
    compound_suffixes: Vec<&'static str>,
}

impl NamingDetector {
    /// Create a new naming detector with default compound suffixes
    pub fn new() -> Self {
        Self {
            // Compound suffixes in priority order (longer first)
            compound_suffixes: vec![
                ".controller.ts",
                ".controller.js",
                ".service.ts",
                ".service.js",
                ".module.ts",
                ".module.js",
                ".route.ts",
                ".route.js",
                ".routes.ts",
                ".routes.js",
                ".spec.ts",
                ".spec.js",
                ".test.ts",
                ".test.js",
                ".types.ts",
                ".types.js",
                ".model.ts",
                ".model.js",
                ".dto.ts",
                ".dto.js",
                ".entity.ts",
                ".entity.js",
                ".component.tsx",
                ".component.ts",
                ".hook.ts",
                ".hook.tsx",
                ".util.ts",
                ".utils.ts",
                ".helper.ts",
                ".config.ts",
                ".config.js",
                ".const.ts",
                ".constants.ts",
                "_test.go",
                "_test.py",
                ".test.py",
                "_spec.rb",
            ],
        }
    }

    /// Detect naming patterns from a list of file paths
    pub fn detect_patterns(&self, files: &[String]) -> Vec<FileNamingConvention> {
        // Group files by directory
        let files_by_dir = self.group_by_directory(files);

        let mut conventions = Vec::new();

        for (directory, dir_files) in files_by_dir {
            if dir_files.len() < MIN_FILES_FOR_PATTERN {
                continue;
            }

            if let Some(convention) = self.detect_directory_pattern(&directory, &dir_files) {
                conventions.push(convention);
            }
        }

        // Sort by directory for consistent output
        conventions.sort_by(|a, b| a.directory.cmp(&b.directory));

        conventions
    }

    /// Group files by their parent directory
    fn group_by_directory(&self, files: &[String]) -> HashMap<String, Vec<String>> {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();

        for file in files {
            let path = Path::new(file);
            if let Some(parent) = path.parent() {
                let dir = parent.to_string_lossy().to_string();
                // Normalize empty parent to "."
                let dir = if dir.is_empty() { ".".to_string() } else { dir };

                if let Some(filename) = path.file_name() {
                    groups
                        .entry(dir)
                        .or_default()
                        .push(filename.to_string_lossy().to_string());
                }
            }
        }

        groups
    }

    /// Detect the dominant naming pattern in a directory
    fn detect_directory_pattern(
        &self,
        directory: &str,
        filenames: &[String],
    ) -> Option<FileNamingConvention> {
        // Extract all suffix patterns from filenames
        let patterns = self.extract_patterns(filenames);

        if patterns.is_empty() {
            return None;
        }

        // Find the dominant pattern (highest count)
        // Note: Only count non-hidden files for confidence calculation
        let total_files = filenames.iter().filter(|f| !f.starts_with('.')).count();
        if total_files == 0 {
            return None;
        }
        let mut best_pattern: Option<(String, usize, Vec<String>)> = None;

        for (pattern, (count, examples)) in &patterns {
            let confidence = *count as f64 / total_files as f64;

            if confidence >= CONFIDENCE_THRESHOLD {
                match &best_pattern {
                    Some((_, best_count, _)) if *best_count >= *count => {}
                    _ => {
                        best_pattern = Some((pattern.clone(), *count, examples.clone()));
                    }
                }
            }
        }

        best_pattern.map(|(pattern, count, examples)| {
            let confidence = count as f64 / total_files as f64;

            // Detect anti-patterns (similar patterns that are NOT used)
            let anti_patterns = self.detect_anti_patterns(&pattern, &patterns);

            FileNamingConvention {
                directory: directory.to_string(),
                pattern: format!("*{}", pattern),
                confidence,
                examples: examples.into_iter().take(MAX_EXAMPLES).collect(),
                anti_patterns,
            }
        })
    }

    /// Extract suffix patterns from filenames
    fn extract_patterns(&self, filenames: &[String]) -> HashMap<String, (usize, Vec<String>)> {
        let mut patterns: HashMap<String, (usize, Vec<String>)> = HashMap::new();

        for filename in filenames {
            // Skip hidden files and special files
            if filename.starts_with('.') {
                continue;
            }

            // Try to match compound suffixes first
            let suffix = self.extract_suffix(filename);

            patterns
                .entry(suffix)
                .or_insert_with(|| (0, Vec::new()))
                .0 += 1;
            patterns.get_mut(&self.extract_suffix(filename)).unwrap().1.push(filename.clone());
        }

        patterns
    }

    /// Extract the suffix pattern from a filename
    fn extract_suffix(&self, filename: &str) -> String {
        // Check compound suffixes first (in priority order)
        for compound in &self.compound_suffixes {
            if filename.ends_with(compound) {
                return compound.to_string();
            }
        }

        // Fall back to simple extension
        if let Some(dot_pos) = filename.rfind('.') {
            return filename[dot_pos..].to_string();
        }

        // No extension
        String::new()
    }

    /// Detect anti-patterns: similar patterns that are NOT used in this directory
    fn detect_anti_patterns(
        &self,
        dominant_pattern: &str,
        detected_patterns: &HashMap<String, (usize, Vec<String>)>,
    ) -> Vec<String> {
        let mut anti_patterns = Vec::new();

        // Generate similar patterns that might be confused
        let similar = self.get_similar_patterns(dominant_pattern);

        for similar_pattern in similar {
            // Only include as anti-pattern if it's NOT already used
            if !detected_patterns.contains_key(&similar_pattern) {
                anti_patterns.push(format!("*{}", similar_pattern));
            }
        }

        anti_patterns
    }

    /// Get patterns that are similar to the given pattern (potential confusion)
    fn get_similar_patterns(&self, pattern: &str) -> Vec<String> {
        let mut similar = Vec::new();

        // Common variations
        let variations: &[(&str, &str)] = &[
            (".ts", ".tsx"),
            (".tsx", ".ts"),
            (".js", ".jsx"),
            (".jsx", ".js"),
            (".ts", ".js"),
            (".js", ".ts"),
            (".route.ts", ".routes.ts"),
            (".routes.ts", ".route.ts"),
            (".test.ts", ".spec.ts"),
            (".spec.ts", ".test.ts"),
            (".test.js", ".spec.js"),
            (".spec.js", ".test.js"),
            ("_test.go", "_test.ts"),
            (".service.ts", ".services.ts"),
            (".controller.ts", ".controllers.ts"),
        ];

        for (from, to) in variations {
            if pattern == *from {
                similar.push(to.to_string());
            }
        }

        similar
    }
}

/// Convenience function for detecting naming conventions
pub fn detect_naming_conventions(files: &[String]) -> Vec<FileNamingConvention> {
    NamingDetector::new().detect_patterns(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_simple_ts_pattern() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
            "src/routes/login.ts".to_string(),
            "src/routes/signup.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        assert_eq!(conventions.len(), 1);
        assert_eq!(conventions[0].directory, "src/routes");
        assert_eq!(conventions[0].pattern, "*.ts");
        assert!(conventions[0].confidence >= 0.70);
    }

    #[test]
    fn test_detect_compound_suffix() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/auth.route.ts".to_string(),
            "src/routes/users.route.ts".to_string(),
            "src/routes/login.route.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        assert_eq!(conventions.len(), 1);
        assert_eq!(conventions[0].pattern, "*.route.ts");
    }

    #[test]
    fn test_anti_pattern_detection() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
            "src/routes/login.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        assert_eq!(conventions.len(), 1);
        // Should suggest avoiding .routes.ts, .tsx variants
        assert!(!conventions[0].anti_patterns.is_empty());
    }

    #[test]
    fn test_insufficient_files() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        // Not enough files (< 3)
        assert!(conventions.is_empty());
    }

    #[test]
    fn test_multiple_directories() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
            "src/routes/login.ts".to_string(),
            "src/services/auth.service.ts".to_string(),
            "src/services/user.service.ts".to_string(),
            "src/services/email.service.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        assert_eq!(conventions.len(), 2);

        let routes_conv = conventions.iter().find(|c| c.directory == "src/routes");
        let services_conv = conventions.iter().find(|c| c.directory == "src/services");

        assert!(routes_conv.is_some());
        assert!(services_conv.is_some());
        assert_eq!(routes_conv.unwrap().pattern, "*.ts");
        assert_eq!(services_conv.unwrap().pattern, "*.service.ts");
    }

    #[test]
    fn test_low_confidence_excluded() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/mixed/file1.ts".to_string(),
            "src/mixed/file2.js".to_string(),
            "src/mixed/file3.tsx".to_string(),
            "src/mixed/file4.jsx".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        // No pattern has >70% confidence
        assert!(conventions.is_empty());
    }

    #[test]
    fn test_extract_suffix() {
        let detector = NamingDetector::new();

        assert_eq!(detector.extract_suffix("auth.ts"), ".ts");
        assert_eq!(detector.extract_suffix("auth.route.ts"), ".route.ts");
        assert_eq!(detector.extract_suffix("auth.service.ts"), ".service.ts");
        assert_eq!(detector.extract_suffix("auth_test.go"), "_test.go");
        assert_eq!(detector.extract_suffix("README"), "");
    }

    #[test]
    fn test_hidden_files_ignored() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/.gitkeep".to_string(),
            "src/routes/.eslintrc".to_string(),
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
            "src/routes/login.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        assert_eq!(conventions.len(), 1);
        assert_eq!(conventions[0].pattern, "*.ts");
        // Confidence should be 100% since hidden files are ignored
        assert!(conventions[0].confidence >= 0.99);
    }

    #[test]
    fn test_examples_included() {
        let detector = NamingDetector::new();
        let files = vec![
            "src/routes/auth.ts".to_string(),
            "src/routes/users.ts".to_string(),
            "src/routes/login.ts".to_string(),
        ];

        let conventions = detector.detect_patterns(&files);

        assert!(!conventions[0].examples.is_empty());
        assert!(conventions[0].examples.len() <= MAX_EXAMPLES);
    }
}
