use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::change_request::ChangeRequest;
use super::constraint::Constraint;

/// Commands run after Claude Code reports success, before ksforge trusts the
/// result. Controlled exclusively by the user/project/workflow — never by
/// the model (section 21).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationPolicy {
    pub commands: Vec<String>,
}

/// Everything needed to ask Claude Code to carry out one capability against
/// one change request in one workspace. Deliberately free of Claude Code
/// request types and of Git/GitHub concepts (sections 7, 20).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationRequest {
    pub change_request: ChangeRequest,
    pub workspace: PathBuf,
    pub capability: String,
    pub constraints: Vec<Constraint>,
    pub validation: ValidationPolicy,
}
