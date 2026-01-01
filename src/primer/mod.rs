//! @acp:module "Primer"
//! @acp:summary "RFC-0004/RFC-0015: AI bootstrap primer generation with tiered selection"
//! @acp:domain cli
//! @acp:layer feature

pub mod condition;
pub mod dynamic;
pub mod ide;
pub mod loader;
pub mod renderer;
pub mod scoring;
pub mod selector;
pub mod types;

pub use condition::{evaluate_condition, ProjectState};
pub use ide::IdeEnvironment;
pub use loader::{load_primer_config, CliOverrides};
pub use renderer::{render_primer, render_primer_with_tier, OutputFormat};
pub use scoring::{calculate_section_value, get_preset_weights};
pub use selector::select_sections;
pub use types::*;
