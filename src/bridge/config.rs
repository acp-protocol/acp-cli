//! @acp:module "Bridge Configuration"
//! @acp:summary "RFC-0006: Configuration types for documentation bridging"
//! @acp:domain cli
//! @acp:layer model

use serde::{Deserialize, Serialize};

/// @acp:summary "Precedence mode for merging native docs with ACP"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Precedence {
    /// ACP annotations take precedence; native docs fill gaps
    #[default]
    AcpFirst,
    /// Native docs are authoritative; ACP adds directives only
    NativeFirst,
    /// Intelligently combine both sources
    Merge,
}

impl std::fmt::Display for Precedence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Precedence::AcpFirst => write!(f, "acp-first"),
            Precedence::NativeFirst => write!(f, "native-first"),
            Precedence::Merge => write!(f, "merge"),
        }
    }
}

/// @acp:summary "Strictness mode for parsing native documentation"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strictness {
    /// Best-effort extraction; skip malformed documentation
    #[default]
    Permissive,
    /// Reject and warn on malformed documentation
    Strict,
}

/// @acp:summary "Python docstring style"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocstringStyle {
    /// Auto-detect from content
    #[default]
    Auto,
    /// Google-style docstrings
    Google,
    /// NumPy-style docstrings
    Numpy,
    /// Sphinx/reST-style docstrings
    Sphinx,
}

/// @acp:summary "JSDoc/TSDoc configuration"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsDocConfig {
    /// Whether JSDoc bridging is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Extract types from @param {Type} annotations
    #[serde(default = "default_true")]
    pub extract_types: bool,
    /// Tags to convert to ACP annotations
    #[serde(default = "default_jsdoc_tags")]
    pub convert_tags: Vec<String>,
}

impl Default for JsDocConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extract_types: true,
            convert_tags: default_jsdoc_tags(),
        }
    }
}

fn default_jsdoc_tags() -> Vec<String> {
    vec![
        "param".to_string(),
        "returns".to_string(),
        "throws".to_string(),
        "deprecated".to_string(),
        "example".to_string(),
        "see".to_string(),
    ]
}

/// @acp:summary "Python docstring configuration"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonConfig {
    /// Whether Python docstring bridging is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Docstring style to use (or auto-detect)
    #[serde(default)]
    pub docstring_style: DocstringStyle,
    /// Extract types from Python type hints
    #[serde(default = "default_true")]
    pub extract_type_hints: bool,
    /// Sections to convert to ACP annotations
    #[serde(default = "default_python_sections")]
    pub convert_sections: Vec<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            docstring_style: DocstringStyle::Auto,
            extract_type_hints: true,
            convert_sections: default_python_sections(),
        }
    }
}

fn default_python_sections() -> Vec<String> {
    vec![
        "Args".to_string(),
        "Parameters".to_string(),
        "Returns".to_string(),
        "Raises".to_string(),
        "Example".to_string(),
        "Yields".to_string(),
    ]
}

/// @acp:summary "Rust doc comment configuration"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustConfig {
    /// Whether Rust doc bridging is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Sections to convert to ACP annotations
    #[serde(default = "default_rust_sections")]
    pub convert_sections: Vec<String>,
}

impl Default for RustConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            convert_sections: default_rust_sections(),
        }
    }
}

fn default_rust_sections() -> Vec<String> {
    vec![
        "Arguments".to_string(),
        "Returns".to_string(),
        "Panics".to_string(),
        "Errors".to_string(),
        "Examples".to_string(),
        "Safety".to_string(),
    ]
}

/// @acp:summary "Provenance tracking configuration"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceConfig {
    /// Mark converted annotations with source information
    #[serde(default = "default_true")]
    pub mark_converted: bool,
    /// Include source format in provenance
    #[serde(default = "default_true")]
    pub include_source_format: bool,
}

impl Default for ProvenanceConfig {
    fn default() -> Self {
        Self {
            mark_converted: true,
            include_source_format: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// @acp:summary "RFC-0006: Documentation bridging configuration"
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeConfig {
    /// Enable documentation bridging (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Precedence mode when both native and ACP exist
    #[serde(default)]
    pub precedence: Precedence,
    /// How to handle malformed documentation
    #[serde(default)]
    pub strictness: Strictness,
    /// JSDoc/TSDoc settings
    #[serde(default)]
    pub jsdoc: JsDocConfig,
    /// Python docstring settings
    #[serde(default)]
    pub python: PythonConfig,
    /// Rust doc comment settings
    #[serde(default)]
    pub rust: RustConfig,
    /// Provenance tracking settings
    #[serde(default)]
    pub provenance: ProvenanceConfig,
}

impl BridgeConfig {
    /// @acp:summary "Create a new default configuration (bridging disabled)"
    pub fn new() -> Self {
        Self::default()
    }

    /// @acp:summary "Create configuration with bridging enabled"
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// @acp:summary "Check if bridging is enabled for a specific language"
    pub fn is_enabled_for(&self, language: &str) -> bool {
        if !self.enabled {
            return false;
        }
        match language.to_lowercase().as_str() {
            "javascript" | "typescript" | "js" | "ts" => self.jsdoc.enabled,
            "python" | "py" => self.python.enabled,
            "rust" | "rs" => self.rust.enabled,
            "java" | "kotlin" => true, // Javadoc always enabled if bridging is on
            "go" => true,              // Godoc always enabled if bridging is on
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_config_defaults() {
        let config = BridgeConfig::new();
        assert!(!config.enabled);
        assert_eq!(config.precedence, Precedence::AcpFirst);
        assert_eq!(config.strictness, Strictness::Permissive);
        assert!(config.jsdoc.enabled);
        assert!(config.python.enabled);
        assert!(config.rust.enabled);
    }

    #[test]
    fn test_bridge_config_enabled() {
        let config = BridgeConfig::enabled();
        assert!(config.enabled);
        assert!(config.is_enabled_for("typescript"));
        assert!(config.is_enabled_for("python"));
        assert!(config.is_enabled_for("rust"));
    }

    #[test]
    fn test_is_enabled_for_disabled_global() {
        let config = BridgeConfig::new();
        assert!(!config.is_enabled_for("typescript"));
        assert!(!config.is_enabled_for("python"));
    }

    #[test]
    fn test_is_enabled_for_specific_disabled() {
        let mut config = BridgeConfig::enabled();
        config.python.enabled = false;

        assert!(config.is_enabled_for("typescript"));
        assert!(!config.is_enabled_for("python"));
    }

    #[test]
    fn test_precedence_display() {
        assert_eq!(Precedence::AcpFirst.to_string(), "acp-first");
        assert_eq!(Precedence::NativeFirst.to_string(), "native-first");
        assert_eq!(Precedence::Merge.to_string(), "merge");
    }

    #[test]
    fn test_config_serialization() {
        let config = BridgeConfig::enabled();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: BridgeConfig = serde_json::from_str(&json).unwrap();

        assert!(parsed.enabled);
        assert_eq!(parsed.precedence, Precedence::AcpFirst);
    }
}
