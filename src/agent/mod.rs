//! The execution engine boundary. `ksforge` does not implement a coding
//! agent here — it spawns and talks to the real Claude Code CLI. See
//! docs/03.

pub mod claude_code;
pub mod coordination;
pub mod executor;
pub mod outcome;

pub mod mock;

pub use claude_code::ClaudeCodeExecutor;
pub use coordination::{Classification, CoordinationDecision};
pub use executor::{AgentError, AgentExecutor, AgentRequest, AgentResult, PermissionMode};
pub use mock::MockAgentExecutor;
pub use outcome::{AgentOutcome, OutcomeStatus};
