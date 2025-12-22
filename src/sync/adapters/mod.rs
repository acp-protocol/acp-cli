//! @acp:module "Tool Adapters"
//! @acp:summary "Built-in adapters for all supported AI tools"
//! @acp:domain cli
//! @acp:layer service

mod cursor;
mod claude_code;
mod copilot;
mod continue_dev;
mod windsurf;
mod cline;
mod aider;
mod generic;

pub use cursor::CursorAdapter;
pub use claude_code::ClaudeCodeAdapter;
pub use copilot::CopilotAdapter;
pub use continue_dev::ContinueAdapter;
pub use windsurf::WindsurfAdapter;
pub use cline::ClineAdapter;
pub use aider::AiderAdapter;
pub use generic::GenericAdapter;
