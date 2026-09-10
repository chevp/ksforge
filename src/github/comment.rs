use std::path::Path;

use crate::domain::{Execution, Result};

use super::process::run_gh;
use super::report;

/// Post or update the ksforge status/decision comment on a pull request
/// (section 13-15). Finds the existing comment carrying this execution's
/// marker (if any) and updates it in place rather than appending a new
/// comment on every status change — the PR should stay readable.
pub async fn post_or_update_report(
    workspace_root: &Path,
    pr_number: u64,
    execution: &Execution,
) -> Result<()> {
    let body = report::render(execution);
    let marker = report::marker_for(&execution.id.0);

    match find_marker_comment(workspace_root, pr_number, &marker).await? {
        Some(comment_id) => {
            run_gh(
                workspace_root,
                &[
                    "api",
                    &format!("repos/{{owner}}/{{repo}}/issues/comments/{comment_id}"),
                    "-X",
                    "PATCH",
                    "-f",
                    &format!("body={body}"),
                ],
            )
            .await?;
        }
        None => {
            run_gh(
                workspace_root,
                &["pr", "comment", &pr_number.to_string(), "--body", &body],
            )
            .await?;
        }
    }
    Ok(())
}

/// `{owner}`/`{repo}` are `gh api`'s own placeholders, resolved from the
/// repository `gh` is already operating against — no separate `gh repo
/// view` round trip needed.
async fn find_marker_comment(
    workspace_root: &Path,
    pr_number: u64,
    marker: &str,
) -> Result<Option<u64>> {
    let output = run_gh(
        workspace_root,
        &[
            "api",
            &format!("repos/{{owner}}/{{repo}}/issues/{pr_number}/comments"),
            "--paginate",
        ],
    )
    .await?;
    let comments: Vec<serde_json::Value> = serde_json::from_str(&output)?;
    Ok(comments.into_iter().find_map(|c| {
        let body = c.get("body")?.as_str()?;
        if body.contains(marker) {
            c.get("id")?.as_u64()
        } else {
            None
        }
    }))
}
