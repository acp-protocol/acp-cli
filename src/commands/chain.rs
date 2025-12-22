//! @acp:module "Chain Command"
//! @acp:summary "Show variable inheritance chain"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::vars::{VarExpander, VarResolver, VarsFile};

/// Options for the chain command
#[derive(Debug, Clone)]
pub struct ChainOptions {
    /// Variable name
    pub name: String,
    /// Vars file path
    pub vars: PathBuf,
    /// Show as tree
    pub tree: bool,
}

/// Execute the chain command
pub fn execute_chain(options: ChainOptions) -> Result<()> {
    let vars_file = VarsFile::from_json(&options.vars)?;
    let resolver = VarResolver::new(vars_file);
    let expander = VarExpander::new(resolver);

    let name = options.name.trim_start_matches('$');
    let chain = expander.get_inheritance_chain(name);

    if options.tree {
        println!("{}", style(format!("${}", name)).cyan().bold());
        print_chain_tree(&chain.chain, 0);
    } else {
        println!("Root: {}", style(&chain.root).cyan());
        println!("Depth: {}", chain.depth);
        println!("Chain: {}", chain.chain.join(" → "));
    }

    Ok(())
}

/// Print inheritance chain as a tree
fn print_chain_tree(chain: &[String], depth: usize) {
    for (i, item) in chain.iter().enumerate() {
        let prefix = if i == chain.len() - 1 {
            "└── "
        } else {
            "├── "
        };
        let indent = "    ".repeat(depth);
        println!("{}{}{}", indent, prefix, style(item).dim());
    }
}
