use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::change_request::ChangeRequest;
use super::constraint::Constraint;

/// What happens after ACT reports success, before ksforge trusts the
/// result. Two independent layers: `commands` (deterministic, exact,
/// user-authored — still never inferred by ksforge itself) and
/// `agent_review` (a real VALIDATE turn with Bash access that works out
/// *what* validating this specific change requires — building/packaging/
/// running it, not just a fixed command — and flags related problems it
/// notices along the way; see `prompts/phases/validate.md`). Only *that* a
/// VALIDATE phase always runs is host-controlled (§OS5hRXm); what it
/// checks is the agent's own judgment, not a command list ksforge would
/// otherwise have to keep growing per project type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationPolicy {
    pub commands: Vec<String>,
    #[serde(default)]
    pub agent_review: bool,
}

/// Everything needed to ask Claude Code to carry out one capability against
/// one change request in one workspace. Deliberately free of Claude Code
/// request types and of Git/GitHub concepts (§u8ynkaQ, §HFNMflB).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationRequest {
    pub change_request: ChangeRequest,
    pub workspace: PathBuf,
    pub capability: String,
    pub constraints: Vec<Constraint>,
    pub validation: ValidationPolicy,
}
