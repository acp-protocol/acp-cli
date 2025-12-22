//! @acp:module "Language Extractors"
//! @acp:summary "Language-specific symbol extraction implementations"
//! @acp:domain cli
//! @acp:layer parsing

pub mod typescript;
pub mod javascript;
pub mod rust;
pub mod python;
pub mod go;
pub mod java;

use tree_sitter::{Language, Tree, Node};
use crate::error::Result;
use super::{ExtractedSymbol, Import, FunctionCall};

/// Trait for language-specific symbol extraction
pub trait LanguageExtractor: Send + Sync {
    /// Get the tree-sitter language
    fn language(&self) -> Language;

    /// Get the language name
    fn name(&self) -> &'static str;

    /// Get file extensions for this language
    fn extensions(&self) -> &'static [&'static str];

    /// Extract symbols from a parsed AST
    fn extract_symbols(&self, tree: &Tree, source: &str) -> Result<Vec<ExtractedSymbol>>;

    /// Extract import statements from a parsed AST
    fn extract_imports(&self, tree: &Tree, source: &str) -> Result<Vec<Import>>;

    /// Extract function calls from a parsed AST
    fn extract_calls(&self, tree: &Tree, source: &str, current_function: Option<&str>) -> Result<Vec<FunctionCall>>;

    /// Extract doc comment for a node (language-specific comment syntax)
    fn extract_doc_comment(&self, node: &Node, source: &str) -> Option<String>;
}

/// Get text for a node from source
pub fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

/// Get an extractor for a language name
pub fn get_extractor(language: &str) -> Option<Box<dyn LanguageExtractor>> {
    match language.to_lowercase().as_str() {
        "typescript" | "ts" => Some(Box::new(typescript::TypeScriptExtractor)),
        "javascript" | "js" => Some(Box::new(javascript::JavaScriptExtractor)),
        "rust" | "rs" => Some(Box::new(rust::RustExtractor)),
        "python" | "py" => Some(Box::new(python::PythonExtractor)),
        "go" => Some(Box::new(go::GoExtractor)),
        "java" => Some(Box::new(java::JavaExtractor)),
        _ => None,
    }
}

/// Get an extractor by file extension
pub fn extractor_for_extension(ext: &str) -> Option<Box<dyn LanguageExtractor>> {
    match ext.to_lowercase().as_str() {
        "ts" | "tsx" => Some(Box::new(typescript::TypeScriptExtractor)),
        "js" | "jsx" | "mjs" | "cjs" => Some(Box::new(javascript::JavaScriptExtractor)),
        "rs" => Some(Box::new(rust::RustExtractor)),
        "py" | "pyi" => Some(Box::new(python::PythonExtractor)),
        "go" => Some(Box::new(go::GoExtractor)),
        "java" => Some(Box::new(java::JavaExtractor)),
        _ => None,
    }
}
