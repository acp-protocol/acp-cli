//! @acp:module "AST Parser"
//! @acp:summary "Core tree-sitter parser for multi-language AST analysis"
//! @acp:domain cli
//! @acp:layer parsing

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tree_sitter::{Parser, Tree};
use crate::error::{AcpError, Result};
use super::{ExtractedSymbol, Import, FunctionCall};
use super::languages::{get_extractor, extractor_for_extension, LanguageExtractor};

/// Multi-language AST parser using tree-sitter
/// Thread-safe via interior mutability for parser caching.
pub struct AstParser {
    /// Cached parsers per language (behind mutex for thread safety)
    parsers: Mutex<HashMap<String, Parser>>,
}

impl AstParser {
    /// Create a new AST parser
    pub fn new() -> Result<Self> {
        Ok(Self {
            parsers: Mutex::new(HashMap::new()),
        })
    }

    /// Parse source code and extract symbols
    pub fn parse_and_extract(&self, source: &str, language: &str) -> Result<Vec<ExtractedSymbol>> {
        let extractor = get_extractor(language)
            .ok_or_else(|| AcpError::UnsupportedLanguage(language.to_string()))?;

        let tree = self.parse(source, extractor.as_ref())?;
        extractor.extract_symbols(&tree, source)
    }

    /// Parse source code by file extension
    pub fn parse_by_extension(&self, source: &str, ext: &str) -> Result<Vec<ExtractedSymbol>> {
        let extractor = extractor_for_extension(ext)
            .ok_or_else(|| AcpError::UnsupportedLanguage(format!(".{}", ext)))?;

        let tree = self.parse(source, extractor.as_ref())?;
        extractor.extract_symbols(&tree, source)
    }

    /// Parse a file and extract symbols (convenience method for indexer)
    pub fn parse_file(&self, path: &Path, source: &str) -> Result<Vec<ExtractedSymbol>> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| AcpError::UnsupportedLanguage("no extension".to_string()))?;
        self.parse_by_extension(source, ext)
    }

    /// Parse a file and extract function calls (convenience method for indexer)
    pub fn parse_calls(&self, path: &Path, source: &str) -> Result<Vec<FunctionCall>> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| AcpError::UnsupportedLanguage("no extension".to_string()))?;

        let extractor = extractor_for_extension(ext)
            .ok_or_else(|| AcpError::UnsupportedLanguage(format!(".{}", ext)))?;

        let tree = self.parse(source, extractor.as_ref())?;
        extractor.extract_calls(&tree, source, None)
    }

    /// Extract imports from source code
    pub fn extract_imports(&self, source: &str, language: &str) -> Result<Vec<Import>> {
        let extractor = get_extractor(language)
            .ok_or_else(|| AcpError::UnsupportedLanguage(language.to_string()))?;

        let tree = self.parse(source, extractor.as_ref())?;
        extractor.extract_imports(&tree, source)
    }

    /// Extract function calls from source code
    pub fn extract_calls_by_language(
        &self,
        source: &str,
        language: &str,
        current_function: Option<&str>,
    ) -> Result<Vec<FunctionCall>> {
        let extractor = get_extractor(language)
            .ok_or_else(|| AcpError::UnsupportedLanguage(language.to_string()))?;

        let tree = self.parse(source, extractor.as_ref())?;
        extractor.extract_calls(&tree, source, current_function)
    }

    /// Parse source code into a tree-sitter Tree
    fn parse(&self, source: &str, extractor: &dyn LanguageExtractor) -> Result<Tree> {
        let lang_name = extractor.name().to_string();

        // Lock the mutex to access/modify parsers
        let mut parsers = self.parsers.lock()
            .map_err(|_| AcpError::parse("Parser lock poisoned".to_string()))?;

        // Get or create parser for this language
        let parser = parsers.entry(lang_name.clone()).or_insert_with(|| {
            let mut p = Parser::new();
            p.set_language(&extractor.language()).expect("Failed to set language");
            p
        });

        parser.parse(source, None)
            .ok_or_else(|| AcpError::parse(format!("Failed to parse {} source", lang_name)))
    }

    /// Get supported languages
    pub fn supported_languages() -> &'static [&'static str] {
        &["typescript", "javascript", "rust", "python", "go", "java"]
    }

    /// Get supported file extensions
    pub fn supported_extensions() -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx", "mjs", "cjs", "rs", "py", "pyi", "go", "java"]
    }

    /// Check if a language is supported
    pub fn is_language_supported(language: &str) -> bool {
        get_extractor(language).is_some()
    }

    /// Check if a file extension is supported
    pub fn is_extension_supported(ext: &str) -> bool {
        extractor_for_extension(ext).is_some()
    }
}

impl Default for AstParser {
    fn default() -> Self {
        Self::new().expect("Failed to create AST parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_languages() {
        let langs = AstParser::supported_languages();
        assert!(langs.contains(&"typescript"));
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"python"));
    }

    #[test]
    fn test_is_language_supported() {
        assert!(AstParser::is_language_supported("typescript"));
        assert!(AstParser::is_language_supported("rust"));
        assert!(AstParser::is_language_supported("python"));
        assert!(!AstParser::is_language_supported("cobol"));
    }

    #[test]
    fn test_is_extension_supported() {
        assert!(AstParser::is_extension_supported("ts"));
        assert!(AstParser::is_extension_supported("rs"));
        assert!(AstParser::is_extension_supported("py"));
        assert!(!AstParser::is_extension_supported("cob"));
    }
}
