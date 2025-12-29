//! @acp:module "Bridge Merger"
//! @acp:summary "RFC-0006: Merges native documentation with ACP annotations"
//! @acp:domain cli
//! @acp:layer service

use crate::cache::{BridgeSource, SourceFormat, ParamEntry, ReturnsEntry, ThrowsEntry};
use crate::annotate::converters::ParsedDocumentation;
use super::config::{BridgeConfig, Precedence};
use super::BridgeResult;

/// @acp:summary "Parsed ACP annotations for a symbol"
#[derive(Debug, Clone, Default)]
pub struct AcpAnnotations {
    /// Summary from @acp:summary or @acp:fn
    pub summary: Option<String>,
    /// Directive from @acp:fn or @acp:method
    pub directive: Option<String>,
    /// Parameter directives: (name, directive)
    pub params: Vec<(String, String)>,
    /// Returns directive
    pub returns: Option<String>,
    /// Throws entries: (exception, directive)
    pub throws: Vec<(String, String)>,
}

/// @acp:summary "Merges native documentation with ACP annotations"
pub struct BridgeMerger {
    config: BridgeConfig,
}

impl BridgeMerger {
    /// @acp:summary "Create a new merger with configuration"
    pub fn new(config: &BridgeConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// @acp:summary "Merge native documentation with ACP annotations"
    pub fn merge(
        &self,
        native: Option<&ParsedDocumentation>,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> BridgeResult {
        // If no native docs, return ACP-only result
        if native.is_none() || !self.config.enabled {
            return self.build_acp_only(acp);
        }

        let native = native.unwrap();

        // If native docs are empty, return ACP-only result
        if native.is_empty() {
            return self.build_acp_only(acp);
        }

        // If no ACP annotations, return native-only result
        if self.is_acp_empty(acp) {
            return BridgeResult::from_native(native, native_format);
        }

        // Merge based on precedence mode
        match self.config.precedence {
            Precedence::AcpFirst => self.merge_acp_first(native, native_format, acp),
            Precedence::NativeFirst => self.merge_native_first(native, native_format, acp),
            Precedence::Merge => self.merge_combined(native, native_format, acp),
        }
    }

    /// @acp:summary "Create result from ACP annotations only"
    fn build_acp_only(&self, acp: &AcpAnnotations) -> BridgeResult {
        let mut result = BridgeResult {
            summary: acp.summary.clone(),
            directive: acp.directive.clone(),
            source: BridgeSource::Explicit,
            source_formats: vec![SourceFormat::Acp],
            ..Default::default()
        };

        // Convert ACP params
        for (name, directive) in &acp.params {
            result.params.push(ParamEntry {
                name: name.clone(),
                r#type: None,
                type_source: None,
                description: None,
                directive: Some(directive.clone()),
                optional: false,
                default: None,
                source: BridgeSource::Explicit,
                source_format: Some(SourceFormat::Acp),
                source_formats: vec![],
            });
        }

        // Convert ACP returns
        if let Some(directive) = &acp.returns {
            result.returns = Some(ReturnsEntry {
                r#type: None,
                type_source: None,
                description: None,
                directive: Some(directive.clone()),
                source: BridgeSource::Explicit,
                source_format: Some(SourceFormat::Acp),
                source_formats: vec![],
            });
        }

        // Convert ACP throws
        for (exception, directive) in &acp.throws {
            result.throws.push(ThrowsEntry {
                exception: exception.clone(),
                description: None,
                directive: Some(directive.clone()),
                source: BridgeSource::Explicit,
                source_format: Some(SourceFormat::Acp),
            });
        }

        result
    }

    /// @acp:summary "Check if ACP annotations are empty"
    fn is_acp_empty(&self, acp: &AcpAnnotations) -> bool {
        acp.summary.is_none()
            && acp.directive.is_none()
            && acp.params.is_empty()
            && acp.returns.is_none()
            && acp.throws.is_empty()
    }

    /// @acp:summary "Merge with ACP taking precedence"
    /// ACP annotations win; native docs fill gaps.
    fn merge_acp_first(
        &self,
        native: &ParsedDocumentation,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> BridgeResult {
        let mut result = BridgeResult {
            // Summary: prefer native (as per spec 15.3.1)
            summary: native.summary.clone().or_else(|| acp.summary.clone()),
            // Directive: use ACP
            directive: acp.directive.clone(),
            source: BridgeSource::Merged,
            source_formats: vec![native_format, SourceFormat::Acp],
            examples: native.examples.clone(),
            ..Default::default()
        };

        // Merge params: native descriptions + ACP directives
        result.params = self.merge_params(native, native_format, acp);

        // Merge returns
        result.returns = self.merge_returns(native, native_format, acp);

        // Merge throws
        result.throws = self.merge_throws(native, native_format, acp);

        result
    }

    /// @acp:summary "Merge with native taking precedence"
    /// Native docs are authoritative; ACP adds directives only.
    fn merge_native_first(
        &self,
        native: &ParsedDocumentation,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> BridgeResult {
        let mut result = BridgeResult::from_native(native, native_format);

        // Layer on ACP directives only
        result.directive = acp.directive.clone();
        result.source = BridgeSource::Merged;
        result.source_formats = vec![native_format, SourceFormat::Acp];

        // Add directives to params
        for param in &mut result.params {
            if let Some((_, directive)) = acp.params.iter().find(|(n, _)| n == &param.name) {
                param.directive = Some(directive.clone());
                param.source = BridgeSource::Merged;
                param.source_formats = vec![native_format, SourceFormat::Acp];
            }
        }

        // Add directive to returns
        if let Some(returns) = &mut result.returns {
            if let Some(directive) = &acp.returns {
                returns.directive = Some(directive.clone());
                returns.source = BridgeSource::Merged;
                returns.source_formats = vec![native_format, SourceFormat::Acp];
            }
        }

        result
    }

    /// @acp:summary "Intelligently combine both sources"
    fn merge_combined(
        &self,
        native: &ParsedDocumentation,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> BridgeResult {
        // For merge mode, combine descriptions from both if they provide different info
        let summary = match (&native.summary, &acp.summary) {
            (Some(n), Some(a)) if n != a => Some(format!("{} {}", n, a)),
            (Some(n), _) => Some(n.clone()),
            (_, Some(a)) => Some(a.clone()),
            _ => None,
        };

        let mut result = BridgeResult {
            summary,
            directive: acp.directive.clone(),
            source: BridgeSource::Merged,
            source_formats: vec![native_format, SourceFormat::Acp],
            examples: native.examples.clone(),
            ..Default::default()
        };

        result.params = self.merge_params(native, native_format, acp);
        result.returns = self.merge_returns(native, native_format, acp);
        result.throws = self.merge_throws(native, native_format, acp);

        result
    }

    /// @acp:summary "Merge parameter entries"
    fn merge_params(
        &self,
        native: &ParsedDocumentation,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> Vec<ParamEntry> {
        let mut params = Vec::new();

        // Start with native params
        for (name, type_str, desc) in &native.params {
            let directive = acp.params.iter()
                .find(|(n, _)| n == name)
                .map(|(_, d)| d.clone());

            let source = if directive.is_some() {
                BridgeSource::Merged
            } else {
                BridgeSource::Converted
            };

            let source_formats = if directive.is_some() {
                vec![native_format, SourceFormat::Acp]
            } else {
                vec![]
            };

            params.push(ParamEntry {
                name: name.clone(),
                r#type: type_str.clone(),
                type_source: type_source_from_format(native_format),
                description: desc.clone(),
                directive,
                optional: false,
                default: None,
                source,
                source_format: if source_formats.is_empty() { Some(native_format) } else { None },
                source_formats,
            });
        }

        // Add ACP-only params (not in native)
        for (name, directive) in &acp.params {
            if !params.iter().any(|p| &p.name == name) {
                params.push(ParamEntry {
                    name: name.clone(),
                    r#type: None,
                    type_source: None,
                    description: None,
                    directive: Some(directive.clone()),
                    optional: false,
                    default: None,
                    source: BridgeSource::Explicit,
                    source_format: Some(SourceFormat::Acp),
                    source_formats: vec![],
                });
            }
        }

        params
    }

    /// @acp:summary "Merge returns entry"
    fn merge_returns(
        &self,
        native: &ParsedDocumentation,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> Option<ReturnsEntry> {
        match (&native.returns, &acp.returns) {
            (Some((type_str, desc)), Some(directive)) => {
                // Both exist - merge
                Some(ReturnsEntry {
                    r#type: type_str.clone(),
                    type_source: type_source_from_format(native_format),
                    description: desc.clone(),
                    directive: Some(directive.clone()),
                    source: BridgeSource::Merged,
                    source_format: None,
                    source_formats: vec![native_format, SourceFormat::Acp],
                })
            }
            (Some((type_str, desc)), None) => {
                // Native only
                Some(ReturnsEntry {
                    r#type: type_str.clone(),
                    type_source: type_source_from_format(native_format),
                    description: desc.clone(),
                    directive: None,
                    source: BridgeSource::Converted,
                    source_format: Some(native_format),
                    source_formats: vec![],
                })
            }
            (None, Some(directive)) => {
                // ACP only
                Some(ReturnsEntry {
                    r#type: None,
                    type_source: None,
                    description: None,
                    directive: Some(directive.clone()),
                    source: BridgeSource::Explicit,
                    source_format: Some(SourceFormat::Acp),
                    source_formats: vec![],
                })
            }
            (None, None) => None,
        }
    }

    /// @acp:summary "Merge throws entries"
    fn merge_throws(
        &self,
        native: &ParsedDocumentation,
        native_format: SourceFormat,
        acp: &AcpAnnotations,
    ) -> Vec<ThrowsEntry> {
        let mut throws = Vec::new();

        // Start with native throws
        for (exc_type, desc) in &native.throws {
            let directive = acp.throws.iter()
                .find(|(e, _)| e == exc_type)
                .map(|(_, d)| d.clone());

            let source = if directive.is_some() {
                BridgeSource::Merged
            } else {
                BridgeSource::Converted
            };

            throws.push(ThrowsEntry {
                exception: exc_type.clone(),
                description: desc.clone(),
                directive,
                source,
                source_format: Some(native_format),
            });
        }

        // Add ACP-only throws
        for (exception, directive) in &acp.throws {
            if !throws.iter().any(|t| &t.exception == exception) {
                throws.push(ThrowsEntry {
                    exception: exception.clone(),
                    description: None,
                    directive: Some(directive.clone()),
                    source: BridgeSource::Explicit,
                    source_format: Some(SourceFormat::Acp),
                });
            }
        }

        throws
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

    fn test_config() -> BridgeConfig {
        BridgeConfig::enabled()
    }

    #[test]
    fn test_merge_acp_only() {
        let merger = BridgeMerger::new(&test_config());
        let acp = AcpAnnotations {
            summary: Some("Test function".to_string()),
            directive: Some("MUST validate input".to_string()),
            params: vec![("userId".to_string(), "MUST be UUID".to_string())],
            returns: Some("MAY be null".to_string()),
            throws: vec![],
        };

        let result = merger.merge(None, SourceFormat::Acp, &acp);

        assert_eq!(result.summary, Some("Test function".to_string()));
        assert_eq!(result.directive, Some("MUST validate input".to_string()));
        assert_eq!(result.source, BridgeSource::Explicit);
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0].directive, Some("MUST be UUID".to_string()));
    }

    #[test]
    fn test_merge_native_only() {
        let merger = BridgeMerger::new(&test_config());
        let mut native = ParsedDocumentation::new();
        native.summary = Some("Native summary".to_string());
        native.params.push(("userId".to_string(), Some("string".to_string()), Some("User ID".to_string())));

        let acp = AcpAnnotations::default();
        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        assert_eq!(result.summary, Some("Native summary".to_string()));
        assert_eq!(result.source, BridgeSource::Converted);
        assert_eq!(result.params.len(), 1);
        assert!(result.params[0].directive.is_none());
    }

    #[test]
    fn test_merge_acp_first() {
        let config = BridgeConfig::enabled();
        let merger = BridgeMerger::new(&config);

        let mut native = ParsedDocumentation::new();
        native.summary = Some("Native summary".to_string());
        native.params.push(("userId".to_string(), Some("string".to_string()), Some("User ID".to_string())));
        native.returns = Some((Some("User".to_string()), Some("The user object".to_string())));

        let acp = AcpAnnotations {
            summary: Some("ACP summary".to_string()),
            directive: Some("MUST authenticate".to_string()),
            params: vec![("userId".to_string(), "MUST be UUID".to_string())],
            returns: Some("MAY be cached".to_string()),
            throws: vec![],
        };

        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        // Summary should be from native (per spec 15.3.1)
        assert_eq!(result.summary, Some("Native summary".to_string()));
        // Directive from ACP
        assert_eq!(result.directive, Some("MUST authenticate".to_string()));
        assert_eq!(result.source, BridgeSource::Merged);

        // Param should have native description + ACP directive
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0].description, Some("User ID".to_string()));
        assert_eq!(result.params[0].directive, Some("MUST be UUID".to_string()));
        assert_eq!(result.params[0].source, BridgeSource::Merged);

        // Returns should be merged
        let returns = result.returns.unwrap();
        assert_eq!(returns.description, Some("The user object".to_string()));
        assert_eq!(returns.directive, Some("MAY be cached".to_string()));
    }

    #[test]
    fn test_merge_native_first() {
        let mut config = BridgeConfig::enabled();
        config.precedence = Precedence::NativeFirst;
        let merger = BridgeMerger::new(&config);

        let mut native = ParsedDocumentation::new();
        native.summary = Some("Native summary".to_string());
        native.params.push(("userId".to_string(), Some("string".to_string()), Some("User ID".to_string())));

        let acp = AcpAnnotations {
            directive: Some("MUST authenticate".to_string()),
            params: vec![("userId".to_string(), "MUST be UUID".to_string())],
            ..Default::default()
        };

        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        // Should use native summary
        assert_eq!(result.summary, Some("Native summary".to_string()));
        // But layer ACP directive
        assert_eq!(result.directive, Some("MUST authenticate".to_string()));
        // Param should be merged
        assert_eq!(result.params[0].directive, Some("MUST be UUID".to_string()));
    }

    #[test]
    fn test_merge_disabled() {
        let config = BridgeConfig::new(); // disabled
        let merger = BridgeMerger::new(&config);

        let mut native = ParsedDocumentation::new();
        native.summary = Some("Native".to_string());

        let acp = AcpAnnotations {
            summary: Some("ACP".to_string()),
            ..Default::default()
        };

        let result = merger.merge(Some(&native), SourceFormat::Jsdoc, &acp);

        // Should only use ACP when bridging disabled
        assert_eq!(result.summary, Some("ACP".to_string()));
        assert_eq!(result.source, BridgeSource::Explicit);
    }
}
