//! @acp:module "Content Merge Logic"
//! @acp:summary "Handles merging generated content with existing files"
//! @acp:domain cli
//! @acp:layer service

use super::tool::MergeStrategy;

/// Merge generated content with existing file content
pub fn merge_content(
    strategy: MergeStrategy,
    existing: &str,
    generated: &str,
    start_marker: &str,
    end_marker: &str,
) -> String {
    match strategy {
        MergeStrategy::Replace => generated.to_string(),

        MergeStrategy::Section => {
            merge_with_markers(existing, generated, start_marker, end_marker)
        }

        MergeStrategy::Append => {
            format!("{}\n\n{}", existing.trim_end(), generated)
        }

        MergeStrategy::Merge => {
            // For JSON/YAML deep merge - handled separately in adapters
            // Fall back to section merge for safety
            merge_with_markers(existing, generated, start_marker, end_marker)
        }
    }
}

/// Merge content using section markers
///
/// If markers exist in the existing content, replaces the section between them.
/// Otherwise, appends the new section at the end.
pub fn merge_with_markers(
    existing: &str,
    generated: &str,
    start_marker: &str,
    end_marker: &str,
) -> String {
    if let (Some(start_pos), Some(end_pos)) = (
        existing.find(start_marker),
        existing.find(end_marker),
    ) {
        if start_pos < end_pos {
            // Replace existing ACP section
            let before = &existing[..start_pos];
            let after = &existing[end_pos + end_marker.len()..];

            format!(
                "{}{}\n{}\n{}{}",
                before.trim_end(),
                if before.trim_end().is_empty() { "" } else { "\n\n" },
                wrap_with_markers(generated, start_marker, end_marker),
                if after.trim_start().is_empty() { "" } else { "\n" },
                after.trim_start()
            )
        } else {
            // Malformed markers - append new section
            append_section(existing, generated, start_marker, end_marker)
        }
    } else {
        // No existing markers - append new section
        append_section(existing, generated, start_marker, end_marker)
    }
}

/// Wrap content with section markers
fn wrap_with_markers(content: &str, start_marker: &str, end_marker: &str) -> String {
    format!("{}\n{}\n{}", start_marker, content, end_marker)
}

/// Append a new section to existing content
fn append_section(
    existing: &str,
    generated: &str,
    start_marker: &str,
    end_marker: &str,
) -> String {
    let trimmed = existing.trim_end();
    if trimmed.is_empty() {
        wrap_with_markers(generated, start_marker, end_marker)
    } else {
        format!(
            "{}\n\n{}",
            trimmed,
            wrap_with_markers(generated, start_marker, end_marker)
        )
    }
}

/// Merge JSON content by updating specific keys
pub fn merge_json(existing: &str, generated: &str) -> Result<String, serde_json::Error> {
    let mut existing_json: serde_json::Value = serde_json::from_str(existing)?;
    let generated_json: serde_json::Value = serde_json::from_str(generated)?;

    // Merge generated keys into existing
    if let (Some(existing_obj), Some(generated_obj)) = (
        existing_json.as_object_mut(),
        generated_json.as_object(),
    ) {
        for (key, value) in generated_obj {
            existing_obj.insert(key.clone(), value.clone());
        }
    }

    serde_json::to_string_pretty(&existing_json)
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = "<!-- BEGIN ACP -->";
    const END: &str = "<!-- END ACP -->";

    #[test]
    fn test_merge_with_markers_new_file() {
        let result = merge_with_markers("", "New content", START, END);
        assert!(result.contains(START));
        assert!(result.contains("New content"));
        assert!(result.contains(END));
    }

    #[test]
    fn test_merge_with_markers_append() {
        let existing = "# My Project\n\nSome user content.";
        let result = merge_with_markers(existing, "ACP content", START, END);

        assert!(result.starts_with("# My Project"));
        assert!(result.contains("Some user content"));
        assert!(result.contains(START));
        assert!(result.contains("ACP content"));
        assert!(result.contains(END));
    }

    #[test]
    fn test_merge_with_markers_replace_existing() {
        let existing = format!(
            "# Header\n\n{}\nOld ACP content\n{}\n\n# Footer",
            START, END
        );
        let result = merge_with_markers(&existing, "New ACP content", START, END);

        assert!(result.contains("# Header"));
        assert!(result.contains("# Footer"));
        assert!(result.contains("New ACP content"));
        assert!(!result.contains("Old ACP content"));
    }

    #[test]
    fn test_merge_json() {
        let existing = r#"{"name": "test", "version": "1.0"}"#;
        let generated = r#"{"systemMessage": "Hello", "_acp": {"version": "1.0"}}"#;

        let result = merge_json(existing, generated).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["name"], "test");
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["systemMessage"], "Hello");
        assert!(parsed.get("_acp").is_some());
    }

    #[test]
    fn test_merge_strategy_replace() {
        let result = merge_content(
            MergeStrategy::Replace,
            "Old content",
            "New content",
            START,
            END,
        );
        assert_eq!(result, "New content");
    }

    #[test]
    fn test_merge_strategy_append() {
        let result = merge_content(
            MergeStrategy::Append,
            "Existing",
            "Appended",
            START,
            END,
        );
        assert!(result.contains("Existing"));
        assert!(result.contains("Appended"));
        assert!(result.find("Existing").unwrap() < result.find("Appended").unwrap());
    }
}
