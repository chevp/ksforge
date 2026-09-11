use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Transform an existing image per a text prompt. Only runs when explicitly
/// requested via `ksforge img2img` — never bundled into `implement`/`fix`.
/// A separate capability from [`super::txt2img::Txt2Img`], though both are
/// meant to eventually call the same standalone `ks-llm-image` crate
/// (`apps/kosmos/libs/ks-llm-image`) — not imported here yet, see
/// [`super::txt2img`] for why.
pub struct Img2Img;

#[async_trait]
impl Capability for Img2Img {
    fn id(&self) -> &'static str {
        "img2img"
    }

    fn description(&self) -> &'static str {
        "Transform an existing image per a text prompt (stub, no real backend yet)."
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy::ReadWrite
    }

    fn default_constraints(&self) -> Vec<Constraint> {
        crate::domain::constraint::write_constraints()
    }

    fn prompt_fragment(&self) -> &'static str {
        include_str!("../../prompts/capabilities/img2img.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
