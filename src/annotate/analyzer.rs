//! @acp:module "Annotation Analyzer"
//! @acp:summary "Analyzes code to identify annotation gaps and existing coverage"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Annotation Analyzer
//!
//! Provides functionality for analyzing source files to:
//! - Detect existing ACP annotations
//! - Identify symbols lacking annotations
//! - Calculate annotation coverage metrics
//! - Extract doc comments for potential conversion

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

use crate::ast::{AstParser, ExtractedSymbol, SymbolKind, Visibility};
use crate::config::Config;
use crate::error::Result;

use super::{AnalysisResult, AnnotateLevel, AnnotationGap, AnnotationType, ExistingAnnotation};

/// @acp:summary "Analyzes source files for ACP annotation coverage"
/// @acp:lock normal
pub struct Analyzer {
    /// Configuration for analysis
    config: Config,

    /// AST parser for symbol extraction
    ast_parser: AstParser,

    /// Regex for detecting @acp: annotations
    annotation_pattern: Regex,

    /// Annotation level for gap detection
    level: AnnotateLevel,
}

impl Analyzer {
    /// @acp:summary "Creates a new analyzer with the given configuration"
    pub fn new(config: &Config) -> Result<Self> {
        let annotation_pattern =
            Regex::new(r"@acp:([a-z][a-z0-9-]*)(?:\s+(.+))?$").expect("Invalid annotation regex");

        Ok(Self {
            config: config.clone(),
            ast_parser: AstParser::new()?,
            annotation_pattern,
            level: AnnotateLevel::Standard,
        })
    }

    /// @acp:summary "Sets the annotation level for gap detection"
    pub fn with_level(mut self, level: AnnotateLevel) -> Self {
        self.level = level;
        self
    }

