//! @acp:module "Variables"
//! @acp:summary "Variable system for token-efficient macros (schema-compliant)"
//! @acp:domain cli
//! @acp:layer model
//! @acp:stability stable

mod resolver;
mod expander;

pub mod presets;

pub use resolver::{VarResolver, VarReference};
pub use expander::{VarExpander, ExpansionMode, ExpansionResult, InheritanceChain};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::error::Result;

fn default_vars_schema() -> String {
    "https://acp-protocol.dev/schemas/v1/vars.schema.json".to_string()
}

/// @acp:summary "Complete vars file structure for .acp.vars.json (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarsFile {
    /// JSON Schema URL for validation
    #[serde(rename = "$schema", default = "default_vars_schema")]
    pub schema: String,
    /// ACP specification version (required)
    pub version: String,
    /// Map of variable names to variable entries (required)
    pub variables: HashMap<String, VarEntry>,
}

impl VarsFile {
    /// Create a new empty vars file
    pub fn new() -> Self {
        Self {
            schema: default_vars_schema(),
            version: crate::VERSION.to_string(),
            variables: HashMap::new(),
        }
    }

    /// Load from JSON file
    pub fn from_json<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
    }

    /// Write to JSON file
    pub fn write_json<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    /// Add a variable entry
    pub fn add_variable(&mut self, name: String, entry: VarEntry) {
        self.variables.insert(name, entry);
    }
}

impl Default for VarsFile {
    fn default() -> Self {
        Self::new()
    }
}

/// @acp:summary "A single variable entry (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarEntry {
    /// Variable type (required)
    #[serde(rename = "type")]
    pub var_type: VarType,
    /// Reference value - qualified name, path, etc. (required)
    pub value: String,
    /// Human-readable description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// References to other variables for inheritance chains (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    /// Source file path where the variable is defined (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Line range [start, end] in source file (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<[usize; 2]>,
}

impl VarEntry {
    /// Create a new symbol variable
    pub fn symbol(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            var_type: VarType::Symbol,
            value: value.into(),
            description,
            refs: vec![],
            source: None,
            lines: None,
        }
    }

    /// Create a new symbol variable with source location
    pub fn symbol_with_source(
        value: impl Into<String>,
        description: Option<String>,
        source: String,
        lines: [usize; 2],
    ) -> Self {
        Self {
            var_type: VarType::Symbol,
            value: value.into(),
            description,
            refs: vec![],
            source: Some(source),
            lines: Some(lines),
        }
    }

    /// Create a new symbol variable with refs (for inheritance)
    pub fn symbol_with_refs(
        value: impl Into<String>,
        description: Option<String>,
        refs: Vec<String>,
    ) -> Self {
        Self {
            var_type: VarType::Symbol,
            value: value.into(),
            description,
            refs,
            source: None,
            lines: None,
        }
    }

    /// Create a new file variable
    pub fn file(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            var_type: VarType::File,
            value: value.into(),
            description,
            refs: vec![],
            source: None,
            lines: None,
        }
    }

    /// Create a new domain variable
    pub fn domain(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            var_type: VarType::Domain,
            value: value.into(),
            description,
            refs: vec![],
            source: None,
            lines: None,
        }
    }

    /// Create a new layer variable
    pub fn layer(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            var_type: VarType::Layer,
            value: value.into(),
            description,
            refs: vec![],
            source: None,
            lines: None,
        }
    }

    /// Create a new pattern variable
    pub fn pattern(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            var_type: VarType::Pattern,
            value: value.into(),
            description,
            refs: vec![],
            source: None,
            lines: None,
        }
    }

    /// Create a new context variable
    pub fn context(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            var_type: VarType::Context,
            value: value.into(),
            description,
            refs: vec![],
            source: None,
            lines: None,
        }
    }
}

/// @acp:summary "Variable type (schema-compliant)"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VarType {
    Symbol,
    File,
    Domain,
    Layer,
    Pattern,
    Context,
}

impl std::fmt::Display for VarType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Symbol => "symbol",
            Self::File => "file",
            Self::Domain => "domain",
            Self::Layer => "layer",
            Self::Pattern => "pattern",
            Self::Context => "context",
        };
        write!(f, "{}", s)
    }
}

/// @acp:summary "Estimate token count from text length"
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// @acp:summary "Capitalize first character of string"
pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().chain(c).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_references() {
        let vars_file = VarsFile {
            schema: default_vars_schema(),
            version: "1.0.0".to_string(),
            variables: HashMap::new(),
        };
        let resolver = VarResolver::new(vars_file);

        let refs = resolver.find_references("Check $SYM_TEST and $ARCH_FLOW.value");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "SYM_TEST");
        assert_eq!(refs[1].name, "ARCH_FLOW");
        assert_eq!(refs[1].modifier, Some("value".to_string()));
    }

    #[test]
    fn test_vars_roundtrip() {
        let mut vars_file = VarsFile::new();
        vars_file.add_variable(
            "SYM_TEST".to_string(),
            VarEntry::symbol("test.rs:test_fn", Some("Test function".to_string())),
        );

        let json = serde_json::to_string_pretty(&vars_file).unwrap();
        let parsed: VarsFile = serde_json::from_str(&json).unwrap();

        assert!(parsed.variables.contains_key("SYM_TEST"));
        assert_eq!(parsed.variables["SYM_TEST"].var_type, VarType::Symbol);
    }
}
