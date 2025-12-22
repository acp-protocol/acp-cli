//! Integration tests for ACP schema validation
//!
//! Tests valid and invalid fixtures against all ACP schemas.

use std::fs;
use std::path::Path;

/// Test all valid cache fixtures pass validation
#[test]
fn test_cache_valid_fixtures() {
    test_valid_fixtures("cache", acp::schema::validate_cache);
}

/// Test all invalid cache fixtures fail validation
#[test]
fn test_cache_invalid_fixtures() {
    test_invalid_fixtures("cache", acp::schema::validate_cache);
}

/// Test all valid config fixtures pass validation
#[test]
fn test_config_valid_fixtures() {
    test_valid_fixtures("config", acp::schema::validate_config);
}

/// Test all invalid config fixtures fail validation
#[test]
fn test_config_invalid_fixtures() {
    test_invalid_fixtures("config", acp::schema::validate_config);
}

/// Test all valid vars fixtures pass validation
#[test]
fn test_vars_valid_fixtures() {
    test_valid_fixtures("vars", acp::schema::validate_vars);
}

/// Test all invalid vars fixtures fail validation
#[test]
fn test_vars_invalid_fixtures() {
    test_invalid_fixtures("vars", acp::schema::validate_vars);
}

/// Test all valid attempts fixtures pass validation
#[test]
fn test_attempts_valid_fixtures() {
    test_valid_fixtures("attempts", acp::schema::validate_attempts);
}

/// Test all invalid attempts fixtures fail validation
#[test]
fn test_attempts_invalid_fixtures() {
    test_invalid_fixtures("attempts", acp::schema::validate_attempts);
}

/// Test all valid sync fixtures pass validation
#[test]
fn test_sync_valid_fixtures() {
    test_valid_fixtures("sync", acp::schema::validate_sync);
}

/// Test all invalid sync fixtures fail validation
#[test]
fn test_sync_invalid_fixtures() {
    test_invalid_fixtures("sync", acp::schema::validate_sync);
}

/// Test all valid primer fixtures pass validation
#[test]
fn test_primer_valid_fixtures() {
    test_valid_fixtures("primer", acp::schema::validate_primer);
}

/// Test all invalid primer fixtures fail validation
#[test]
fn test_primer_invalid_fixtures() {
    test_invalid_fixtures("primer", acp::schema::validate_primer);
}

/// Test schema type detection from filenames
#[test]
fn test_schema_type_detection() {
    use acp::schema::detect_schema_type;

    // Cache schema
    assert_eq!(detect_schema_type(".acp.cache.json"), Some("cache"));
    assert_eq!(detect_schema_type("project.acp.cache.json"), Some("cache"));

    // Vars schema
    assert_eq!(detect_schema_type(".acp/acp.vars.json"), Some("vars"));

    // Config schema
    assert_eq!(detect_schema_type(".acp.config.json"), Some("config"));

    // Attempts schema
    assert_eq!(detect_schema_type(".acp/acp.attempts.json"), Some("attempts"));

    // Sync schema
    assert_eq!(detect_schema_type("acp.sync.json"), Some("sync"));

    // Primer schema
    assert_eq!(detect_schema_type("primer.defaults.json"), Some("primer"));
    assert_eq!(detect_schema_type("my-primer.json"), Some("primer"));

    // Unknown
    assert_eq!(detect_schema_type("random.json"), None);
}

/// Test validate_by_type function
#[test]
fn test_validate_by_type() {
    use acp::schema::validate_by_type;

    let valid_sync = r#"{"version": "1.0.0"}"#;
    assert!(validate_by_type(valid_sync, "sync").is_ok());

    let invalid_sync = r#"{"tools": ["invalid-tool"]}"#;
    assert!(validate_by_type(invalid_sync, "sync").is_err());

    // Unknown type
    assert!(validate_by_type("{}", "unknown").is_err());
}

// ============================================================================
// Helper Functions
// ============================================================================

fn test_valid_fixtures<F>(schema_type: &str, validate_fn: F)
where
    F: Fn(&str) -> acp::error::Result<()>,
{
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/schemas")
        .join(schema_type)
        .join("valid");

    if !fixtures_dir.exists() {
        panic!("Fixtures directory does not exist: {:?}", fixtures_dir);
    }

    let mut tested = 0;
    for entry in fs::read_dir(&fixtures_dir).expect("Failed to read fixtures directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", path, e));

        let result = validate_fn(&content);
        assert!(
            result.is_ok(),
            "Valid fixture {:?} failed validation: {:?}",
            path.file_name().unwrap(),
            result.err()
        );
        tested += 1;
    }

    assert!(
        tested > 0,
        "No valid fixtures found in {:?}",
        fixtures_dir
    );
    println!(
        "Validated {} valid {} fixtures",
        tested, schema_type
    );
}

fn test_invalid_fixtures<F>(schema_type: &str, validate_fn: F)
where
    F: Fn(&str) -> acp::error::Result<()>,
{
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/schemas")
        .join(schema_type)
        .join("invalid");

    if !fixtures_dir.exists() {
        panic!("Fixtures directory does not exist: {:?}", fixtures_dir);
    }

    let mut tested = 0;
    for entry in fs::read_dir(&fixtures_dir).expect("Failed to read fixtures directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", path, e));

        let result = validate_fn(&content);
        assert!(
            result.is_err(),
            "Invalid fixture {:?} should have failed validation but passed",
            path.file_name().unwrap()
        );
        tested += 1;
    }

    assert!(
        tested > 0,
        "No invalid fixtures found in {:?}",
        fixtures_dir
    );
    println!(
        "Validated {} invalid {} fixtures",
        tested, schema_type
    );
}
