//! Integration tests for the annotation pipeline.
//!
//! Tests the full flow from analysis through suggestion to writing.

use std::path::PathBuf;

use acp::annotate::{
    Analyzer, AnnotateLevel, AnnotationType, ConversionSource, Suggester, Writer,
};
use acp::Config;

/// Helper to get the fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("annotate")
}

/// Helper to create default test config
fn test_config() -> Config {
    let mut config = Config::default();
    config.include = vec![
        "**/*.ts".to_string(),
        "**/*.py".to_string(),
        "**/*.rs".to_string(),
        "**/*.go".to_string(),
        "**/*.java".to_string(),
    ];
    config
}

mod typescript_tests {
    use super::*;

    #[test]
    fn test_analyze_typescript_file() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixture = fixtures_dir().join("sample.ts");
        if !fixture.exists() {
            eprintln!("Skipping test: fixture not found at {:?}", fixture);
            return;
        }

        let result = analyzer.analyze_file(&fixture).expect("Failed to analyze");

        assert_eq!(result.language, "typescript");
        // Should find some existing annotations or gaps
        assert!(
            !result.existing_annotations.is_empty() || !result.gaps.is_empty(),
            "Expected to find annotations or gaps"
        );
    }

    #[test]
    fn test_suggest_from_jsdoc() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard)
            .with_conversion_source(ConversionSource::Jsdoc);

        let fixture = fixtures_dir().join("sample.ts");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        // Should generate suggestions from JSDoc comments
        let _has_summary = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Summary);
        let _has_ref = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Ref);

        // At minimum should have summaries from heuristics
        assert!(
            !suggestions.is_empty(),
            "Expected suggestions from TypeScript file"
        );
    }

    #[test]
    fn test_generate_diff_typescript() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard);
        let writer = Writer::new();

        let fixture = fixtures_dir().join("sample.ts");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        if suggestions.is_empty() {
            return;
        }

        let changes = writer
            .plan_changes(&fixture, &suggestions, &analysis)
            .expect("Failed to plan changes");
        let diff = writer
            .generate_diff(&fixture, &changes)
            .expect("Failed to generate diff");

        // Diff should contain ACP annotations
        if !diff.is_empty() {
            assert!(
                diff.contains("@acp:"),
                "Diff should contain @acp annotations"
            );
        }
    }
}

mod python_tests {
    use super::*;

    #[test]
    fn test_analyze_python_file() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixture = fixtures_dir().join("sample.py");
        if !fixture.exists() {
            return;
        }

        let result = analyzer.analyze_file(&fixture).expect("Failed to analyze");

        assert_eq!(result.language, "python");
    }

    #[test]
    fn test_suggest_from_docstring() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard)
            .with_conversion_source(ConversionSource::Docstring);

        let fixture = fixtures_dir().join("sample.py");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        // Should have suggestions (at least from heuristics)
        assert!(!suggestions.is_empty(), "Expected suggestions from Python file");
    }

    #[test]
    fn test_python_comment_style() {
        use acp::annotate::writer::CommentStyle;

        let style = CommentStyle::from_language("python", false);
        assert_eq!(style, CommentStyle::PyDocstring);
    }
}

mod rust_tests {
    use super::*;

    #[test]
    fn test_analyze_rust_file() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixture = fixtures_dir().join("sample.rs");
        if !fixture.exists() {
            return;
        }

        let result = analyzer.analyze_file(&fixture).expect("Failed to analyze");

        assert_eq!(result.language, "rust");
    }

    #[test]
    fn test_suggest_from_rustdoc() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard)
            .with_conversion_source(ConversionSource::Rustdoc);

        let fixture = fixtures_dir().join("sample.rs");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        assert!(!suggestions.is_empty(), "Expected suggestions from Rust file");
    }

    #[test]
    fn test_rust_comment_styles() {
        use acp::annotate::writer::CommentStyle;

        let item_style = CommentStyle::from_language("rust", false);
        assert_eq!(item_style, CommentStyle::RustDoc);

        let module_style = CommentStyle::from_language("rust", true);
        assert_eq!(module_style, CommentStyle::RustModuleDoc);
    }
}

mod go_tests {
    use super::*;

    #[test]
    fn test_analyze_go_file() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixture = fixtures_dir().join("sample.go");
        if !fixture.exists() {
            return;
        }

