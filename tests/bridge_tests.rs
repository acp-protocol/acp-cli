//! RFC-0006: Bridge integration tests
//!
//! Tests for documentation system bridging functionality.

use acp::annotate::converters::ParsedDocumentation;
use acp::bridge::merger::AcpAnnotations;
use acp::bridge::{BridgeConfig, BridgeMerger, BridgeResult, FormatDetector};
use acp::cache::{BridgeMetadata, BridgeSource, BridgeStats, BridgeSummary, SourceFormat};

// =============================================================================
// T5.1: Format Detection Tests
// =============================================================================

mod format_detection_tests {
    use super::*;

    #[test]
    fn test_detect_jsdoc_in_typescript() {
        let config = BridgeConfig::enabled();
        let detector = FormatDetector::new(&config);

        let source = r#"
/**
 * Process user data
 * @param {string} userId - The user ID
 * @param {Object} options - Processing options
 * @returns {Promise<User>} The processed user
 * @throws {ValidationError} If userId is invalid
 */
function processUser(userId, options) {
    // ...
}
"#;
        assert_eq!(
            detector.detect(source, "typescript"),
            Some(SourceFormat::Jsdoc)
        );
    }

    #[test]
    fn test_detect_google_docstring_in_python() {
        let config = BridgeConfig::enabled();
        let detector = FormatDetector::new(&config);

        let source = r#"
def process_user(user_id: str, options: dict) -> User:
    """Process user data.

    Args:
        user_id: The user ID.
        options: Processing options.

    Returns:
        The processed user.

    Raises:
        ValidationError: If user_id is invalid.
    """
    pass
"#;
        assert_eq!(
            detector.detect(source, "python"),
            Some(SourceFormat::DocstringGoogle)
        );
    }

    #[test]
    fn test_detect_numpy_docstring_in_python() {
        let config = BridgeConfig::enabled();
        let detector = FormatDetector::new(&config);

        let source = r#"
def process_user(user_id, options):
    """Process user data.

    Parameters
    ----------
    user_id : str
        The user ID.
    options : dict
        Processing options.

    Returns
    -------
    User
        The processed user.
    """
    pass
"#;
        assert_eq!(
            detector.detect(source, "python"),
            Some(SourceFormat::DocstringNumpy)
        );
    }

    #[test]
    fn test_detect_sphinx_docstring_in_python() {
        let config = BridgeConfig::enabled();
        let detector = FormatDetector::new(&config);

        let source = r#"
def process_user(user_id, options):
    """Process user data.

    :param user_id: The user ID.
    :type user_id: str
    :param options: Processing options.
    :returns: The processed user.
    :rtype: User
    """
    pass
"#;
        assert_eq!(
            detector.detect(source, "python"),
            Some(SourceFormat::DocstringSphinx)
        );
    }

    #[test]
    fn test_detect_rustdoc() {
        let config = BridgeConfig::enabled();
        let detector = FormatDetector::new(&config);

        let source = r#"
/// Process user data.
///
/// # Arguments
///
/// * `user_id` - The user ID
/// * `options` - Processing options
///
/// # Returns
///
/// The processed user.
///
/// # Panics
///
/// If user_id is empty.
fn process_user(user_id: &str, options: Options) -> User {
    todo!()
}
"#;
        assert_eq!(detector.detect(source, "rust"), Some(SourceFormat::Rustdoc));
    }

    #[test]
    fn test_detect_disabled() {
        let config = BridgeConfig::new(); // disabled
        let detector = FormatDetector::new(&config);

        let source = "@param {string} name";
        assert_eq!(detector.detect(source, "typescript"), None);
    }

    #[test]
    fn test_detect_language_disabled() {
        let mut config = BridgeConfig::enabled();
        config.python.enabled = false;
        let detector = FormatDetector::new(&config);

        let source = "Args:\n    query: Search query.";
        assert_eq!(detector.detect(source, "python"), None);
    }
}

// =============================================================================
// T5.1: Merge Tests
// =============================================================================

mod merge_tests {
    use super::*;

    fn enabled_config() -> BridgeConfig {
        BridgeConfig::enabled()
    }

