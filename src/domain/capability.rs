use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::AgentExecutor;

use super::constraint::Constraint;
use super::error::Result;
use super::execution::Execution;
use super::request::ImplementationRequest;

/// What a capability is allowed to touch. Enforced by which tools ksforge
/// hands Claude Code (`--tools` / `--allowedTools`), not re-implemented by
/// ksforge itself (§3kuclkU: Claude Code owns repository exploration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    /// Read/Grep/Glob only — never Edit, Write, or Bash.
    ReadOnly,
    /// The full default tool set, for capabilities that modify the workspace.
    ReadWrite,
}

/// A restriction on which files a capability's ACT phase may have touched,
/// checked deterministically against the filesystem diff after ACT
/// (`domain::test_scope`) — the host enforces this, not the prompt (see
/// docs/03-architecture.md, "The phase loop"). `None` (the default) means
/// no restriction beyond `tool_policy()` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeScope {
    /// Every changed path must be a recognized test file/directory for its
    /// language, or a non-test file that contains a recognized inline-test
    /// marker after the change (e.g. Rust's colocated `#[cfg(test)]`) — see
    /// `domain::test_scope`. A heuristic, not a full diff: it cannot prove
    /// a mixed file's change was *only* to its test portion, only that the
    /// file plausibly contains tests at all.
    TestsOnly,
}

/// Everything a capability needs to actually run: where the change request runs, and
/// the port to the execution engine. Composes agent + workspace/config —
/// deliberately outside pure domain purity for a CLI this size (§UKoR4HU).
#[derive(Clone)]
pub struct ExecutionContext {
    pub executor: Arc<dyn AgentExecutor>,
    pub workspace_root: PathBuf,
    pub model: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub dry_run: bool,
    /// Path to a Claude Code `--mcp-config` file, giving the agent access
    /// to additional MCP servers for this run — see
    /// docs/11-integrations.md. `None` by default.
    pub mcp_config: Option<PathBuf>,
}

/// A kind of operation ksforge can orchestrate (§6Kh5ESS/§mgbJZP0). Capabilities
/// share one execution pipeline (`crate::application::execute::run`); each
/// impl only supplies policy: id, description, tool scope, default
/// constraints, and its prompt fragment.
#[async_trait]
pub trait Capability: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn tool_policy(&self) -> ToolPolicy;
    fn default_constraints(&self) -> Vec<Constraint>;
    /// Whether this capability may pause on a `HumanDecisionRequest`.
    /// `review`/`explain` never do — they have nothing to decide.
    fn supports_human_interaction(&self) -> bool {
        false
    }
    /// See `ChangeScope`. `None` by default — most capabilities have no
    /// restriction beyond `tool_policy()`.
    fn change_scope(&self) -> Option<ChangeScope> {
        None
    }
    /// Capability-specific instructions appended to the shared prompt
    /// skeleton built in `application::prompt` (§TkQFZyO: prompt
    /// construction stays centralized, capabilities only add their slice).
    fn prompt_fragment(&self) -> &'static str;

    /// Every impl's body is the same one-liner delegating to the shared
    /// pipeline (`application::execute::run`) — this cannot be a default
    /// method here because that pipeline takes `&dyn Capability`, and a
    /// default method's `&Self` cannot unsize to `&dyn Capability` without
    /// `Self: Sized` (which an object-safe trait can't require).
    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution>;
}

pub struct CapabilityRegistry {
    capabilities: Vec<Arc<dyn Capability>>,
}

impl CapabilityRegistry {
    pub fn with_defaults() -> Self {
        let capabilities: Vec<Arc<dyn Capability>> = vec![
            Arc::new(crate::application::implement::Implement),
            Arc::new(crate::application::review::Review),
            Arc::new(crate::application::fix::Fix),
            Arc::new(crate::application::explain::Explain),
            Arc::new(crate::application::txt2img::Txt2Img),
            Arc::new(crate::application::test::Test),
        ];
        Self { capabilities }
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Capability>> {
        self.capabilities.iter().find(|c| c.id() == id).cloned()
    }

    pub fn list(&self) -> &[Arc<dyn Capability>] {
        &self.capabilities
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
