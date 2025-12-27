//! @acp:module "Documentation System Bridging"
//! @acp:summary "RFC-0006: Bridges native documentation formats (JSDoc, docstrings, etc.) to ACP"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Documentation System Bridging
//!
//! This module implements RFC-0006 Documentation System Bridging, enabling
//! ACP to leverage existing documentation from native formats:
//!
//! - JSDoc/TSDoc (JavaScript/TypeScript)
//! - Python docstrings (Google, NumPy, Sphinx)
//! - Rust doc comments
//! - Go doc comments
//! - Javadoc
//!
//! ## Core Components
//!
//! - [`BridgeConfig`]: Configuration for bridging behavior
//! - [`FormatDetector`]: Auto-detects documentation format
//! - [`BridgeMerger`]: Merges native docs with ACP annotations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use acp::bridge::{BridgeConfig, FormatDetector, BridgeMerger};
//!
//! let config = BridgeConfig::from_file(".acp.config.json")?;
//! let detector = FormatDetector::new(&config);
//! let merger = BridgeMerger::new(&config);
//!
//! // Detect format from content
//! let format = detector.detect(content, language);
//!
//! // Merge native docs with ACP annotations
//! let merged = merger.merge(native_docs, acp_annotations)?;
//! ```

pub mod config;
pub mod detector;
pub mod merger;

pub use config::{BridgeConfig, JsDocConfig, PythonConfig, RustConfig, ProvenanceConfig};
pub use detector::FormatDetector;
pub use merger::BridgeMerger;

use crate::cache::{BridgeSource, SourceFormat, ParamEntry, ReturnsEntry, ThrowsEntry};
use crate::annotate::converters::ParsedDocumentation;

/// @acp:summary "Result of bridging native documentation with ACP annotations"
#[derive(Debug, Clone, Default)]
pub struct BridgeResult {
    /// Merged summary/description
    pub summary: Option<String>,
    /// AI behavioral directive for the function/method
    pub directive: Option<String>,
    /// Parameter entries with provenance
    pub params: Vec<ParamEntry>,
    /// Returns entry with provenance
    pub returns: Option<ReturnsEntry>,
    /// Throws entries with provenance
    pub throws: Vec<ThrowsEntry>,
    /// Examples extracted from documentation
    pub examples: Vec<String>,
    /// Overall source of the merged documentation
    pub source: BridgeSource,
    /// Source formats that contributed to this result
    pub source_formats: Vec<SourceFormat>,
}

impl BridgeResult {
    /// @acp:summary "Create a new empty bridge result"
    pub fn new() -> Self {
        Self::default()
    }

    /// @acp:summary "Create from pure ACP annotations (explicit source)"
    pub fn from_acp(
        summary: Option<String>,
        directive: Option<String>,
    ) -> Self {
        Self {
            summary,
            directive,
            source: BridgeSource::Explicit,
            source_formats: vec![SourceFormat::Acp],
            ..Default::default()
        }
    }

    /// @acp:summary "Create from converted native documentation"
    pub fn from_native(
        parsed: &ParsedDocumentation,
        format: SourceFormat,
    ) -> Self {
        let mut result = Self {
            summary: parsed.summary.clone(),
            source: BridgeSource::Converted,
            source_formats: vec![format],
            examples: parsed.examples.clone(),
            ..Default::default()
        };

        // Convert params
        for (name, type_str, desc) in &parsed.params {
            result.params.push(ParamEntry {
                name: name.clone(),
                r#type: type_str.clone(),
                type_source: type_source_from_format(format),
                description: desc.clone(),
                directive: None,
                optional: false,
                default: None,
                source: BridgeSource::Converted,
                source_format: Some(format),
                source_formats: vec![],
            });
        }

        // Convert returns
        if let Some((type_str, desc)) = &parsed.returns {
            result.returns = Some(ReturnsEntry {
                r#type: type_str.clone(),
                type_source: type_source_from_format(format),
                description: desc.clone(),
                directive: None,
                source: BridgeSource::Converted,
                source_format: Some(format),
                source_formats: vec![],
            });
        }

        // Convert throws
        for (exc_type, desc) in &parsed.throws {
            result.throws.push(ThrowsEntry {
                exception: exc_type.clone(),
                description: desc.clone(),
                directive: None,
                source: BridgeSource::Converted,
                source_format: Some(format),
            });
        }

        result
    }
}

/// @acp:summary "Determine TypeSource from SourceFormat"
fn type_source_from_format(format: SourceFormat) -> Option<crate::cache::TypeSource> {
    use crate::cache::TypeSource;
    match format {
        SourceFormat::Jsdoc => Some(TypeSource::Jsdoc),
        SourceFormat::DocstringGoogle | SourceFormat::DocstringNumpy | SourceFormat::DocstringSphinx => {
            Some(TypeSource::Docstring)
        }
        SourceFormat::Rustdoc => Some(TypeSource::Rustdoc),
        SourceFormat::Javadoc => Some(TypeSource::Javadoc),
        SourceFormat::TypeHint => Some(TypeSource::TypeHint),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_result_from_acp() {
        let result = BridgeResult::from_acp(
            Some("Test summary".to_string()),
            Some("MUST validate input".to_string()),
        );
        assert_eq!(result.summary, Some("Test summary".to_string()));
        assert_eq!(result.directive, Some("MUST validate input".to_string()));
        assert_eq!(result.source, BridgeSource::Explicit);
        assert_eq!(result.source_formats, vec![SourceFormat::Acp]);
    }

    #[test]
    fn test_bridge_result_from_native() {
        let mut parsed = ParsedDocumentation::new();
        parsed.summary = Some("Native summary".to_string());
        parsed.params.push(("userId".to_string(), Some("string".to_string()), Some("User ID".to_string())));
        parsed.returns = Some((Some("User".to_string()), Some("The user object".to_string())));

        let result = BridgeResult::from_native(&parsed, SourceFormat::Jsdoc);

        assert_eq!(result.summary, Some("Native summary".to_string()));
        assert_eq!(result.source, BridgeSource::Converted);
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0].name, "userId");
        assert!(result.returns.is_some());
    }
}
