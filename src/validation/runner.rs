use std::path::Path;

use tokio::process::Command;

use crate::domain::{KsforgeError, ValidationCommandOutcome, ValidationOutcome};

const OUTPUT_TAIL_BYTES: usize = 4000;

/// Run each validation command in order, in `working_dir`, via the
/// platform shell. Stops at the first failure. Commands come exclusively
/// from the user/project/CLI — never from the model (§OS5hRXm) — and are
/// opt-in: an empty policy runs nothing.
pub async fn run(
    commands: &[String],
    working_dir: &Path,
) -> Result<ValidationOutcome, KsforgeError> {
    let mut outcomes = Vec::new();
    let mut all_passed = true;

    for command in commands {
        eprintln!("  running: {command}");
        let mut shell = shell_command(command);
        shell.current_dir(working_dir);
        let output = shell.output().await.map_err(|e| {
            KsforgeError::ValidationFailed(format!("failed to run `{command}`: {e}"))
        })?;

        let passed = output.status.success();
        eprintln!(
            "  {} {command}",
            if passed { "\u{2713}" } else { "\u{2717}" }
        );
        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        let output_tail = tail(&combined, OUTPUT_TAIL_BYTES);

        outcomes.push(ValidationCommandOutcome {
            command: command.clone(),
            passed,
            output_tail,
        });

        if !passed {
            all_passed = false;
            break;
        }
    }

    Ok(ValidationOutcome {
        passed: all_passed,
        commands: outcomes,
        agent_review: None,
    })
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(command);
    c
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(command);
    c
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...[truncated]...{}", &s[s.len() - max..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_policy_passes_trivially() {
        let outcome = run(&[], Path::new(".")).await.unwrap();
        assert!(outcome.passed);
        assert!(outcome.commands.is_empty());
    }

    #[tokio::test]
    async fn stops_at_first_failure() {
        let dir = tempfile::tempdir().unwrap();
        let commands = vec!["exit 1".to_string(), "exit 0".to_string()];
        let outcome = run(&commands, dir.path()).await.unwrap();
        assert!(!outcome.passed);
        assert_eq!(outcome.commands.len(), 1);
        assert!(!outcome.commands[0].passed);
    }

    #[tokio::test]
    async fn all_pass() {
        let dir = tempfile::tempdir().unwrap();
        let commands = vec!["exit 0".to_string()];
        let outcome = run(&commands, dir.path()).await.unwrap();
        assert!(outcome.passed);
    }
}
