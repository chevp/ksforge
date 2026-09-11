use async_trait::async_trait;

use crate::domain::{
    Capability, Constraint, Execution, ExecutionContext, ImplementationRequest, Result, ToolPolicy,
};

/// Generate an image from a text prompt. Only runs when explicitly
/// requested via `ksforge txt2img` — never bundled into `implement`/`fix`.
/// Stub for now: no dependency on the standalone `ks-llm-image` crate
/// (`apps/kosmos/libs/ks-llm-image`, which also backs [`super::img2img`]),
/// which builds/tests independently of this repo's own CI
/// (`.github/workflows/ci.yml` checks out only `chevp/ksforge`, so a local
/// `path` dependency into the kosmos superrepo would break it there).
pub struct Txt2Img;

#[async_trait]
impl Capability for Txt2Img {
    fn id(&self) -> &'static str {
        "txt2img"
    }

    fn description(&self) -> &'static str {
        "Generate an image from a text prompt (stub, no real backend yet)."
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy::ReadWrite
    }

    fn default_constraints(&self) -> Vec<Constraint> {
        crate::domain::constraint::write_constraints()
    }

    fn prompt_fragment(&self) -> &'static str {
        include_str!("../../prompts/capabilities/txt2img.md")
    }

    async fn execute(
        &self,
        request: ImplementationRequest,
        context: ExecutionContext,
    ) -> Result<Execution> {
        crate::application::execute::run(self, request, context).await
    }
}
