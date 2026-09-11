use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Explain part of the workspace. Read-only, like `review`, but the
/// change request names what to explain rather than asking for findings.
pub struct Explain;

#[async_trait]
impl Capability for Explain {
    fn id(&self) -> &'static str {
        "explain"
    }

    fn description(&self) -> &'static str {
        "Explain part of the workspace without modifying anything."
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy::ReadOnly
    }

    fn default_constraints(&self) -> Vec<Constraint> {
        crate::domain::constraint::read_only_constraints()
    }

    fn prompt_fragment(&self) -> &'static str {
        include_str!("../../prompts/capabilities/explain.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
