use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Implement a user story end to end: analyze, plan, modify, validate. The
/// primary capability (section 8) and the only one with full
/// human-in-the-loop depth by default.
pub struct Implement;

#[async_trait]
impl Capability for Implement {
    fn id(&self) -> &'static str {
        "implement"
    }

    fn description(&self) -> &'static str {
        "Implement a user story in the current workspace."
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
        "Implement the user story below in this workspace. Inspect the relevant \
         parts of the codebase first, make the smallest coherent set of changes \
         that satisfies the story, and prefer the codebase's existing patterns \
         and conventions over introducing new ones."
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