    #[test]
    fn test_merge_native_only() {
        let merger = BridgeMerger::new(&enabled_config());

        let mut native = ParsedDocumentation::new();
        native.summary = Some("Native function summary".to_string());
        native.params.push((
            "userId".to_string(),
            Some("string".to_string()),
            Some("The user ID".to_string()),
        ));
        native.returns = Some((
            Some("User".to_string()),
            Some("The user object".to_string()),
        ));

        let acp = AcpAnnotations::default();
        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        assert_eq!(result.summary, Some("Native function summary".to_string()));
        assert_eq!(result.source, BridgeSource::Converted);
        assert_eq!(result.source_formats, vec![SourceFormat::Jsdoc]);
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0].name, "userId");
        assert_eq!(result.params[0].r#type, Some("string".to_string()));
    }

    #[test]
    fn test_merge_acp_only() {
        let merger = BridgeMerger::new(&enabled_config());

        let acp = AcpAnnotations {
            summary: Some("ACP function summary".to_string()),
            directive: Some("MUST validate input".to_string()),
            params: vec![("userId".to_string(), "MUST be a valid UUID".to_string())],
            returns: Some("MAY return null if not found".to_string()),
            throws: vec![],
        };

        let result = merger.merge(None, SourceFormat::Acp, &acp);

        assert_eq!(result.summary, Some("ACP function summary".to_string()));
        assert_eq!(result.directive, Some("MUST validate input".to_string()));
        assert_eq!(result.source, BridgeSource::Explicit);
        assert_eq!(result.params.len(), 1);
        assert_eq!(
            result.params[0].directive,
            Some("MUST be a valid UUID".to_string())
        );
    }

    #[test]
    fn test_merge_acp_first() {
        let config = enabled_config(); // Default: acp-first
        let merger = BridgeMerger::new(&config);

        let mut native = ParsedDocumentation::new();
        native.summary = Some("Native summary".to_string());
        native.params.push((
            "userId".to_string(),
            Some("string".to_string()),
            Some("The user ID".to_string()),
        ));

        let acp = AcpAnnotations {
            directive: Some("MUST authenticate".to_string()),
            params: vec![("userId".to_string(), "MUST be UUID".to_string())],
            ..Default::default()
        };

        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        // In acp-first mode: native summary + ACP directive
        assert_eq!(result.summary, Some("Native summary".to_string()));
        assert_eq!(result.directive, Some("MUST authenticate".to_string()));
        assert_eq!(result.source, BridgeSource::Merged);

        // Param should have native description + ACP directive
        assert_eq!(result.params.len(), 1);
        assert_eq!(
            result.params[0].description,
            Some("The user ID".to_string())
        );
        assert_eq!(result.params[0].directive, Some("MUST be UUID".to_string()));
        assert_eq!(result.params[0].source, BridgeSource::Merged);
    }

    #[test]
    fn test_merge_with_throws() {
        let merger = BridgeMerger::new(&enabled_config());

        let mut native = ParsedDocumentation::new();
        native.throws.push((
            "ValidationError".to_string(),
            Some("If input is invalid".to_string()),
        ));
        native.throws.push((
            "NetworkError".to_string(),
            Some("If network fails".to_string()),
        ));

        let acp = AcpAnnotations {
            throws: vec![(
                "ValidationError".to_string(),
                "MUST be caught and handled".to_string(),
            )],
            ..Default::default()
        };

        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        assert_eq!(result.throws.len(), 2);

        // First throw should be merged
        let validation_error = result
            .throws
            .iter()
            .find(|t| t.exception == "ValidationError")
            .unwrap();
        assert_eq!(
            validation_error.description,
            Some("If input is invalid".to_string())
        );
        assert_eq!(
            validation_error.directive,
            Some("MUST be caught and handled".to_string())
        );
        assert_eq!(validation_error.source, BridgeSource::Merged);

        // Second throw should be converted only
        let network_error = result
            .throws
            .iter()
            .find(|t| t.exception == "NetworkError")
            .unwrap();
        assert_eq!(
            network_error.description,
            Some("If network fails".to_string())
        );
        assert!(network_error.directive.is_none());
        assert_eq!(network_error.source, BridgeSource::Converted);
    }
}

// =============================================================================
// T5.1: Bridge Statistics Tests
// =============================================================================

mod statistics_tests {
    use super::*;

