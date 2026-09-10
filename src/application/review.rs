use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Analyze code and report findings. Never modifies files (section: CLI
/// design, `review` "must not modify files by default" carried over from
/// the tool-transformation spec's intent, kept here too).
pub struct Review;

#[async_trait]
impl Capability for Review {
    fn id(&self) -> &'static str {
        "review"
    }

    fn description(&self) -> &'static str {
        "Review code and report findings without modifying anything."
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy::ReadOnly
    }

    fn default_constraints(&self) -> Vec<Constraint> {
        crate::domain::constraint::read_only_constraints()
    }

    fn prompt_fragment(&self) -> &'static str {
        include_str!("../../prompts/capabilities/review.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
