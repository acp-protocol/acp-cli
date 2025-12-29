//! @acp:module "Variable Expander"
//! @acp:summary "Expands variable references with inheritance support"
//! @acp:domain cli
//! @acp:layer service

use std::collections::{HashMap, HashSet};

use super::{capitalize, estimate_tokens, VarEntry, VarResolver};

/// @acp:summary "Expands variable references with caching"
pub struct VarExpander {
    resolver: VarResolver,
    expansion_cache: HashMap<String, String>,
}

impl VarExpander {
    /// Create a new expander from a resolver
    pub fn new(resolver: VarResolver) -> Self {
        Self {
            resolver,
            expansion_cache: HashMap::new(),
        }
    }

    /// Expand a single variable
    pub fn expand_var(
        &mut self,
        name: &str,
        max_depth: usize,
        visited: &mut HashSet<String>,
    ) -> Option<String> {
        // Check cache
        if let Some(cached) = self.expansion_cache.get(name) {
            return Some(cached.clone());
        }

        // Cycle detection
        if visited.contains(name) {
            return Some(format!("[CYCLE:${}]", name));
        }

        let var = self.resolver.get(name)?;
        visited.insert(name.to_string());

        let mut value = var.value.clone();

        // Recursively expand nested references
        if max_depth > 0 {
            let refs = self.resolver.find_references(&value);
            for r in refs.iter().rev() {
                if let Some(expanded) = self.expand_var(&r.name, max_depth - 1, visited) {
                    value = format!("{}{}{}", &value[..r.start], expanded, &value[r.end..]);
                }
            }
        }

        self.expansion_cache.insert(name.to_string(), value.clone());
        Some(value)
    }

    /// Expand all variable references in text
    pub fn expand_text(&mut self, text: &str, mode: ExpansionMode) -> ExpansionResult {
        let refs = self.resolver.find_references(text);
        let tokens_before = estimate_tokens(text);
        let mut expanded = text.to_string();
        let mut vars_expanded = Vec::new();
        let mut vars_unresolved = Vec::new();
        let mut chains = Vec::new();

        // Process in reverse to preserve positions
        for r in refs.iter().rev() {
            if let Some(var) = self.resolver.get(&r.name).cloned() {
                vars_expanded.push(r.name.clone());

                // Track inheritance chain
                let chain = self.get_inheritance_chain(&r.name);
                chains.push(chain);

                // Format based on mode
                let replacement = self.format_var(&r.name, &var, r.modifier.as_deref(), mode);
                expanded = format!(
                    "{}{}{}",
                    &expanded[..r.start],
                    replacement,
                    &expanded[r.end..]
                );
            } else {
                vars_unresolved.push(r.name.clone());
            }
        }

        ExpansionResult {
            original: text.to_string(),
            expanded,
            vars_expanded,
            vars_unresolved,
            tokens_before,
            tokens_after: estimate_tokens(text),
            inheritance_chains: chains,
        }
    }

    /// Get inheritance chain for a variable by traversing refs
    pub fn get_inheritance_chain(&self, name: &str) -> InheritanceChain {
        let mut chain = vec![name.to_string()];
        let mut visited = HashSet::new();
        visited.insert(name.to_string());

        let has_cycle = self.build_chain(name, &mut chain, &mut visited);
        let depth = chain.len() - 1;

        InheritanceChain {
            root: name.to_string(),
            chain,
            depth,
            has_cycle,
        }
    }

    /// Build inheritance chain by traversing refs recursively
    /// Returns true if a cycle was detected
    fn build_chain(
        &self,
        name: &str,
        chain: &mut Vec<String>,
        visited: &mut HashSet<String>,
    ) -> bool {
        if let Some(var) = self.resolver.get(name) {
            for ref_name in &var.refs {
                if visited.contains(ref_name) {
                    // Cycle detected
                    return true;
                }
                visited.insert(ref_name.clone());
                chain.push(ref_name.clone());
                if self.build_chain(ref_name, chain, visited) {
                    return true;
                }
            }
        }
        false
    }

    fn format_var(
        &mut self,
        name: &str,
        var: &VarEntry,
        modifier: Option<&str>,
        mode: ExpansionMode,
    ) -> String {
        // Handle modifier
        if let Some(m) = modifier {
            return match m {
                "description" | "summary" => var.description.clone().unwrap_or_default(),
                "value" => var.value.clone(),
                "type" => var.var_type.to_string(),
                _ => var.value.clone(),
            };
        }

        // Handle expansion mode
        match mode {
            ExpansionMode::None => format!("${}", name),
            ExpansionMode::Summary => var.description.clone().unwrap_or_else(|| name.to_string()),
            ExpansionMode::Inline => {
                let mut visited = HashSet::new();
                self.expand_var(name, 3, &mut visited)
                    .unwrap_or_else(|| var.value.clone())
            }
            ExpansionMode::Annotated => {
                format!("**${}** → {}", name, self.humanize_value(&var.value))
            }
            ExpansionMode::Block => self.format_block(name, var),
            ExpansionMode::Interactive => {
                format!(
                    r#"<acp-var name="{}" description="{}">{}</acp-var>"#,
                    name,
                    var.description.as_deref().unwrap_or(name),
                    var.description.as_deref().unwrap_or(name)
                )
            }
        }
    }

    fn format_block(&self, name: &str, var: &VarEntry) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "> **{}**: {}",
            name,
            var.description.as_deref().unwrap_or("")
        ));
        lines.push(">".to_string());
        lines.push(format!("> - **type**: {}", var.var_type));
        lines.push(format!("> - **value**: {}", var.value));
        lines.join("\n")
    }

    fn humanize_value(&self, value: &str) -> String {
        value
            .split('|')
            .filter_map(|part| {
                let (key, val) = part.split_once(':')?;

                Some(format!("{}: {}", capitalize(key), val))
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Clear the expansion cache
    pub fn clear_cache(&mut self) {
        self.expansion_cache.clear();
    }
}

/// @acp:summary "Variable expansion mode controlling output format"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionMode {
    /// Keep $VAR as-is
    None,
    /// Replace with description only
    Summary,
    /// Replace with full value inline
    Inline,
    /// Show both: **$VAR** → value
    Annotated,
    /// Full formatted block
    Block,
    /// HTML-like markers for UI
    Interactive,
}

/// @acp:summary "Result of variable expansion operation"
#[derive(Debug, Clone)]
pub struct ExpansionResult {
    pub original: String,
    pub expanded: String,
    pub vars_expanded: Vec<String>,
    pub vars_unresolved: Vec<String>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub inheritance_chains: Vec<InheritanceChain>,
}

/// @acp:summary "Inheritance chain for a variable"
#[derive(Debug, Clone)]
pub struct InheritanceChain {
    pub root: String,
    pub chain: Vec<String>,
    pub depth: usize,
    pub has_cycle: bool,
}
