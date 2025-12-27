//! @acp:module "Schema Validation"
//! @acp:summary "JSON schema validation for ACP files using jsonschema crate"
//! @acp:domain cli
//! @acp:layer utility
//!
//! Provides comprehensive validation of ACP JSON files against their schemas.
//! Uses embedded schemas compiled once at runtime for efficient repeated validation.

use std::sync::OnceLock;
use jsonschema::{Validator, Draft, Retrieve, Uri};
use crate::error::{AcpError, Result};
use crate::attempts::AttemptTracker;

// Embed schemas at compile time from acp-spec submodule (source of truth)
static CACHE_SCHEMA_STR: &str = include_str!("../acp-spec/schemas/v1/cache.schema.json");
static VARS_SCHEMA_STR: &str = include_str!("../acp-spec/schemas/v1/vars.schema.json");
static CONFIG_SCHEMA_STR: &str = include_str!("../acp-spec/schemas/v1/config.schema.json");
static ATTEMPTS_SCHEMA_STR: &str = include_str!("../acp-spec/schemas/v1/attempts.schema.json");
static SYNC_SCHEMA_STR: &str = include_str!("../acp-spec/schemas/v1/sync.schema.json");
static PRIMER_SCHEMA_STR: &str = include_str!("../acp-spec/schemas/v1/primer.schema.json");

// Compiled schema validators (lazy initialization)
static CACHE_VALIDATOR: OnceLock<Validator> = OnceLock::new();
static VARS_VALIDATOR: OnceLock<Validator> = OnceLock::new();
static CONFIG_VALIDATOR: OnceLock<Validator> = OnceLock::new();
static ATTEMPTS_VALIDATOR: OnceLock<Validator> = OnceLock::new();
static SYNC_VALIDATOR: OnceLock<Validator> = OnceLock::new();
static PRIMER_VALIDATOR: OnceLock<Validator> = OnceLock::new();

/// A retriever that returns embedded schemas for ACP URLs and fails for others
struct AcpRetriever;

impl Retrieve for AcpRetriever {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let uri_str = uri.as_str();

        // Return embedded schemas for known ACP URLs
        if uri_str == "https://acp-protocol.dev/schemas/v1/sync.schema.json" {
            let mut schema: serde_json::Value = serde_json::from_str(SYNC_SCHEMA_STR)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            // Remove $schema to prevent recursive meta-schema lookup
            if let Some(obj) = schema.as_object_mut() {
                obj.remove("$schema");
            }
            return Ok(schema);
        }

        // For meta-schemas, return a permissive empty schema
        // This allows validation without fetching external resources
        if uri_str.starts_with("https://json-schema.org/") {
            return Ok(serde_json::json!({}));
        }

        Err(format!("Unknown schema URI: {}", uri_str).into())
    }
}

/// Compile a JSON Schema from a string using Draft 7 with embedded retriever
fn compile_schema(schema_str: &str, name: &str) -> Validator {
    let mut schema: serde_json::Value = serde_json::from_str(schema_str)
        .unwrap_or_else(|e| panic!("Invalid {} schema JSON: {}", name, e));

    // Remove $schema field to prevent meta-schema lookup
    // We explicitly set Draft7 below
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("$schema");
    }

    // Build validator with custom retriever to avoid external fetches
    jsonschema::options()
        .with_draft(Draft::Draft7)
        .with_retriever(AcpRetriever)
        .build(&schema)
        .unwrap_or_else(|e| panic!("Failed to compile {} schema: {}", name, e))
}

fn get_cache_validator() -> &'static Validator {
    CACHE_VALIDATOR.get_or_init(|| compile_schema(CACHE_SCHEMA_STR, "cache"))
}

fn get_vars_validator() -> &'static Validator {
    VARS_VALIDATOR.get_or_init(|| compile_schema(VARS_SCHEMA_STR, "vars"))
}

fn get_config_validator() -> &'static Validator {
    CONFIG_VALIDATOR.get_or_init(|| compile_schema(CONFIG_SCHEMA_STR, "config"))
}

fn get_attempts_validator() -> &'static Validator {
    ATTEMPTS_VALIDATOR.get_or_init(|| compile_schema(ATTEMPTS_SCHEMA_STR, "attempts"))
}

fn get_sync_validator() -> &'static Validator {
    SYNC_VALIDATOR.get_or_init(|| compile_schema(SYNC_SCHEMA_STR, "sync"))
}

fn get_primer_validator() -> &'static Validator {
    PRIMER_VALIDATOR.get_or_init(|| compile_schema(PRIMER_SCHEMA_STR, "primer"))
}

/// Collect validation errors into a formatted string
fn collect_errors<'a>(errors: impl Iterator<Item = jsonschema::error::ValidationError<'a>>) -> String {
    errors
        .map(|e| format!("{}", e))
        .collect::<Vec<_>>()
        .join("; ")
}

