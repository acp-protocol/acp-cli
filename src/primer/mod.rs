//! @acp:module "Primer"
//! @acp:summary "RFC-0004: AI bootstrap primer generation with value-based section selection"
//! @acp:domain cli
//! @acp:layer feature

pub mod condition;
pub mod dynamic;
pub mod loader;
pub mod renderer;
pub mod scoring;
pub mod selector;
pub mod types;

pub use condition::{evaluate_condition, ProjectState};
pub use loader::{load_primer_config, CliOverrides};
pub use renderer::{render_primer, OutputFormat};
pub use scoring::{calculate_section_value, get_preset_weights};
pub use selector::select_sections;
pub use types::*;