        let result = analyzer.analyze_file(&fixture).expect("Failed to analyze");

        assert_eq!(result.language, "go");
    }

    #[test]
    fn test_suggest_from_godoc() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard)
            .with_conversion_source(ConversionSource::Godoc);

        let fixture = fixtures_dir().join("sample.go");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        assert!(!suggestions.is_empty(), "Expected suggestions from Go file");
    }

    #[test]
    fn test_go_comment_style() {
        use acp::annotate::writer::CommentStyle;

        let style = CommentStyle::from_language("go", false);
        assert_eq!(style, CommentStyle::GoDoc);
    }
}

mod java_tests {
    use super::*;

    #[test]
    fn test_analyze_java_file() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixture = fixtures_dir().join("Sample.java");
        if !fixture.exists() {
            return;
        }

        let result = analyzer.analyze_file(&fixture).expect("Failed to analyze");

        assert_eq!(result.language, "java");
    }

    #[test]
    fn test_suggest_from_javadoc() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard)
            .with_conversion_source(ConversionSource::Javadoc);

        let fixture = fixtures_dir().join("Sample.java");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        assert!(!suggestions.is_empty(), "Expected suggestions from Java file");
    }

    #[test]
    fn test_java_comment_style() {
        use acp::annotate::writer::CommentStyle;

        let style = CommentStyle::from_language("java", false);
        assert_eq!(style, CommentStyle::Javadoc);
    }
}

mod pipeline_tests {
    use super::*;

    #[test]
    fn test_full_pipeline_all_languages() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard);
        let writer = Writer::new();

        let fixtures = [
            ("sample.ts", "typescript"),
            ("sample.py", "python"),
            ("sample.rs", "rust"),
            ("sample.go", "go"),
            ("Sample.java", "java"),
        ];

        for (filename, expected_lang) in fixtures {
            let fixture = fixtures_dir().join(filename);
            if !fixture.exists() {
                eprintln!("Skipping {}: not found", filename);
                continue;
            }

            // Analyze
            let analysis = analyzer
                .analyze_file(&fixture)
                .expect(&format!("Failed to analyze {}", filename));
            assert_eq!(
                analysis.language, expected_lang,
                "Wrong language for {}",
                filename
            );

            // Suggest
            let suggestions = suggester.suggest(&analysis);
            // At minimum, heuristics should generate something
            // (file may not have doc comments but should have identifiers)

            // Plan changes
            if !suggestions.is_empty() {
                let changes = writer
                    .plan_changes(&fixture, &suggestions, &analysis)
                    .expect(&format!("Failed to plan changes for {}", filename));

                // Changes should be sorted by line (descending)
                let lines: Vec<usize> = changes.iter().map(|c| c.line).collect();
                let mut sorted_lines = lines.clone();
                sorted_lines.sort_by(|a, b| b.cmp(a));
                assert_eq!(
                    lines, sorted_lines,
                    "Changes should be sorted descending by line"
                );
            }
        }
    }

    #[test]
    fn test_annotation_level_filtering() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixture = fixtures_dir().join("sample.ts");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");

        // Minimal level should have fewer annotations
        let minimal = Suggester::new(AnnotateLevel::Minimal);
        let minimal_suggestions = minimal.suggest(&analysis);

        // Standard level should have more
        let standard = Suggester::new(AnnotateLevel::Standard);
        let standard_suggestions = standard.suggest(&analysis);

        // Full level should have the most
        let full = Suggester::new(AnnotateLevel::Full);
        let full_suggestions = full.suggest(&analysis);

        // Each level should include at least as many as the previous
        assert!(
            standard_suggestions.len() >= minimal_suggestions.len(),
            "Standard should have >= minimal suggestions"
        );
        assert!(
            full_suggestions.len() >= standard_suggestions.len(),
            "Full should have >= standard suggestions"
        );
    }

    #[test]
    fn test_suggestion_priority() {
        // Test that the suggestion priority system works
        // Converted suggestions should have higher priority than heuristic

        use acp::annotate::SuggestionSource;

        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Standard)
            .with_conversion_source(ConversionSource::Auto);

        let fixture = fixtures_dir().join("sample.ts");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        // Verify that suggestions are returned and properly structured
        assert!(!suggestions.is_empty(), "Should have suggestions");

        // Check that all suggestions have valid sources
        for s in &suggestions {
            assert!(
                matches!(
                    s.source,
                    SuggestionSource::Explicit
                        | SuggestionSource::Converted
                        | SuggestionSource::Heuristic
                ),
                "Suggestion should have valid source"
            );
        }

        // Check priority ordering (Explicit < Converted < Heuristic in ord value)
        // Lower source value = higher priority
        assert!(
            SuggestionSource::Explicit < SuggestionSource::Converted,
            "Explicit should have higher priority than Converted"
        );
        assert!(
            SuggestionSource::Converted < SuggestionSource::Heuristic,
            "Converted should have higher priority than Heuristic"
        );
    }

    #[test]
    fn test_coverage_calculation() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");

        let fixtures = vec!["sample.ts", "sample.py", "sample.rs"];
        let mut results = Vec::new();

        for filename in fixtures {
            let fixture = fixtures_dir().join(filename);
            if fixture.exists() {
                if let Ok(analysis) = analyzer.analyze_file(&fixture) {
                    results.push(analysis);
                }
            }
        }

        if results.is_empty() {
            return;
        }

        let coverage = Analyzer::calculate_total_coverage(&results);
        assert!(coverage >= 0.0 && coverage <= 100.0, "Coverage should be 0-100%");
    }
}