/// @acp:summary "Validate cache file against schema"
pub fn validate_cache(json: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let validator = get_cache_validator();

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        return Err(AcpError::SchemaValidation(collect_errors(errors.into_iter())));
    }

    // Also validate with serde for type checking
    let _: crate::cache::Cache = serde_json::from_value(value)?;
    Ok(())
}

/// @acp:summary "Validate vars file against schema"
pub fn validate_vars(json: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let validator = get_vars_validator();

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        return Err(AcpError::SchemaValidation(collect_errors(errors.into_iter())));
    }

    // Also validate with serde for type checking
    let _: crate::vars::VarsFile = serde_json::from_value(value)?;
    Ok(())
}

/// @acp:summary "Validate config file against schema"
pub fn validate_config(json: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let validator = get_config_validator();

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        return Err(AcpError::SchemaValidation(collect_errors(errors.into_iter())));
    }

    // Also validate with serde for type checking
    let _: crate::config::Config = serde_json::from_value(value)?;
    Ok(())
}

/// @acp:summary "Validate attempts file against schema"
pub fn validate_attempts(json: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let validator = get_attempts_validator();

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        return Err(AcpError::SchemaValidation(collect_errors(errors.into_iter())));
    }

    // Also validate with serde for type checking
    let tracker: AttemptTracker = serde_json::from_value(value)?;

    // Semantic validation
    validate_attempts_semantic(&tracker)?;

    Ok(())
}

/// @acp:summary "Validate sync config against schema"
pub fn validate_sync(json: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let validator = get_sync_validator();

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        return Err(AcpError::SchemaValidation(collect_errors(errors.into_iter())));
    }

    // Semantic validation via JSON value (no Rust struct yet)
    validate_sync_semantic(&value)?;

    Ok(())
}

/// @acp:summary "Validate primer definition against schema"
pub fn validate_primer(json: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let validator = get_primer_validator();

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        return Err(AcpError::SchemaValidation(collect_errors(errors.into_iter())));
    }

    // Semantic validation via JSON value (no Rust struct yet)
    validate_primer_semantic(&value)?;

    Ok(())
}

// ============================================================================
// Semantic Validation Functions
// ============================================================================

/// @acp:summary "Semantic validation for attempts that cannot be expressed in JSON Schema"
fn validate_attempts_semantic(tracker: &AttemptTracker) -> Result<()> {
    // Check lines_changed order: start_line <= end_line
    for (attempt_id, attempt) in &tracker.attempts {
        for file in &attempt.files {
            if let Some([start, end]) = file.lines_changed {
                if start > end {
                    return Err(AcpError::SemanticValidation(format!(
                        "In attempt '{}', file '{}': lines_changed start ({}) > end ({})",
                        attempt_id, file.path, start, end
                    )));
                }
            }
        }
    }

    // Check history entries have valid ordering
    for (i, entry) in tracker.history.iter().enumerate() {
        if entry.started_at > entry.ended_at {
            return Err(AcpError::SemanticValidation(format!(
                "History entry {} ({}): started_at > ended_at",
                i, entry.id
            )));
        }
    }

    Ok(())
}

/// @acp:summary "Semantic validation for sync config"
fn validate_sync_semantic(value: &serde_json::Value) -> Result<()> {
    // Warn (but don't error) on tool overlap between tools and exclude arrays
    if let (Some(tools), Some(exclude)) = (
        value.get("tools").and_then(|v| v.as_array()),
        value.get("exclude").and_then(|v| v.as_array()),
    ) {
        let tool_set: std::collections::HashSet<_> = tools.iter()
            .filter_map(|v| v.as_str())
            .collect();

        let overlap: Vec<_> = exclude.iter()
            .filter_map(|v| v.as_str())
            .filter(|t| tool_set.contains(t))
            .collect();

        if !overlap.is_empty() {
            tracing::warn!(
                "Tools appear in both 'tools' and 'exclude' arrays: {:?}. Behavior is undefined.",
                overlap
            );
        }
    }

    Ok(())
}

/// @acp:summary "Semantic validation for primer definitions"
fn validate_primer_semantic(value: &serde_json::Value) -> Result<()> {
    if let Some(sections) = value.get("sections").and_then(|v| v.as_array()) {
        let section_ids: std::collections::HashSet<_> = sections.iter()
            .filter_map(|s| s.get("id"))
            .filter_map(|id| id.as_str())
            .collect();

        for section in sections {
            let section_id = section.get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("unknown");

            // Check for self-references in conflictsWith
            if let Some(conflicts) = section.get("conflictsWith").and_then(|v| v.as_array()) {
                for conflict in conflicts {
                    if let Some(conflict_id) = conflict.as_str() {
                        if conflict_id == section_id {
                            return Err(AcpError::SemanticValidation(format!(
                                "Section '{}' has self-reference in conflictsWith",
                                section_id
                            )));
                        }
                    }
                }
            }

            // Check for circular dependencies (simple check - warns if dependsOn references non-existent sections)
            if let Some(depends) = section.get("dependsOn").and_then(|v| v.as_array()) {
                for dep in depends {
                    if let Some(dep_id) = dep.as_str() {
                        if !section_ids.contains(dep_id) {
                            tracing::warn!(
                                "Section '{}' depends on '{}' which does not exist",
                                section_id, dep_id
                            );
                        }
                    }
                }
            }
        }

        // Full cycle detection using DFS
        if let Err(cycle) = detect_dependency_cycles(sections) {
            return Err(AcpError::SemanticValidation(format!(
                "Circular dependency detected in primer sections: {}",
                cycle
            )));
        }
    }

    Ok(())
}