    /// @acp:summary "Discovers files to analyze based on configuration"
    ///
    /// Walks the directory tree and filters files based on include/exclude
    /// patterns from the configuration.
    pub fn discover_files(&self, root: &Path, filter: Option<&str>) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check if file matches include patterns
            let path_str = path.to_string_lossy();
            let matches_include = self.config.include.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(&path_str))
                    .unwrap_or(false)
            });

            if !matches_include {
                continue;
            }

            // Check if file matches exclude patterns
            let matches_exclude = self.config.exclude.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(&path_str))
                    .unwrap_or(false)
            });

            if matches_exclude {
                continue;
            }

            // Apply optional filter
            if let Some(filter_pattern) = filter {
                if let Ok(pattern) = glob::Pattern::new(filter_pattern) {
                    if !pattern.matches(&path_str) {
                        continue;
                    }
                }
            }

            files.push(path.to_path_buf());
        }

        Ok(files)
    }

    /// @acp:summary "Analyzes a single file for annotation coverage"
    ///
    /// Parses the file, extracts symbols and existing annotations,
    /// and identifies gaps where annotations are missing.
    pub fn analyze_file(&self, file_path: &Path) -> Result<AnalysisResult> {
        let content = std::fs::read_to_string(file_path)?;
        let path_str = file_path.to_string_lossy().to_string();

        // Detect language from extension
        let language = self.detect_language(file_path);

        let mut result = AnalysisResult::new(&path_str, &language);

        // Extract existing annotations from comments
        result.existing_annotations = self.extract_existing_annotations(&content, &path_str);

        // Parse AST and extract symbols
        if let Ok(symbols) = self.ast_parser.parse_file(file_path, &content) {
            // Associate annotations with their correct symbol targets
            self.associate_annotations_with_symbols(&mut result.existing_annotations, &symbols);

            // Build map of annotated targets -> annotation types they have
            let annotated_types: HashMap<String, HashSet<AnnotationType>> = {
                let mut map: HashMap<String, HashSet<AnnotationType>> = HashMap::new();
                for ann in &result.existing_annotations {
                    map.entry(ann.target.clone())
                        .or_default()
                        .insert(ann.annotation_type);
                }
                map
            };

            // Find gaps (symbols with missing annotation types)
            for symbol in &symbols {
                if self.should_annotate_symbol(symbol) {
                    let target = symbol.qualified_name.as_ref().unwrap_or(&symbol.name);

                    // Get existing annotation types for this target
                    let existing_types = annotated_types.get(target).cloned().unwrap_or_default();

                    // Determine which annotation types are missing
                    let missing = self.get_missing_annotation_types(symbol, &existing_types);

                    if !missing.is_empty() {
                        let mut gap = AnnotationGap::new(target, symbol.start_line)
                            .with_symbol_kind(symbol.kind)
                            .with_visibility(symbol.visibility);

                        if symbol.exported {
                            gap = gap.exported();
                        }

                        // Set doc comment with calculated line range
                        if let Some(doc) = &symbol.doc_comment {
                            // Try to find actual doc comment boundaries in source
                            if let Some((start, end)) =
                                self.find_doc_comment_range(&content, symbol.start_line)
                            {
                                gap = gap.with_doc_comment_range(doc, start, end);
                            } else {
                                // Fallback to calculated range
                                let doc_line_count = doc.lines().count();
                                if doc_line_count > 0 && symbol.start_line > doc_line_count {
                                    let doc_end = symbol.start_line - 1;
                                    let doc_start = doc_end.saturating_sub(doc_line_count - 1);
                                    gap = gap.with_doc_comment_range(doc, doc_start, doc_end);
                                } else {
                                    gap = gap.with_doc_comment(doc);
                                }
                            }
                        }

                        gap.missing = missing;
                        result.gaps.push(gap);
                    }
                }
            }

            // Check for file-level annotation gap
            let file_existing_types = annotated_types.get(&path_str).cloned().unwrap_or_default();
            let mut file_missing = Vec::new();

            if !file_existing_types.contains(&AnnotationType::Module) {
                file_missing.push(AnnotationType::Module);
            }
            if self.level.includes(AnnotationType::Summary)
                && !file_existing_types.contains(&AnnotationType::Summary)
            {
                file_missing.push(AnnotationType::Summary);
            }
            if self.level.includes(AnnotationType::Domain)
                && !file_existing_types.contains(&AnnotationType::Domain)
            {
                file_missing.push(AnnotationType::Domain);
            }

            if !file_missing.is_empty() {
                let mut file_gap = AnnotationGap::new(&path_str, 1);
                file_gap.missing = file_missing;
                result.gaps.push(file_gap);
            }
        }

        // Calculate coverage
        result.calculate_coverage();

        Ok(result)
    }

    /// @acp:summary "Detects the programming language from file extension"
    fn detect_language(&self, path: &Path) -> String {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext {
                "ts" | "tsx" => "typescript",
                "js" | "jsx" | "mjs" | "cjs" => "javascript",
                "py" | "pyi" => "python",
                "rs" => "rust",
                "go" => "go",
                "java" => "java",
                _ => "unknown",
            })
            .unwrap_or("unknown")
            .to_string()
    }

    /// @acp:summary "Extracts existing @acp: annotations from file content"
    fn extract_existing_annotations(
        &self,
        content: &str,
        file_path: &str,
    ) -> Vec<ExistingAnnotation> {
        let mut annotations = Vec::new();
        let current_target = file_path.to_string();

        for (line_num, line) in content.lines().enumerate() {
            let line_number = line_num + 1; // Convert to 1-indexed

            // Check for @acp: annotation
            if let Some(caps) = self.annotation_pattern.captures(line) {
                let namespace = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let value = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");

                if let Some(annotation_type) = self.parse_annotation_type(namespace) {
                    annotations.push(ExistingAnnotation {
                        target: current_target.clone(),
                        annotation_type,
                        value: value.trim_matches('"').to_string(),
                        line: line_number,
                    });
                }
            }
        }

        annotations
    }

    /// @acp:summary "Associates annotations with their correct symbol targets"
    ///
    /// For each annotation, finds the symbol that immediately follows it
    /// (within a reasonable line distance) and updates the annotation's target.
    fn associate_annotations_with_symbols(
        &self,
        annotations: &mut [ExistingAnnotation],
        symbols: &[ExtractedSymbol],
    ) {
        // Sort symbols by start line for efficient lookup
        let mut sorted_symbols: Vec<&ExtractedSymbol> = symbols.iter().collect();
        sorted_symbols.sort_by_key(|s| s.start_line);

        for annotation in annotations.iter_mut() {
            // Find the symbol that starts closest after this annotation
            // (annotations appear in doc comments just before the symbol)
            let annotation_line = annotation.line;

            // Look for a symbol that starts within 20 lines after the annotation
            // (doc comments can be multi-line)
            let max_distance = 20;

            if let Some(symbol) = sorted_symbols.iter().find(|s| {
                s.start_line > annotation_line && s.start_line <= annotation_line + max_distance
            }) {
                // Update the target to the symbol's qualified name
                annotation.target = symbol
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| symbol.name.clone());
            }
            // If no symbol found, the annotation stays associated with the file path
            // (module-level annotation)
        }
    }

    /// @acp:summary "Parses an annotation namespace into an AnnotationType"
    fn parse_annotation_type(&self, namespace: &str) -> Option<AnnotationType> {
        match namespace {
            "module" => Some(AnnotationType::Module),
            "summary" => Some(AnnotationType::Summary),
            "domain" => Some(AnnotationType::Domain),
            "layer" => Some(AnnotationType::Layer),
            "lock" => Some(AnnotationType::Lock),
            "stability" => Some(AnnotationType::Stability),
            "deprecated" => Some(AnnotationType::Deprecated),
            "ai-hint" => Some(AnnotationType::AiHint),
            "ref" => Some(AnnotationType::Ref),
            "hack" => Some(AnnotationType::Hack),
            "lock-reason" => Some(AnnotationType::LockReason),
            _ => None,
        }
    }

    /// @acp:summary "Determines if a symbol should be annotated"
    fn should_annotate_symbol(&self, symbol: &ExtractedSymbol) -> bool {
        // Skip private symbols unless they're important
        match symbol.visibility {
            Visibility::Private => false,
            Visibility::Protected | Visibility::Internal | Visibility::Crate => {
                // Include protected/internal if they're "important" kinds
                matches!(
                    symbol.kind,
                    SymbolKind::Class
                        | SymbolKind::Struct
                        | SymbolKind::Interface
                        | SymbolKind::Trait
                )
            }
            Visibility::Public => true,
        }
    }

    /// @acp:summary "Determines which annotation types are missing for a symbol"
    fn get_missing_annotation_types(
        &self,
        symbol: &ExtractedSymbol,
        existing_types: &HashSet<AnnotationType>,
    ) -> Vec<AnnotationType> {
        let mut missing = Vec::new();

        // Check each annotation type at current level
        for annotation_type in self.level.included_types() {
            // Skip file-level only annotations for symbols
            if matches!(annotation_type, AnnotationType::Module) {
                continue;
            }

            // Check if this specific annotation type already exists
            if !existing_types.contains(&annotation_type) {
                missing.push(annotation_type);
            }
        }

        // @acp:summary is always recommended for exported symbols
        if symbol.exported
            && !existing_types.contains(&AnnotationType::Summary)
            && !missing.contains(&AnnotationType::Summary)
        {
            missing.insert(0, AnnotationType::Summary);
        }

        missing
    }

    /// @acp:summary "Finds the actual doc comment range by parsing source"
    ///
    /// Searches backward from the symbol line to find the JSDoc/doc comment
    /// block boundaries (/** ... */). Returns (start_line, end_line) 1-indexed.
    fn find_doc_comment_range(&self, content: &str, symbol_line: usize) -> Option<(usize, usize)> {
        let lines: Vec<&str> = content.lines().collect();

        // symbol_line is 1-indexed, convert to 0-indexed for array access
        if symbol_line == 0 || symbol_line > lines.len() {
            return None;
        }

        let mut end_line = None;
        let mut start_line = None;

        // Search backward from symbol (excluding the symbol line itself)
        for i in (0..symbol_line.saturating_sub(1)).rev() {
            let line = lines.get(i).map(|s| s.trim()).unwrap_or("");

            // Found end of doc comment
            if line.ends_with("*/") && end_line.is_none() {
                end_line = Some(i + 1); // Convert back to 1-indexed
            }

            // Found start of doc comment
            if line.starts_with("/**") || line == "/**" {
                start_line = Some(i + 1); // Convert back to 1-indexed
                break;
            }

            // If we haven't found end_line yet and hit non-comment/non-whitespace, stop
            if end_line.is_none() {
                // Allow: empty lines, decorator lines (@...), single-line comments
                if !line.is_empty()
                    && !line.starts_with("//")
                    && !line.starts_with("@")
                    && !line.starts_with("*")
                {
                    break;
                }
            }
        }

        match (start_line, end_line) {
            (Some(s), Some(e)) if s <= e => Some((s, e)),
            _ => None,
        }
    }

    /// @acp:summary "Checks if a cache exists and has been initialized"
    pub fn has_existing_cache(&self, root: &Path) -> bool {
        let cache_path = root.join(".acp").join("acp.cache.json");
        cache_path.exists()
    }

    /// @acp:summary "Calculates total coverage across multiple analysis results"
    pub fn calculate_total_coverage(results: &[AnalysisResult]) -> f32 {
        if results.is_empty() {
            return 100.0;
        }

        let total_annotated: usize = results.iter().map(|r| r.existing_annotations.len()).sum();
        let total_gaps: usize = results.iter().map(|r| r.gaps.len()).sum();
        let total = total_annotated + total_gaps;

        if total == 0 {
            100.0
        } else {
            (total_annotated as f32 / total as f32) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        let config = Config::default();
        let analyzer = Analyzer::new(&config).unwrap();

        assert_eq!(analyzer.detect_language(Path::new("test.ts")), "typescript");
        assert_eq!(analyzer.detect_language(Path::new("test.py")), "python");
        assert_eq!(analyzer.detect_language(Path::new("test.rs")), "rust");
        assert_eq!(analyzer.detect_language(Path::new("test.txt")), "unknown");
    }

    #[test]
    fn test_parse_annotation_type() {
        let config = Config::default();
        let analyzer = Analyzer::new(&config).unwrap();

        assert_eq!(
            analyzer.parse_annotation_type("summary"),
            Some(AnnotationType::Summary)
        );
        assert_eq!(
            analyzer.parse_annotation_type("domain"),
            Some(AnnotationType::Domain)
        );
        assert_eq!(analyzer.parse_annotation_type("unknown"), None);
    }

    #[test]
    fn test_calculate_total_coverage() {
        let mut result1 = AnalysisResult::new("file1.ts", "typescript");
        result1.existing_annotations.push(ExistingAnnotation {
            target: "file1.ts".to_string(),
            annotation_type: AnnotationType::Module,
            value: "Test".to_string(),
            line: 1,
        });

        let mut result2 = AnalysisResult::new("file2.ts", "typescript");
        result2.gaps.push(AnnotationGap::new("MyClass", 10));

        let coverage = Analyzer::calculate_total_coverage(&[result1, result2]);
        assert!((coverage - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_doc_comment_range() {
        // Test the with_doc_comment_range builder method
        let gap = AnnotationGap::new("MyClass", 10).with_doc_comment_range(
            "/// This is a doc comment\n/// Second line",
            8,
            9,
        );

        assert!(gap.doc_comment.is_some());
        assert_eq!(gap.doc_comment_range, Some((8, 9)));
        assert!(gap.doc_comment.unwrap().contains("This is a doc comment"));
    }

    #[test]
    fn test_associate_annotations_with_symbols() {
        use crate::ast::SymbolKind;

        let config = Config::default();
        let analyzer = Analyzer::new(&config).unwrap();

        // Create mock annotations at lines that precede symbols
        let mut annotations = vec![
            ExistingAnnotation {
                target: "file.rs".to_string(), // Initially assigned to file
                annotation_type: AnnotationType::Summary,
                value: "MyStruct summary".to_string(),
                line: 28, // Annotation on line 28 (near symbol at 30)
            },
            ExistingAnnotation {
                target: "file.rs".to_string(),
                annotation_type: AnnotationType::Domain,
                value: "core".to_string(),
                line: 29, // Another annotation on line 29
            },
            ExistingAnnotation {
                target: "file.rs".to_string(),
                annotation_type: AnnotationType::Module,
                value: "FileModule".to_string(),
                line: 1, // Module annotation at top (>20 lines from any symbol)
            },
        ];

        // Create mock symbols
        let symbols = vec![ExtractedSymbol {
            name: "MyStruct".to_string(),
            qualified_name: Some("module::MyStruct".to_string()),
            kind: SymbolKind::Struct,
            visibility: Visibility::Public,
            start_line: 30, // Symbol starts at line 30 (within 20 lines of annotations at 28-29)
            end_line: 50,
            start_col: 0,
            end_col: 0,
            signature: None,
            doc_comment: None,
            parent: None,
            type_info: None,
            parameters: vec![],
            return_type: None,
            exported: true,
            is_async: false,
            is_static: false,
            generics: vec![],
        }];

        analyzer.associate_annotations_with_symbols(&mut annotations, &symbols);

        // Check that annotations on lines 28 and 29 were associated with MyStruct
        assert_eq!(annotations[0].target, "module::MyStruct");
        assert_eq!(annotations[1].target, "module::MyStruct");

        // Module annotation at line 1 should stay as file target (symbol at 30 is >20 lines away)
        assert_eq!(annotations[2].target, "file.rs");
    }
}