    #[test]
    fn test_bridge_metadata_is_empty() {
        let empty = BridgeMetadata::default();
        assert!(empty.is_empty());

        let with_data = BridgeMetadata {
            enabled: true,
            detected_format: Some(SourceFormat::Jsdoc),
            converted_count: 5,
            merged_count: 0,
            explicit_count: 3,
        };
        assert!(!with_data.is_empty());
    }

    #[test]
    fn test_bridge_stats_is_empty() {
        let empty = BridgeStats::default();
        assert!(empty.is_empty());

        let with_data = BridgeStats {
            enabled: true,
            precedence: "acp-first".to_string(),
            summary: BridgeSummary {
                total_annotations: 10,
                explicit_count: 5,
                converted_count: 3,
                merged_count: 2,
            },
            by_format: std::collections::HashMap::new(),
        };
        assert!(!with_data.is_empty());
    }

    #[test]
    fn test_bridge_stats_serialization() {
        let stats = BridgeStats {
            enabled: true,
            precedence: "acp-first".to_string(),
            summary: BridgeSummary {
                total_annotations: 10,
                explicit_count: 5,
                converted_count: 3,
                merged_count: 2,
            },
            by_format: [("jsdoc".to_string(), 5u64)].into_iter().collect(),
        };

        let json = serde_json::to_string_pretty(&stats).unwrap();
        let parsed: BridgeStats = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.enabled, true);
        assert_eq!(parsed.precedence, "acp-first");
        assert_eq!(parsed.summary.total_annotations, 10);
        assert_eq!(parsed.by_format.get("jsdoc"), Some(&5));
    }
}

// =============================================================================
// T5.1: Bridge Result Tests
// =============================================================================

mod result_tests {
    use super::*;

    #[test]
    fn test_bridge_result_from_acp() {
        let result = BridgeResult::from_acp(
            Some("Test summary".to_string()),
            Some("MUST validate".to_string()),
        );

        assert_eq!(result.summary, Some("Test summary".to_string()));
        assert_eq!(result.directive, Some("MUST validate".to_string()));
        assert_eq!(result.source, BridgeSource::Explicit);
        assert_eq!(result.source_formats, vec![SourceFormat::Acp]);
    }

    #[test]
    fn test_bridge_result_from_native() {
        let mut parsed = ParsedDocumentation::new();
        parsed.summary = Some("Native summary".to_string());
        parsed.params.push((
            "id".to_string(),
            Some("number".to_string()),
            Some("ID value".to_string()),
        ));
        parsed.returns = Some((Some("string".to_string()), Some("Result".to_string())));

        let result = BridgeResult::from_native(&parsed, SourceFormat::Jsdoc);

        assert_eq!(result.summary, Some("Native summary".to_string()));
        assert_eq!(result.source, BridgeSource::Converted);
        assert_eq!(result.source_formats, vec![SourceFormat::Jsdoc]);
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0].r#type, Some("number".to_string()));
        assert!(result.returns.is_some());
    }
}

// =============================================================================
// T5.1: Source Format Tests
// =============================================================================

mod source_format_tests {
    use super::*;

    #[test]
    fn test_source_format_serialization() {
        let formats = vec![
            (SourceFormat::Acp, "\"acp\""),
            (SourceFormat::Jsdoc, "\"jsdoc\""),
            (SourceFormat::DocstringGoogle, "\"docstring:google\""),
            (SourceFormat::DocstringNumpy, "\"docstring:numpy\""),
            (SourceFormat::DocstringSphinx, "\"docstring:sphinx\""),
            (SourceFormat::Rustdoc, "\"rustdoc\""),
            (SourceFormat::Javadoc, "\"javadoc\""),
            (SourceFormat::Godoc, "\"godoc\""),
            (SourceFormat::TypeHint, "\"type_hint\""),
        ];

        for (format, expected_json) in formats {
            let json = serde_json::to_string(&format).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", format);

            let parsed: SourceFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, format);
        }
    }

    #[test]
    fn test_bridge_source_serialization() {
        let sources = vec![
            (BridgeSource::Explicit, "\"explicit\""),
            (BridgeSource::Converted, "\"converted\""),
            (BridgeSource::Merged, "\"merged\""),
            (BridgeSource::Heuristic, "\"heuristic\""),
        ];

        for (source, expected_json) in sources {
            let json = serde_json::to_string(&source).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", source);

            let parsed: BridgeSource = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, source);
        }
    }
}
