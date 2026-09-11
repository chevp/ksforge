use serde::{Deserialize, Serialize};

/// A single operating rule ksforge tells Claude Code (and enforces or checks
/// where possible) about how it may work. Constraints are policy, not
/// implementation detail: they never encode Git/GitHub concepts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Constraint {
    pub id: String,
    pub description: String,
}

impl Constraint {
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
        }
    }
}

/// Default constraints for a capability id. Capabilities may extend this set
/// (see `Capability::default_constraints`); users/projects may add further
/// constraints on top via CLI/config, but may never remove the baseline
/// safety constraints below.
pub fn baseline_constraints() -> Vec<Constraint> {
    vec![
        Constraint::new(
            "inspect-before-edit",
            "Inspect the relevant parts of the repository before modifying files.",
        ),
        Constraint::new(
            "avoid-unrelated-changes",
            "Do not modify files unrelated to the change request.",
        ),
        Constraint::new(
            "no-secret-exposure",
            "Never print, log, commit, or embed API keys, tokens, or other secrets.",
        ),
        Constraint::new(
            "no-arbitrary-dependencies",
            "Do not install or add dependencies unless the change request requires it.",
        ),
    ]
}

pub fn write_constraints() -> Vec<Constraint> {
    let mut c = baseline_constraints();
    c.push(Constraint::new(
        "preserve-public-api",
        "Preserve existing public APIs unless the change request explicitly requires changing them.",
    ));
    c.push(Constraint::new(
        "run-existing-tests",
        "Run the project's existing test suite if one exists, before reporting success.",
    ));
    c
}

pub fn read_only_constraints() -> Vec<Constraint> {
    let mut c = baseline_constraints();
    c.push(Constraint::new(
        "read-only",
        "Do not modify any files; only read and report.",
    ));
    c
}
