use std::path::Path;

use tokio::process::Command;

use crate::domain::{KsforgeError, Result};

/// The one place ksforge spawns `git`/`gh` subprocesses. `pull_request`,
/// `comment`, and `decision` all go through this rather than each shelling
/// out independently (section 20: Git/GitHub are infrastructure, and one
/// infrastructure detail — how a subprocess is run and its failure
/// reported — belongs in one place).
pub(crate) async fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| KsforgeError::GitHub(format!("failed to run {program} {args:?}: {e}")))?;

    if !output.status.success() {
        return Err(KsforgeError::GitHub(format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) async fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    run(cwd, "git", args).await
}

pub(crate) async fn run_gh(cwd: &Path, args: &[&str]) -> Result<String> {
    run(cwd, "gh", args).await
}
