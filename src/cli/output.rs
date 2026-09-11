use serde_json::json;

use crate::agent::CoordinationDecision;
use crate::domain::{Execution, ExecutionStatus};

use super::args::OutputFormat;

/// Print a `ksforge coordinate` result. Same `Json`/`Text` contract as
/// [`print_execution`] — not an `Execution` (this command never starts
/// one), so it gets its own small printer instead of reusing that one.
pub fn print_coordination_decision(decision: &CoordinationDecision, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(decision).unwrap());
        }
        OutputFormat::Text => {
            println!();
            println!("Classification: {:?}", decision.classification);
            println!("Proceed: {}", if decision.proceed { "YES" } else { "NO" });
            print_bullets("Affected scope", &decision.affected_scope);
            print_bullets("Related executions", &decision.related_executions);
            print_bullets("Dependencies", &decision.dependencies);
            print_bullets("Conflicts", &decision.conflicts);
            print_bullets("Recommendations", &decision.recommendations);
            println!();
        }
    }
}

fn print_bullets(label: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    println!();
    println!("{label}:");
    for item in items {
        println!("  - {item}");
    }
}

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
        "gates": execution.gates,
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
    println!("Change request:");
    println!(
        "  {}",
        execution.change_request.text.lines().next().unwrap_or("")
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
                if let Some(rec) = &q.recommended_option {
                    println!();
                    println!("Recommended:");
                    println!("  {rec}");
                    if !q.context.is_empty() {
                        println!("  {}", q.context);
                    }
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
                if !result.open_items.is_empty() {
                    println!();
                    println!("Open:");
                    for item in &result.open_items {
                        println!("  {item}");
                    }
                }
                if let Some(rec) = &result.recommendation {
                    println!();
                    println!("Recommendation:");
                    println!("  {rec}");
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
