use crate::domain::{Execution, ExecutionEvent, ExecutionStatus, HumanDecisionRequest};

/// Render an [`Execution`] as the markdown body for its ksforge status/
/// decision PR comment (§egkEINs/§qHiXmf0/§sTdJsQn/§TkQFZyO). Pure and side-effect free so it
/// can be unit-tested without `gh`/network — posting is `github::comment`'s
/// job.
pub fn render(execution: &Execution) -> String {
    let marker = marker_for(&execution.id.0);
    let body = match execution.status {
        ExecutionStatus::WaitingForHuman => render_waiting(execution),
        ExecutionStatus::Completed => render_completed(execution),
        ExecutionStatus::Failed => render_failed(execution),
        ExecutionStatus::Cancelled => render_cancelled(execution),
        ExecutionStatus::Running => render_running(execution),
    };
    format!("{marker}\n{body}")
}

/// The HTML-comment marker ksforge uses to find its own comment on a PR
/// (§egkEINs) so it can update it instead of appending a new one every
/// time — never the only visible content of the comment.
pub fn marker_for(execution_id: &str) -> String {
    format!("<!-- ksforge:execution={execution_id}:report -->")
}

fn render_waiting(execution: &Execution) -> String {
    let Some(gate) = &execution.pending_question else {
        return render_running(execution);
    };
    let mut s = String::new();
    s.push_str("## ksforge — Decision required\n\n");
    s.push_str(&format!("**Execution:** `{}`\n\n", execution.id));

    s.push_str("### Completed\n\n");
    push_bullets_or(&mut s, &gate.completed, "No prior progress recorded.");
    s.push('\n');

    s.push_str("### Still open\n\n");
    s.push_str(&gate.question);
    s.push_str("\n\n");

    s.push_str("### Recommendation\n\n");
    push_recommendation(
        &mut s,
        gate.recommended_option.as_deref(),
        &gate.context,
        &gate.options,
    );

    s.push_str("### Options\n\n");
    for (i, option) in gate.options.iter().enumerate() {
        s.push_str(&format!("{}. **{}**\n", i + 1, option.label));
        s.push_str(&format!("   `/ksforge choose {}`\n\n", option.id));
    }

    s.push_str("### What happens next\n\n");
    s.push_str(
        "After a decision is received, ksforge will resume this execution, provide the \
         selected decision to Claude Code, continue the implementation, and run validation \
         again.\n\n",
    );

    s.push_str("**Status:** `WAITING_FOR_HUMAN`\n");
    s
}

fn render_completed(execution: &Execution) -> String {
    let mut s = String::new();
    s.push_str("## ksforge — Completed\n\n");
    s.push_str(&format!("**Execution:** `{}`\n\n", execution.id));

    let result = execution.result.as_ref();

    s.push_str("### Completed\n\n");
    let completed = result.map(|r| r.completed.as_slice()).unwrap_or(&[]);
    push_bullets_or(&mut s, completed, "No details reported.");
    s.push('\n');

    if let Some(result) = result
        && !result.validation.commands.is_empty()
    {
        s.push_str("### Validation\n\n");
        for c in &result.validation.commands {
            s.push_str(&format!(
                "- `{}`: {}\n",
                c.command,
                if c.passed { "passed" } else { "failed" }
            ));
        }
        s.push('\n');
    }

    if let Some(review) = result.and_then(|r| r.validation.agent_review.as_ref()) {
        s.push_str("### Validation (agent)\n\n");
        s.push_str(&format!("{}\n\n", review.summary));
        for c in &review.completed {
            s.push_str(&format!("- {c}\n"));
        }
        if !review.open_items.is_empty() {
            s.push_str("\nFurther findings:\n\n");
            for item in &review.open_items {
                s.push_str(&format!("- {item}\n"));
            }
        }
        if let Some(rec) = &review.recommendation {
            s.push_str(&format!("\nRecommendation: {rec}\n"));
        }
        s.push('\n');
    }

    let decisions = decision_history(execution);
    if !decisions.is_empty() {
        s.push_str("### Human decisions\n\n");
        for (gate, option) in &decisions {
            let label = gate
                .options
                .iter()
                .find(|o| o.id == *option)
                .map(|o| o.label.as_str())
                .unwrap_or(option);
            s.push_str(&format!("- {}: {}\n", gate.question, label));
        }
        s.push('\n');
    }

    let open_items = result.map(|r| r.open_items.as_slice()).unwrap_or(&[]);
    if !open_items.is_empty() {
        s.push_str("### Open items\n\n");
        for item in open_items {
            s.push_str(&format!("- {item}\n"));
        }
        s.push('\n');
    }

    s.push_str("### Recommendation\n\n");
    match result.and_then(|r| r.recommendation.as_deref()) {
        Some(rec) if !rec.is_empty() => {
            s.push_str(rec);
            s.push_str("\n\n");
        }
        _ => {
            s.push_str("No further action required.\n\n");
        }
    }

    s.push_str("**Status:** `COMPLETED`\n");
    s
}

fn render_failed(execution: &Execution) -> String {
    let mut s = String::new();
    s.push_str("## ksforge — Failed\n\n");
    s.push_str(&format!("**Execution:** `{}`\n\n", execution.id));
    s.push_str("### Result\n\n");
    let summary = execution
        .result
        .as_ref()
        .map(|r| r.summary.as_str())
        .unwrap_or("unknown error");
    s.push_str(&format!("Failed: {summary}\n\n"));
    s.push_str("**Status:** `FAILED`\n");
    s
}

