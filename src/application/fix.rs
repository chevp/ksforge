use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Analyze and apply a fix for a described problem. Shares `implement`'s
/// write access and validation posture; the difference is purely in the
/// prompt (fixing a described defect vs. building a described feature).
pub struct Fix;

#[async_trait]
impl Capability for Fix {
    fn id(&self) -> &'static str {
        "fix"
    }

    fn description(&self) -> &'static str {
        "Diagnose and fix a described problem in the workspace."
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
        include_str!("../../prompts/capabilities/fix.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
