//! @acp:module "Variable Resolver"
//! @acp:summary "Resolves variable references and provides lookup functionality"
//! @acp:domain cli
//! @acp:layer service

use regex::Regex;
use std::collections::HashMap;

use super::{VarType, VarEntry, VarsFile};

/// @acp:summary "Resolves variable references from a vars file"
pub struct VarResolver {
    vars: HashMap<String, VarEntry>,
    var_pattern: Regex,
}

impl VarResolver {
    /// Create a new resolver from a vars file
    pub fn new(vars_file: VarsFile) -> Self {
        Self {
            vars: vars_file.variables,
            var_pattern: Regex::new(r"\$([A-Z][A-Z0-9_]+)(?:\.(\w+))?").unwrap(),
        }
    }

    /// Get a variable by name
    pub fn get(&self, name: &str) -> Option<&VarEntry> {
        self.vars.get(name)
    }

    /// Find all variable references in text
    pub fn find_references(&self, text: &str) -> Vec<VarReference> {
        self.var_pattern
            .captures_iter(text)
            .map(|cap| VarReference {
                full_match: cap.get(0).unwrap().as_str().to_string(),
                name: cap.get(1).unwrap().as_str().to_string(),
                modifier: cap.get(2).map(|m| m.as_str().to_string()),
                start: cap.get(0).unwrap().start(),
                end: cap.get(0).unwrap().end(),
            })
            .collect()
    }

    /// Get variables by type
    pub fn by_type(&self, var_type: VarType) -> Vec<&VarEntry> {
        self.vars
            .values()
            .filter(|v| v.var_type == var_type)
            .collect()
    }

    /// Search variables by query string
    pub fn search(&self, query: &str) -> Vec<&VarEntry> {
        let q = query.to_lowercase();
        self.vars
            .values()
            .filter(|v| {
                v.value.to_lowercase().contains(&q)
                    || v.description.as_ref().map(|s| s.to_lowercase().contains(&q)).unwrap_or(false)
            })
            .collect()
    }
}

/// @acp:summary "A parsed variable reference from text"
#[derive(Debug, Clone)]
pub struct VarReference {
    pub full_match: String,
    pub name: String,
    pub modifier: Option<String>,
    pub start: usize,
    pub end: usize,
}
