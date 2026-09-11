use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Implement a change request end to end: analyze, plan, modify, validate. The
/// primary capability (section 8) and the only one with full
/// human-in-the-loop depth by default.
pub struct Implement;

#[async_trait]
impl Capability for Implement {
    fn id(&self) -> &'static str {
        "implement"
    }

    fn description(&self) -> &'static str {
        "Implement a change request in the current workspace."
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy::ReadWrite
    }

    fn default_constraints(&self) -> Vec<Constraint> {
        crate::domain::constraint::write_constraints()
    }

    fn supports_human_interaction(&self) -> bool {
        true
    }

    fn prompt_fragment(&self) -> &'static str {
        include_str!("../../prompts/capabilities/implement.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
