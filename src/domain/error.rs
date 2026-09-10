use thiserror::Error;

/// Error taxonomy for ksforge. Each variant maps to a distinct process exit
/// code (see [`KsforgeError::exit_code`]) so scripts and CI can branch on
/// failure kind without parsing text.
#[derive(Debug, Error)]
pub enum KsforgeError {
    #[error("invalid usage: {0}")]
    Usage(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("Claude Code is unavailable: {0}")]
    ExecutorUnavailable(String),

    #[error("Claude Code execution failed: {0}")]
    ExecutorFailed(String),

    #[error("validation failed: {0}")]
    ValidationFailed(String),

    #[error("policy violation: {0}")]
    PolicyViolation(String),

    #[error("workspace error: {0}")]
    Workspace(String),

    #[error("GitHub integration error: {0}")]
    GitHub(String),

    #[error("execution not found: {0}")]
    ExecutionNotFound(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl KsforgeError {
    /// Process exit code for this error. 0 (success) is never returned here;
    /// callers that need it construct it separately.
    ///
    /// ```text
    /// 1 = general/application failure (workspace, GitHub, not-found)
    /// 2 = invalid CLI usage / configuration
    /// 3 = Claude Code (agent) unavailable or execution failure
    /// 4 = validation failure
    /// 5 = policy/safety violation
    /// ```
    /// Exit code 6 (waiting_for_human) is not an error and is assigned
    /// directly by `main` when an `Execution` pauses — see docs/06.
    pub fn exit_code(&self) -> i32 {
        match self {
            KsforgeError::Usage(_) | KsforgeError::Config(_) => 2,
            KsforgeError::ExecutorUnavailable(_) | KsforgeError::ExecutorFailed(_) => 3,
            KsforgeError::ValidationFailed(_) => 4,
            KsforgeError::PolicyViolation(_) => 5,
            KsforgeError::Workspace(_)
            | KsforgeError::GitHub(_)
            | KsforgeError::ExecutionNotFound(_)
            | KsforgeError::Io(_)
            | KsforgeError::Json(_) => 1,
        }
    }
}

pub type Result<T> = std::result::Result<T, KsforgeError>;
