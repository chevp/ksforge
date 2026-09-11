//! GitHub/Git integration. The only place branch/commit/PR concepts exist
//! in ksforge (§HFNMflB).

pub mod comment;
pub mod decision;
mod process;
pub mod pull_request;
mod repo;
pub mod report;

pub use comment::post_or_update_report;
pub use decision::{
    FollowUpCommand, authorize_commenter, parse_choose_command, parse_follow_up_command,
};
pub use pull_request::{
    PullRequestBatch, PullRequestOutcome, RepoCommitBatch, RepoCommitOutcome, commit_to_new_branch,
    create_from_execution, merge_branch_into, push_follow_up,
};
pub use repo::{GitState, discover_repos, git_state};
pub use report::render as render_report;
