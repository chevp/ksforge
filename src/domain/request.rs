use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::constraint::Constraint;
use super::story::UserStory;

/// Commands run after Claude Code reports success, before ksforge trusts the
/// result. Controlled exclusively by the user/project/workflow — never by
/// the model (section 21).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationPolicy {
    pub commands: Vec<String>,
}

/// Everything needed to ask Claude Code to carry out one capability against
/// one story in one workspace. Deliberately free of Claude Code request
/// types and of Git/GitHub concepts (sections 7, 20).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationRequest {
    pub story: UserStory,
    pub workspace: PathBuf,
    pub capability: String,
    pub constraints: Vec<Constraint>,
    pub validation: ValidationPolicy,
}
