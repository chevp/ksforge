use async_trait::async_trait;

use crate::domain::{
    Capability, ChangeScope, Constraint, Execution, ExecutionContext, ImplementationRequest,
    Result, ToolPolicy,
};

/// Adds or updates tests without touching non-test source. Unlike
/// `implement`/`fix`, the write scope isn't just requested in the prompt —
/// it's checked deterministically after ACT (`Capability::change_scope`,
/// `domain::test_scope`), so a run that touches a non-test file fails
/// regardless of what the agent reports.
pub struct Test;

#[async_trait]
impl Capability for Test {
    fn id(&self) -> &'static str {
        "test"
    }

    fn description(&self) -> &'static str {
        "Add or update tests only — never modifies non-test source files."
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy::ReadWrite
    }

    fn default_constraints(&self) -> Vec<Constraint> {
        crate::domain::constraint::test_only_constraints()
    }

    fn supports_human_interaction(&self) -> bool {
        true
    }

    fn change_scope(&self) -> Option<ChangeScope> {
        Some(ChangeScope::TestsOnly)
    }

    fn prompt_fragment(&self) -> &'static str {
        include_str!("../../prompts/capabilities/test.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
