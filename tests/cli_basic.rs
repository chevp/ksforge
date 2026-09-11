use assert_cmd::Command;
use predicates::prelude::*;

fn ksforge() -> Command {
    Command::cargo_bin("ksforge").unwrap()
}

#[test]
fn help_lists_the_seven_verbs() {
    ksforge()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("implement"))
        .stdout(predicate::str::contains("review"))
        .stdout(predicate::str::contains("fix"))
        .stdout(predicate::str::contains("explain"))
        .stdout(predicate::str::contains("txt2img"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("resume"));
}

#[test]
fn version_flag_works() {
    ksforge().arg("--version").assert().success();
}

#[test]
fn capabilities_lists_all_six() {
    ksforge()
        .arg("capabilities")
        .assert()
        .success()
        .stdout(predicate::str::contains("implement"))
        .stdout(predicate::str::contains("review"))
        .stdout(predicate::str::contains("fix"))
        .stdout(predicate::str::contains("explain"))
        .stdout(predicate::str::contains("txt2img"))
        .stdout(predicate::str::contains("test"));
}

#[test]
fn implement_without_change_request_is_usage_error_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .args(["implement", "--workspace", "."])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--change"));
}

#[test]
fn status_on_unknown_execution_is_not_found_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .args(["status", "ksf_doesnotexist"])
        .assert()
        .failure()
        .code(1);
}

/// `status` with no execution id at all is the workspace-wide overview
/// (no repos here — a plain empty tempdir).
#[test]
fn status_with_no_execution_id_shows_an_empty_workspace_overview() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("no git repositories found"));
}

/// The overview is about ksforge's own executions, not the repo's git
/// state — a repo with no executions yet still counts, but does not print
/// a branch or dirty flag anywhere.
#[test]
fn status_with_no_execution_id_lists_the_current_repo() {
    let dir = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap();

    ksforge()
        .current_dir(dir.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("repos     1"))
        .stdout(predicate::str::contains("Executions:"))
        .stdout(predicate::str::contains("(none found)"));
}

#[test]
fn cancel_on_unknown_execution_is_not_found_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .args(["cancel", "ksf_doesnotexist"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn handle_comment_without_choose_command_is_usage_error_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .args([
            "handle-comment",
            "--execution-id",
            "ksf_doesnotexist",
            "--comment-id",
            "1",
            "--commenter",
            "someone",
            "--body",
            "looks good to me",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("does not contain a recognized"));
}

/// `chat` no longer requires `--workspace` to be a git repository up front —
/// a workspace root with no `.git` anywhere (a plain multi-repo folder, or
/// no repo at all) is a supported shape; git plumbing only matters at
/// merge-back time, per repo actually touched (see `github::repo`).
#[test]
fn chat_works_without_a_git_repository() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .arg("chat")
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("ksforge chat"));
}

#[test]
fn bare_invocation_with_no_subcommand_is_also_chat() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("ksforge chat"));
}

#[test]
fn chat_prints_a_banner_and_exits_cleanly_on_immediate_eof() {
    let dir = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap();

    ksforge()
        .current_dir(dir.path())
        .arg("chat")
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("ksforge chat"));
}

#[test]
fn change_with_at_prefix_reads_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    // No such file — this only proves the `@` prefix was actually parsed as
    // a file reference (a distinct, file-specific error) rather than taken
    // as literal change-request text; a real file is covered at the unit
    // level in `cli::commands::load_change_request_tests`.
    ksforge()
        .current_dir(dir.path())
        .args(["implement", "--change", "@does-not-exist.md"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("does-not-exist.md"));
}
