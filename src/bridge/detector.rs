//! @acp:module "Format Detector"
//! @acp:summary "RFC-0006: Auto-detects documentation format from content"
//! @acp:domain cli
//! @acp:layer service

use super::config::{BridgeConfig, DocstringStyle};
use crate::cache::SourceFormat;
use regex::Regex;

/// @acp:summary "Detects documentation format from content"
pub struct FormatDetector {
    config: BridgeConfig,
    // Compiled regexes for detection
    numpy_pattern: Regex,
    sphinx_pattern: Regex,
    google_pattern: Regex,
    jsdoc_pattern: Regex,
    rustdoc_section_pattern: Regex,
}

impl FormatDetector {
    /// @acp:summary "Create a new format detector with configuration"
    pub fn new(config: &BridgeConfig) -> Self {
        Self {
            config: config.clone(),
            // NumPy: Section headers with underlines
            numpy_pattern: Regex::new(r"(?m)^\s*(Parameters|Returns|Raises|Yields|Examples?|Notes?|Attributes?)\s*\n\s*-{3,}").unwrap(),
            // Sphinx: :param:, :returns:, :raises: tags
            sphinx_pattern: Regex::new(r":(param|returns?|raises?|type|rtype)\s+").unwrap(),
            // Google: Args:, Returns:, Raises: sections
            google_pattern: Regex::new(r"(?m)^\s*(Args|Arguments|Parameters|Returns|Raises|Yields|Examples?|Attributes?):\s*$").unwrap(),
            // JSDoc: @param, @returns, etc.
            jsdoc_pattern: Regex::new(r"@(param|returns?|throws?|deprecated|example|see)\b").unwrap(),
            // Rustdoc: # Arguments, # Returns, etc.
            rustdoc_section_pattern: Regex::new(r"(?m)^#\s*(Arguments?|Returns?|Panics?|Errors?|Examples?|Safety)\s*$").unwrap(),
        }
    }

    /// @acp:summary "Detect documentation format from content and language"
    pub fn detect(&self, content: &str, language: &str) -> Option<SourceFormat> {
        if !self.config.enabled {
            return None;
        }

        match language.to_lowercase().as_str() {
            "javascript" | "typescript" | "js" | "ts" => {
                if self.config.jsdoc.enabled {
                    self.detect_jsdoc(content)
                } else {
                    None
                }
            }
            "python" | "py" => {
                if self.config.python.enabled {
                    self.detect_python_docstring(content)
                } else {
                    None
                }
            }
            "rust" | "rs" => {
                if self.config.rust.enabled {
                    self.detect_rustdoc(content)
                } else {
                    None
                }
            }
            "java" | "kotlin" => Some(SourceFormat::Javadoc),
            "go" => Some(SourceFormat::Godoc),
            _ => None,
        }
    }

    /// @acp:summary "Detect JSDoc format"
    fn detect_jsdoc(&self, content: &str) -> Option<SourceFormat> {
        if self.jsdoc_pattern.is_match(content) {
            Some(SourceFormat::Jsdoc)
        } else {
            None
        }
    }

    /// @acp:summary "Detect Python docstring style"
    pub fn detect_python_docstring(&self, content: &str) -> Option<SourceFormat> {
        // Check explicit configuration first
        match self.config.python.docstring_style {
            DocstringStyle::Google => return Some(SourceFormat::DocstringGoogle),
            DocstringStyle::Numpy => return Some(SourceFormat::DocstringNumpy),
            DocstringStyle::Sphinx => return Some(SourceFormat::DocstringSphinx),
            DocstringStyle::Auto => {} // Continue to auto-detection
        }

        // Auto-detect from content patterns
        self.auto_detect_docstring_style(content)
    }

    /// @acp:summary "Auto-detect docstring style from content patterns"
    fn auto_detect_docstring_style(&self, content: &str) -> Option<SourceFormat> {
        // Priority order: NumPy (most distinctive), Sphinx, Google

        // NumPy: Section headers with underlines (most distinctive)
        if self.numpy_pattern.is_match(content) {
            return Some(SourceFormat::DocstringNumpy);
        }

        // Sphinx: :param:, :returns:, :raises: tags
        if self.sphinx_pattern.is_match(content) {
            return Some(SourceFormat::DocstringSphinx);
        }

        // Google: Args:, Returns:, Raises: sections
        if self.google_pattern.is_match(content) {
            return Some(SourceFormat::DocstringGoogle);
        }

        // No recognizable format found - might be plain docstring
        None
    }