fn render_cancelled(execution: &Execution) -> String {
    let mut s = String::new();
    s.push_str("## ksforge — Cancelled\n\n");
    s.push_str(&format!("**Execution:** `{}`\n\n", execution.id));
    if let Some(result) = &execution.result {
        s.push_str(&format!("{}\n\n", result.summary));
    }
    s.push_str("**Status:** `CANCELLED`\n");
    s
}

fn render_running(execution: &Execution) -> String {
    format!(
        "## ksforge — Running\n\n**Execution:** `{}`\n\n**Status:** `RUNNING`\n",
        execution.id
    )
}

fn push_bullets_or(s: &mut String, items: &[String], fallback: &str) {
    if items.is_empty() {
        s.push_str(&format!("- {fallback}\n"));
    } else {
        for item in items {
            s.push_str(&format!("- {item}\n"));
        }
    }
}

fn push_recommendation(
    s: &mut String,
    recommended_option: Option<&str>,
    context: &str,
    options: &[crate::domain::DecisionOption],
) {
    match recommended_option {
        Some(rec) => {
            let label = options
                .iter()
                .find(|o| o.id == rec)
                .map(|o| o.label.as_str())
                .unwrap_or(rec);
            s.push_str(&format!("**{label}**\n\n"));
            if !context.is_empty() {
                s.push_str(context);
                s.push_str("\n\n");
            }
        }
        None if !context.is_empty() => {
            s.push_str(context);
            s.push_str("\n\n");
        }
        None => {
            s.push_str("No recommendation given.\n\n");
        }
    }
}

/// Pair each resolved gate with the option that was chosen, in order — the
/// gate history is sequential (one open at a time, per `Execution::ask`),
/// so the nth `HumanDecided` event always answers the nth gate.
fn decision_history(execution: &Execution) -> Vec<(&HumanDecisionRequest, &str)> {
    let decided_options = execution.messages.iter().filter_map(|e| match e {
        ExecutionEvent::HumanDecided { option, .. } => Some(option.as_str()),
        _ => None,
    });
    execution.gates.iter().zip(decided_options).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ChangeRequest, DecisionOption, Execution, ExecutionResult, HumanDecision, ValidationOutcome,
    };

    fn change_request() -> ChangeRequest {
        ChangeRequest::from_text("As a user, I want password reset.").unwrap()
    }

    #[test]
    fn waiting_report_carries_marker_and_recommendation() {
        let mut exec = Execution::start(change_request(), "implement");
        exec.ask(
            "OAuth2 or JWT?".into(),
            vec![
                DecisionOption {
                    id: "oauth2".into(),
                    label: "OAuth 2".into(),
                },
                DecisionOption {
                    id: "jwt".into(),
                    label: "JWT".into(),
                },
            ],
            Some("oauth2".into()),
            "The repo already has an OAuth boundary.".into(),
            vec!["Analyzed the auth module.".into()],
        );

        let body = render(&exec);
        assert!(body.starts_with(&marker_for(&exec.id.0)));
        assert!(body.contains("Decision required"));
        assert!(body.contains("Analyzed the auth module."));
        assert!(body.contains("**OAuth 2**"));
        assert!(body.contains("/ksforge choose oauth2"));
        assert!(body.contains("/ksforge choose jwt"));
        assert!(body.contains("WAITING_FOR_HUMAN"));
    }

    #[test]
    fn waiting_report_without_recommendation_says_so() {
        let mut exec = Execution::start(change_request(), "implement");
        exec.ask(
            "Postgres or MySQL?".into(),
            vec![DecisionOption {
                id: "postgres".into(),
                label: "Postgres".into(),
            }],
            None,
            String::new(),
            Vec::new(),
        );

        let body = render(&exec);
        assert!(body.contains("No recommendation given."));
        assert!(body.contains("No prior progress recorded."));
    }

    #[test]
    fn completed_report_lists_decisions_and_no_further_action() {
        let mut exec = Execution::start(change_request(), "implement");
        exec.ask(
            "OAuth2 or JWT?".into(),
            vec![DecisionOption {
                id: "oauth2".into(),
                label: "OAuth 2".into(),
            }],
            None,
            String::new(),
            Vec::new(),
        );
        exec.apply_decision(&HumanDecision {
            execution_id: exec.id.clone(),
            option: "oauth2".into(),
            decided_by: Some("chevp".into()),
        });
        exec.complete(ExecutionResult {
            success: true,
            title: Some("Add OAuth2 authentication".into()),
            summary: "Implemented OAuth2 authentication.".into(),
            changed_files: vec!["src/auth.rs".into()],
            validation: ValidationOutcome::default(),
            completed: vec!["Implemented OAuth 2.0 authentication.".into()],
            open_items: Vec::new(),
            recommendation: None,
        });

        let body = render(&exec);
        assert!(body.contains("## ksforge — Completed"));
        assert!(body.contains("OAuth2 or JWT?: OAuth 2"));
        assert!(body.contains("No further action required."));
        assert!(body.contains("COMPLETED"));
    }

    #[test]
    fn failed_report_shows_summary() {
        let mut exec = Execution::start(change_request(), "implement");
        exec.fail("validate.sh failed");
        let body = render(&exec);
        assert!(body.contains("Failed: validate.sh failed"));
        assert!(body.contains("FAILED"));
    }
}
