//! GitHub/Git integration. The only place branch/commit/PR concepts exist
//! in ksforge (section 20).

pub mod comment;
pub mod decision;
mod process;
pub mod pull_request;
pub mod report;

pub use comment::post_or_update_report;
pub use decision::{authorize_commenter, parse_choose_command};
pub use pull_request::{PullRequestOutcome, create_from_execution};
pub use report::render as render_report;
