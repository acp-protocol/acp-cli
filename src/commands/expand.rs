//! @acp:module "Expand Command"
//! @acp:summary "Expand variable references in text"
//! @acp:domain cli
//! @acp:layer handler

use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::vars::{ExpansionMode, VarExpander, VarResolver, VarsFile};

/// Options for the expand command
#[derive(Debug, Clone)]
pub struct ExpandOptions {
    /// Text to expand (reads from stdin if None)
    pub text: Option<String>,
    /// Expansion mode
    pub mode: String,
    /// Vars file path
    pub vars: PathBuf,
    /// Show inheritance chains
    pub chains: bool,
}

/// Execute the expand command
pub fn execute_expand(options: ExpandOptions) -> Result<()> {
    let vars_file = VarsFile::from_json(&options.vars)?;
    let resolver = VarResolver::new(vars_file);
    let mut expander = VarExpander::new(resolver);

    let input = match options.text {
        Some(t) => t,
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };

    let expansion_mode = match options.mode.as_str() {
        "none" => ExpansionMode::None,
        "summary" => ExpansionMode::Summary,
        "inline" => ExpansionMode::Inline,
        "annotated" => ExpansionMode::Annotated,
        "block" => ExpansionMode::Block,
        "interactive" => ExpansionMode::Interactive,
        _ => ExpansionMode::Annotated,
    };

    let result = expander.expand_text(&input, expansion_mode);
    println!("{}", result.expanded);

    if options.chains && !result.inheritance_chains.is_empty() {
        println!("\n{}", style("Inheritance Chains:").bold());
        for chain in &result.inheritance_chains {
            println!(
                "  {} → {}",
                style(&chain.root).cyan(),
                chain.chain.join(" → ")
            );
        }
    }

    Ok(())
}
