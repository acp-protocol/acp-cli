//! @acp:module "Tool Adapters"
//! @acp:summary "Built-in adapters for all supported AI tools"
//! @acp:domain cli
//! @acp:layer service

mod aider;
mod claude_code;
mod cline;
mod continue_dev;
mod copilot;
mod cursor;
mod generic;
mod windsurf;

pub use aider::AiderAdapter;
pub use claude_code::ClaudeCodeAdapter;
pub use cline::ClineAdapter;
pub use continue_dev::ContinueAdapter;
pub use copilot::CopilotAdapter;
pub use cursor::CursorAdapter;
pub use generic::GenericAdapter;
pub use windsurf::WindsurfAdapter;
