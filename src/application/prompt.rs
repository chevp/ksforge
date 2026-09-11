use crate::domain::{Capability, Constraint, ImplementationRequest};

/// Core + policies, embedded at compile time (section: single-binary
/// distribution, see docs/01-installation.md — ksforge ships as one binary,
/// so prompt text is compiled in rather than read from disk at runtime).
/// Loaded together, in this fixed order, on every run — not optional depth,
/// see the header note in `prompts/system-prompt.md`.
const CORE: &str = include_str!("../../prompts/system-prompt.md");
const POLICY_REPOSITORY_ANALYSIS: &str =
    include_str!("../../prompts/policies/repository-analysis.md");
const POLICY_VALIDATION_AND_OUTPUT: &str =
    include_str!("../../prompts/policies/validation-and-output.md");

/// Appended only when `capability.supports_human_interaction()` — the
/// protocol is meaningless (and actively misleading) for a capability that
/// never offers it, see `application::execute::finish`.
const HUMAN_IN_THE_LOOP: &str = include_str!("../../prompts/fragments/human-in-the-loop.md");

/// The one place prompt text gets assembled (section 19/11: never scatter
/// prompt construction across CLI commands). Produces a `(system_prompt,
/// user_prompt)` pair; `system_prompt` carries policy that must not be
/// overridable by change-request/repo text (section 34), `user_prompt` carries the
/// change request and capability-specific instructions.
pub fn build(capability: &dyn Capability, request: &ImplementationRequest) -> (String, String) {
    let system_prompt = system_prompt(capability, &request.constraints);
    let user_prompt = user_prompt(capability, request);
    (system_prompt, user_prompt)
}

fn system_prompt(capability: &dyn Capability, constraints: &[Constraint]) -> String {
    let mut s = String::new();
    s.push_str(CORE);
    s.push_str("\n\n---\n\n");
    s.push_str(POLICY_REPOSITORY_ANALYSIS);
    s.push_str("\n\n---\n\n");
    s.push_str(POLICY_VALIDATION_AND_OUTPUT);
    s.push_str("\n\n---\n\n");
    s.push_str(&format!("Capability: {}\n", capability.id()));
    s.push_str(&format!("{}\n\n", capability.prompt_fragment()));
    if !constraints.is_empty() {
        s.push_str("Constraints:\n");
        for c in constraints {
            s.push_str(&format!("- {}: {}\n", c.id, c.description));
        }
        s.push('\n');
    }
    if capability.supports_human_interaction() {
        s.push_str("---\n\n");
        s.push_str(HUMAN_IN_THE_LOOP);
        s.push('\n');
    }
    s.push_str(
        "Respond with your final turn matching the required JSON schema exactly. \
         Do not include explanatory text outside the JSON.\n",
    );
    s
}

fn user_prompt(_capability: &dyn Capability, request: &ImplementationRequest) -> String {
    format!(
        "Change request:\n{}\n\nWorkspace: {}\n",
        request.change_request.text,
        request.workspace.display()
    )
}
