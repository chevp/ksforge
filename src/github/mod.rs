//! GitHub/Git integration. The only place branch/commit/PR concepts exist
//! in ksforge (section 20).

pub mod pull_request;

pub use pull_request::{PullRequestOutcome, create_from_execution};
