use std::path::Path;

use crate::domain::{
    ActionResult, Capability, Constraint, ImplementationRequest, LocatedContext, Phase,
    Understanding,
};

/// Core, embedded at compile time (ksforge ships as one binary, see
/// docs/01-installation.md, so prompt text is compiled in rather than read
/// from disk at runtime). Loaded on every LLM turn (UNDERSTAND/LOCATE/ACT).
const CORE: &str = include_str!("../../prompts/system-prompt.md");
const PHASE_UNDERSTAND: &str = include_str!("../../prompts/phases/understand.md");
const PHASE_LOCATE: &str = include_str!("../../prompts/phases/locate.md");
const PHASE_ACT: &str = include_str!("../../prompts/phases/act.md");
const PHASE_VALIDATE: &str = include_str!("../../prompts/phases/validate.md");

/// Appended only when `capability.supports_human_interaction()` — the
/// protocol is meaningless (and actively misleading) for a capability that
/// never offers it, see `application::execute::finish`.
const HUMAN_IN_THE_LOOP: &str = include_str!("../../prompts/fragments/human-in-the-loop.md");
const SCHEMA_REMINDER: &str = include_str!("../../prompts/fragments/schema-reminder.md");

/// One `(system_prompt, user_prompt)` builder per LLM phase (UNDERSTAND,
/// LOCATE, ACT, and VALIDATE when `ValidationPolicy::agent_review` is set —
/// REPORT runs no agent turn at all, see `application::execute`). The one
/// place prompt text gets assembled:
/// never scatter prompt construction across CLI commands. `system_prompt`
/// carries policy that must not be overridable by change-request/repo
/// text; `user_prompt` carries the change request plus, from LOCATE
/// onward, the prior phases' own structured output restated explicitly —
/// correctness never depends on the model's own memory of earlier turns,
/// only on what ksforge hands it this turn (`--resume` is still passed for
/// continuity, but is not load-bearing).
///
/// `working_dir` — not `request.workspace` — is what every builder here
/// states as "Workspace: ...": `request.workspace` is the originally
/// *requested* root, but the agent's actual `--cwd` (`AgentRequest.working_directory`
/// in `application::execute`) is `request.workspace` only when `--dry-run`
/// is off; with it on, the agent runs inside an isolated temp copy
/// instead. Stating the wrong one is not just misleading — confirmed
/// against a real run, a model that builds an absolute path from a stated
/// workspace that doesn't match its real `--cwd` gets flagged by Claude
/// Code's own sandbox as touching a path outside its permitted root,
/// which a non-interactive run has no one to approve.
pub fn for_understand(
    capability: &dyn Capability,
    request: &ImplementationRequest,
    working_dir: &Path,
) -> (String, String) {
    let mut system_prompt = String::new();
    push_core_and_phase(
        &mut system_prompt,
        Phase::Understand,
        PHASE_UNDERSTAND,
        capability,
    );
    common_tail(&mut system_prompt, capability, &request.constraints);

    let user_prompt = format!(
        "Change request:\n{}\n\nWorkspace: {}\n",
        request.change_request.text,
        working_dir.display()
    );
    (system_prompt, user_prompt)
}

pub fn for_locate(
    capability: &dyn Capability,
    request: &ImplementationRequest,
    working_dir: &Path,
    understanding: &Understanding,
) -> (String, String) {
    let mut system_prompt = String::new();
    push_core_and_phase(&mut system_prompt, Phase::Locate, PHASE_LOCATE, capability);
    common_tail(&mut system_prompt, capability, &request.constraints);

    let user_prompt = format!(
        "Change request:\n{}\n\nWorkspace: {}\n\nYour understanding from the UNDERSTAND phase:\n{}\nScope: {}\n",
        request.change_request.text,
        working_dir.display(),
        understanding.summary,
        join_or_none(&understanding.scope, "none named"),
    );
    (system_prompt, user_prompt)
}

pub fn for_act(
    capability: &dyn Capability,
    request: &ImplementationRequest,
    working_dir: &Path,
    understanding: &Understanding,
    located: &LocatedContext,
) -> (String, String) {
    let mut system_prompt = String::new();
    push_core_and_phase(&mut system_prompt, Phase::Act, PHASE_ACT, capability);
    system_prompt.push_str("---\n\n");
    system_prompt.push_str(capability.prompt_fragment());
    system_prompt.push_str("\n\n");
    common_tail(&mut system_prompt, capability, &request.constraints);

    let user_prompt = format!(
        "Change request:\n{}\n\nWorkspace: {}\n\n\
         Your understanding from the UNDERSTAND phase:\n{}\n\n\
         What the LOCATE phase found:\n\
         - Relevant files: {}\n\
         - Existing abstractions to reuse: {}\n\
         - Existing tests: {}\n\
         - Conventions: {}\n",
        request.change_request.text,
        working_dir.display(),
        understanding.summary,
        join_or_none(&located.relevant_files, "none found"),
        join_or_none(&located.existing_abstractions, "none found"),
        join_or_none(&located.existing_tests, "none found"),
        join_or_none(&located.conventions, "none found"),
    );
    (system_prompt, user_prompt)
}

/// Unlike UNDERSTAND/LOCATE/ACT, VALIDATE gets no `capability.prompt_fragment()`
/// — it is a review of ACT's result, not another attempt at the change
/// request in the capability's own voice.
pub fn for_validate(
    capability: &dyn Capability,
    request: &ImplementationRequest,
    working_dir: &Path,
    understanding: &Understanding,
    action: &ActionResult,
) -> (String, String) {
    let mut system_prompt = String::new();
    push_core_and_phase(
        &mut system_prompt,
        Phase::Validate,
        PHASE_VALIDATE,
        capability,
    );
    common_tail(&mut system_prompt, capability, &request.constraints);

    let changed_files: Vec<String> = action
        .changed_files
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let user_prompt = format!(
        "Change request:\n{}\n\nWorkspace: {}\n\n\
         Your understanding from the UNDERSTAND phase:\n{}\n\n\
         What the ACT phase changed:\n\
         - Files: {}\n\
         - Its own summary: {}\n\
         - Its own completed list: {}\n",
        request.change_request.text,
        working_dir.display(),
        understanding.summary,
        join_or_none(&changed_files, "no files changed"),
        action.summary,
        join_or_none(&action.completed, "none reported"),
    );
    (system_prompt, user_prompt)
}

fn push_core_and_phase(
    s: &mut String,
    phase: Phase,
    phase_prompt: &str,
    capability: &dyn Capability,
) {
    s.push_str(CORE);
    s.push_str("\n\n---\n\n");
    s.push_str(&format!(
        "Phase: {phase}\nCapability: {}\n\n",
        capability.id()
    ));
    s.push_str(phase_prompt);
    s.push_str("\n\n");
}

/// Constraints, the human-in-the-loop protocol (only when the capability
/// supports it), and a short output-format reminder — the actual JSON
/// shape is enforced by `--json-schema` on every phase turn, unchanged
/// mechanism; this is just semantics the schema itself can't carry.
fn common_tail(s: &mut String, capability: &dyn Capability, constraints: &[Constraint]) {
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
    s.push_str("---\n\n");
    s.push_str(SCHEMA_REMINDER);
}

fn join_or_none(items: &[String], none_label: &str) -> String {
    if items.is_empty() {
        format!("({none_label})")
    } else {
        items.join(", ")
    }
}