/// Detect cycles in section dependencies using DFS
fn detect_dependency_cycles(sections: &[serde_json::Value]) -> std::result::Result<(), String> {
    use std::collections::{HashMap, HashSet};

    // Build adjacency list
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();

    for section in sections {
        if let Some(id) = section.get("id").and_then(|v| v.as_str()) {
            let dep_list = section.get("dependsOn")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|d| d.as_str()).collect())
                .unwrap_or_default();
            deps.insert(id, dep_list);
        }
    }

    // DFS for cycle detection
    let mut visited: HashSet<&str> = HashSet::new();
    let mut rec_stack: HashSet<&str> = HashSet::new();
    let mut path: Vec<&str> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        deps: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        rec_stack: &mut HashSet<&'a str>,
        path: &mut Vec<&'a str>,
    ) -> std::result::Result<(), String> {
        visited.insert(node);
        rec_stack.insert(node);
        path.push(node);

        if let Some(neighbors) = deps.get(node) {
            for &neighbor in neighbors {
                if !visited.contains(neighbor) {
                    dfs(neighbor, deps, visited, rec_stack, path)?;
                } else if rec_stack.contains(neighbor) {
                    // Found cycle - construct path
                    let cycle_start = path.iter().position(|&n| n == neighbor).unwrap();
                    let cycle: Vec<_> = path[cycle_start..].to_vec();
                    return Err(format!("{} -> {}", cycle.join(" -> "), neighbor));
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
        Ok(())
    }

    for &node in deps.keys() {
        if !visited.contains(node) {
            dfs(node, &deps, &mut visited, &mut rec_stack, &mut path)?;
        }
    }

    Ok(())
}

/// @acp:summary "Detect schema type from filename"
pub fn detect_schema_type(filename: &str) -> Option<&'static str> {
    let lower = filename.to_lowercase();

    if lower.contains("cache") || lower.ends_with(".acp.cache.json") {
        Some("cache")
    } else if lower.contains("vars") || lower.ends_with(".acp.vars.json") {
        Some("vars")
    } else if lower.contains("config") || lower.ends_with(".acp.config.json") {
        Some("config")
    } else if lower.contains("attempts") || lower.ends_with("acp.attempts.json") {
        Some("attempts")
    } else if lower.contains("sync") || lower.ends_with("acp.sync.json") {
        Some("sync")
    } else if lower.contains("primer") {
        Some("primer")
    } else {
        None
    }
}

/// @acp:summary "Validate JSON against a specific schema type"
pub fn validate_by_type(json: &str, schema_type: &str) -> Result<()> {
    match schema_type {
        "cache" => validate_cache(json),
        "vars" => validate_vars(json),
        "config" => validate_config(json),
        "attempts" => validate_attempts(json),
        "sync" => validate_sync(json),
        "primer" => validate_primer(json),
        _ => Err(AcpError::Other(format!("Unknown schema type: {}", schema_type))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_schema_type() {
        assert_eq!(detect_schema_type(".acp.cache.json"), Some("cache"));
        assert_eq!(detect_schema_type(".acp/acp.vars.json"), Some("vars"));
        assert_eq!(detect_schema_type(".acp.config.json"), Some("config"));
        assert_eq!(detect_schema_type(".acp/acp.attempts.json"), Some("attempts"));
        assert_eq!(detect_schema_type("acp.sync.json"), Some("sync"));
        assert_eq!(detect_schema_type("primer.defaults.json"), Some("primer"));
        assert_eq!(detect_schema_type("random.json"), None);
    }

    #[test]
    fn test_dependency_cycle_detection() {
        // No cycle
        let sections: Vec<serde_json::Value> = serde_json::from_str(r#"[
            {"id": "a", "dependsOn": ["b"]},
            {"id": "b", "dependsOn": ["c"]},
            {"id": "c"}
        ]"#).unwrap();
        assert!(detect_dependency_cycles(&sections).is_ok());

        // With cycle
        let sections_with_cycle: Vec<serde_json::Value> = serde_json::from_str(r#"[
            {"id": "a", "dependsOn": ["b"]},
            {"id": "b", "dependsOn": ["c"]},
            {"id": "c", "dependsOn": ["a"]}
        ]"#).unwrap();
        assert!(detect_dependency_cycles(&sections_with_cycle).is_err());
    }
}