    /// @acp:summary "Detect Rust doc format"
    fn detect_rustdoc(&self, content: &str) -> Option<SourceFormat> {
        // Rust doc comments with sections
        if self.rustdoc_section_pattern.is_match(content) {
            return Some(SourceFormat::Rustdoc);
        }

        // Any /// or //! comment is considered rustdoc
        if content.contains("///") || content.contains("//!") {
            return Some(SourceFormat::Rustdoc);
        }

        None
    }

    /// @acp:summary "Check if content has any documentation comments"
    pub fn has_documentation(&self, content: &str, language: &str) -> bool {
        match language.to_lowercase().as_str() {
            "javascript" | "typescript" | "js" | "ts" => {
                content.contains("/**")
                    || content.contains("@param")
                    || content.contains("@returns")
            }
            "python" | "py" => content.contains("\"\"\"") || content.contains("'''"),
            "rust" | "rs" => content.contains("///") || content.contains("//!"),
            "java" | "kotlin" => content.contains("/**"),
            "go" => {
                // Go doc comments are // directly before declaration
                content.lines().any(|line| {
                    let trimmed = line.trim();
                    trimmed.starts_with("//") && !trimmed.starts_with("// +build")
                })
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> BridgeConfig {
        BridgeConfig::enabled()
    }

    #[test]
    fn test_detect_jsdoc() {
        let detector = FormatDetector::new(&enabled_config());

        let jsdoc = r#"
            /**
             * @param {string} name - The name
             * @returns {User} The user
             */
        "#;
        assert_eq!(
            detector.detect(jsdoc, "typescript"),
            Some(SourceFormat::Jsdoc)
        );
    }

    #[test]
    fn test_detect_google_docstring() {
        let detector = FormatDetector::new(&enabled_config());

        let google = r#"
            """Search for users.

            Args:
                query: Search query string.
                limit: Maximum results.

            Returns:
                List of matching users.
            """
        "#;
        assert_eq!(
            detector.detect(google, "python"),
            Some(SourceFormat::DocstringGoogle)
        );
    }

    #[test]
    fn test_detect_numpy_docstring() {
        let detector = FormatDetector::new(&enabled_config());

        let numpy = r#"
            """Search for users.

            Parameters
            ----------
            query : str
                Search query string.
            limit : int, optional
                Maximum results.

            Returns
            -------
            list
                List of matching users.
            """
        "#;
        assert_eq!(
            detector.detect(numpy, "python"),
            Some(SourceFormat::DocstringNumpy)
        );
    }

    #[test]
    fn test_detect_sphinx_docstring() {
        let detector = FormatDetector::new(&enabled_config());

        let sphinx = r#"
            """Search for users.

            :param query: Search query string.
            :type query: str
            :param limit: Maximum results.
            :returns: List of matching users.
            :rtype: list
            """
        "#;
        assert_eq!(
            detector.detect(sphinx, "python"),
            Some(SourceFormat::DocstringSphinx)
        );
    }

    #[test]
    fn test_detect_rustdoc() {
        let detector = FormatDetector::new(&enabled_config());

        let rustdoc = r#"
            /// Search for users in the database.
            ///
            /// # Arguments
            ///
            /// * `query` - Search query string
            /// * `limit` - Maximum results
            ///
            /// # Returns
            ///
            /// A vector of matching users.
        "#;
        assert_eq!(
            detector.detect(rustdoc, "rust"),
            Some(SourceFormat::Rustdoc)
        );
    }

    #[test]
    fn test_detect_disabled() {
        let config = BridgeConfig::new(); // disabled by default
        let detector = FormatDetector::new(&config);

        let jsdoc = "@param {string} name - The name";
        assert_eq!(detector.detect(jsdoc, "typescript"), None);
    }

    #[test]
    fn test_detect_language_disabled() {
        let mut config = BridgeConfig::enabled();
        config.python.enabled = false;
        let detector = FormatDetector::new(&config);

        let google = "Args:\n    query: Search query.";
        assert_eq!(detector.detect(google, "python"), None);
    }

    #[test]
    fn test_has_documentation() {
        let detector = FormatDetector::new(&enabled_config());

        assert!(detector.has_documentation("/** @param x */", "typescript"));
        assert!(detector.has_documentation("'''docstring'''", "python"));
        assert!(detector.has_documentation("/// doc comment", "rust"));
        assert!(!detector.has_documentation("// regular comment", "typescript"));
    }
}