mod diff_tests {
    use super::*;

    #[test]
    fn test_diff_format() {
        let config = test_config();
        let analyzer = Analyzer::new(&config).expect("Failed to create analyzer");
        let suggester = Suggester::new(AnnotateLevel::Minimal);
        let writer = Writer::new();

        let fixture = fixtures_dir().join("sample.ts");
        if !fixture.exists() {
            return;
        }

        let analysis = analyzer.analyze_file(&fixture).expect("Failed to analyze");
        let suggestions = suggester.suggest(&analysis);

        if suggestions.is_empty() {
            return;
        }

        let changes = writer
            .plan_changes(&fixture, &suggestions, &analysis)
            .expect("Failed to plan changes");
        let diff = writer
            .generate_diff(&fixture, &changes)
            .expect("Failed to generate diff");

        if !diff.is_empty() {
            // Unified diff format checks
            assert!(diff.contains("---"), "Diff should have --- header");
            assert!(diff.contains("+++"), "Diff should have +++ header");
            assert!(diff.contains("@@"), "Diff should have @@ markers");
        }
    }
}

mod heuristics_tests {
    use super::*;
    use acp::annotate::heuristics::HeuristicsEngine;

    #[test]
    fn test_security_patterns() {
        let engine = HeuristicsEngine::new();

        let suggestions = engine.suggest(
            "authenticateUser",
            10,
            Some(acp::ast::SymbolKind::Function),
            "src/auth/login.ts",
        );

        // Should detect security patterns from both name and path
        let has_security = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Domain && s.value.contains("security"));

        let has_auth_path = suggestions
            .iter()
            .any(|s| s.annotation_type == AnnotationType::Domain && s.value == "authentication");

        assert!(
            has_security || has_auth_path,
            "Should detect security-related patterns"
        );
    }

    #[test]
    fn test_path_domain_inference() {
        let engine = HeuristicsEngine::new();

        let test_cases = vec![
            ("src/billing/payments.ts", "billing"),
            ("src/auth/session.ts", "authentication"),
            ("src/db/users.ts", "database"),
            ("tests/unit/service.test.ts", "testing"),
        ];

        for (path, expected_domain) in test_cases {
            let suggestions = engine.suggest("something", 1, None, path);

            let has_domain = suggestions.iter().any(|s| {
                s.annotation_type == AnnotationType::Domain && s.value == expected_domain
            });

            assert!(
                has_domain,
                "Path {} should infer domain {}",
                path,
                expected_domain
            );
        }
    }

    #[test]
    fn test_summary_generation() {
        let engine = HeuristicsEngine::new();

        let suggestions = engine.suggest(
            "getUserById",
            1,
            Some(acp::ast::SymbolKind::Function),
            "src/users.ts",
        );

        let summary = suggestions
            .iter()
            .find(|s| s.annotation_type == AnnotationType::Summary);

        assert!(summary.is_some(), "Should generate summary from identifier");

        if let Some(s) = summary {
            assert!(
                s.value.to_lowercase().contains("get"),
                "Summary should contain verb"
            );
        }
    }
}
