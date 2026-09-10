use serde_json::json;

use crate::domain::{Execution, ExecutionStatus};

use super::args::OutputFormat;

/// Print a finished or paused execution. In `Json` mode stdout carries
/// exactly one JSON object and nothing else (section 24); in `Text` mode
/// it's a short human transcript. Diagnostics never go to stdout in either
/// mode — see [`eprint_notice`].
pub fn print_execution(execution: &Execution, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&as_json(execution)).unwrap());
        }
        OutputFormat::Text => print_human(execution),
    }
}

fn as_json(execution: &Execution) -> serde_json::Value {
    json!({
        "execution_id": execution.id.0,
        "capability": execution.capability,
        "status": execution.status.to_string(),
        "pending_question": execution.pending_question,
        "result": execution.result,
        "artifacts": execution.artifacts,
    })
}

fn print_human(execution: &Execution) {
    println!();
    println!("ksforge");
    println!("{}", "─".repeat(36));
    println!();
    println!("Capability:");
    println!("  {}", execution.capability);
    println!();
    println!("User story:");
    println!(
        "  {}",
        execution.user_story.text.lines().next().unwrap_or("")
    );
    println!();
    println!("Execution: {}", execution.id);

    match execution.status {
        ExecutionStatus::WaitingForHuman => {
            if let Some(q) = &execution.pending_question {
                println!();
                println!("Status:");
                println!("  Waiting for human input");
                println!();
                println!("Question:");
                println!("  {}", q.question);
                println!();
                println!("Options:");
                for o in &q.options {
                    println!("  {} - {}", o.id, o.label);
                }
                println!();
                println!(
                    "Resume:\n  ksforge resume {} --decision <option-id>",
                    execution.id
                );
            }
        }
        ExecutionStatus::Completed => {
            if let Some(result) = &execution.result {
                println!();
                if result.changed_files.is_empty() {
                    println!("Changed: none");
                } else {
                    println!("Changed:");
                    for f in &result.changed_files {
                        println!("  {}", f.display());
                    }
                }
                if !result.validation.commands.is_empty() {
                    println!();
                    println!("Validation:");
                    for c in &result.validation.commands {
                        println!("  {} {}", if c.passed { "✓" } else { "✗" }, c.command);
                    }
                }
                println!();
                println!("Result:");
                println!("  {}", result.summary);
            }
        }
        ExecutionStatus::Failed => {
            println!();
            println!("Result:");
            println!(
                "  Failed: {}",
                execution
                    .result
                    .as_ref()
                    .map(|r| r.summary.as_str())
                    .unwrap_or("unknown error")
            );
        }
        ExecutionStatus::Running | ExecutionStatus::Cancelled => {
            println!();
            println!("Status: {}", execution.status);
        }
    }
    println!();
}

/// Progress/diagnostic text. Always stderr, in every output mode, so
/// `--format json` stdout is never polluted (section 24).
pub fn eprint_notice(message: &str) {
    eprintln!("{message}");
}
