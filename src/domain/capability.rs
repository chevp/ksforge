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
/// ksforge itself (section 27: Claude Code owns repository exploration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    /// Read/Grep/Glob only — never Edit, Write, or Bash.
    ReadOnly,
    /// The full default tool set, for capabilities that modify the workspace.
    ReadWrite,
}

/// Everything a capability needs to actually run: where the story runs, and
/// the port to the execution engine. Composes agent + workspace/config —
/// deliberately outside pure domain purity for a CLI this size (section 6).
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

/// A kind of operation ksforge can orchestrate (section 8/9). Capabilities
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
    /// Capability-specific instructions appended to the shared prompt
    /// skeleton built in `application::prompt` (section 19: prompt
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
