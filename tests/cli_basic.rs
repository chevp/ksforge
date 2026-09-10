use assert_cmd::Command;
use predicates::prelude::*;

fn ksforge() -> Command {
    Command::cargo_bin("ksforge").unwrap()
}

#[test]
fn help_lists_the_five_verbs() {
    ksforge()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("implement"))
        .stdout(predicate::str::contains("review"))
        .stdout(predicate::str::contains("fix"))
        .stdout(predicate::str::contains("explain"))
        .stdout(predicate::str::contains("resume"));
}

#[test]
fn version_flag_works() {
    ksforge().arg("--version").assert().success();
}

#[test]
fn capabilities_lists_all_four() {
    ksforge()
        .arg("capabilities")
        .assert()
        .success()
        .stdout(predicate::str::contains("implement"))
        .stdout(predicate::str::contains("review"))
        .stdout(predicate::str::contains("fix"))
        .stdout(predicate::str::contains("explain"));
}

#[test]
fn implement_without_story_is_usage_error_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .args(["implement", "--workspace", "."])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--story"));
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
        .stderr(predicate::str::contains("/ksforge choose"));
}

#[test]
fn story_and_story_file_are_mutually_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    ksforge()
        .current_dir(dir.path())
        .args(["implement", "--story", "a", "--story-file", "b.md"])
        .assert()
        .failure()
        .code(2);
}
