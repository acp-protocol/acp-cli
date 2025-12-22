//! @acp:module "Expansion Presets"
//! @acp:summary "Re-exports expansion types and presets from vars module"
//! @acp:domain cli
//! @acp:layer utility
//!
//! This module provides convenient re-exports for variable expansion functionality.

pub use crate::vars::{
    ExpansionMode, ExpansionResult, InheritanceChain,
    VarExpander, VarResolver, presets,
};