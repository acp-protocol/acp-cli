//! @acp:module "Expansion Presets"
//! @acp:summary "Pre-configured expansion modes for common use cases"
//! @acp:domain cli
//! @acp:layer utility

use super::ExpansionMode;

/// For AI-to-AI communication - keeps $VAR references intact
pub const AI_TO_AI: ExpansionMode = ExpansionMode::None;

/// Quick human reading - inline expansion
pub const QUICK: ExpansionMode = ExpansionMode::Inline;

/// Detailed human reading (default) - annotated format
pub const DETAILED: ExpansionMode = ExpansionMode::Annotated;

/// Documentation generation - full block format
pub const DOCUMENTATION: ExpansionMode = ExpansionMode::Block;

/// Interactive web UI - HTML-like markers
pub const INTERACTIVE: ExpansionMode = ExpansionMode::Interactive;

/// Most compact human-readable - summary only
pub const SUMMARY: ExpansionMode = ExpansionMode::Summary;
