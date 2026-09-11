use std::collections::HashMap;

use crate::agents::{FailureContext, RecoveryAgent, RecoveryProposal};
use crate::domain::Artifact;
use crate::techniques::{TechniqueId, TechniqueRegistry};
use crate::workflow::{StepId, ValueRef, Workflow};

use super::state_machine::{Event, MachineState, StateMachine};

/// What running a workflow to completion (or as far as it degrades to)
/// produced.
#[derive(Debug)]
pub enum ExecutionOutcome {
    Completed(Artifact),
    Degraded {
        partial: Option<Artifact>,
        reason: String,
    },
}

/// Drives a validated `Workflow` step by step through the `StateMachine`,
/// calling techniques and consulting the `RecoveryAgent` on failure. This
/// is the only place that calls `TechniqueRegistry::execute` — see
/// CLAUDE.md section 22: agents propose, techniques execute, the runtime
/// decides what happens next.
pub struct WorkflowEngine<'a> {
    registry: &'a TechniqueRegistry,
    recovery_agent: &'a dyn RecoveryAgent,
    state_machine: StateMachine,
}

impl<'a> WorkflowEngine<'a> {
    pub fn new(registry: &'a TechniqueRegistry, recovery_agent: &'a dyn RecoveryAgent) -> Self {
        let mut state_machine = StateMachine::new();
        state_machine.handle(Event::Start);
        println!("STARTING");
        println!("READY");
        Self {
            registry,
            recovery_agent,
            state_machine,
        }
    }

    pub fn state(&self) -> MachineState {
        self.state_machine.state()
    }

    pub fn run(&mut self, workflow: &Workflow, input: Artifact) -> ExecutionOutcome {
        let mut values: Vec<(ValueRef, Artifact)> = vec![(ValueRef::Input, input)];
        let mut overrides: HashMap<StepId, TechniqueId> = HashMap::new();

        let mut index = 0;
        while index < workflow.steps.len() {
            let step = &workflow.steps[index];
            let technique_id = overrides
                .get(&step.id)
                .cloned()
                .unwrap_or_else(|| step.operation.clone());

            self.state_machine.handle(Event::RunStep);
            println!("PROCESSING: {technique_id}");

            let inputs: Vec<Artifact> = step
                .inputs
                .iter()
                .map(|value_ref| {
                    values
                        .iter()
                        .find(|(available, _)| available == value_ref)
                        .map(|(_, artifact)| artifact.clone())
                        .expect("the builder only ever wires up values already produced")
                })
                .collect();

            let attempt = self.state_machine.attempt();
            match self.registry.execute(&technique_id, &inputs, attempt) {
                Ok(output) => {
                    self.state_machine.handle(Event::StepSucceeded);
                    println!("  \u{2713} success");
                    values.push((step.output.clone(), output));
                    index += 1;
                }
                Err(error) => {
                    self.state_machine.handle(Event::StepFailed(error.clone()));
                    println!("  \u{2717} failure: {error}");
                    println!("RECOVERING");

                    let failure = FailureContext {
                        workflow_id: workflow.id.clone(),
                        step_id: step.id.clone(),
                        technique: technique_id.clone(),
                        error,
                        attempt,
                    };
                    let proposal = self.recovery_agent.diagnose(&failure);
                    println!("  RecoveryAgent: proposal = {proposal:?}");

                    match proposal {
                        RecoveryProposal::Retry | RecoveryProposal::RetryWithParameters(_) => {
                            self.state_machine.handle(Event::RetryRequested);
                            // Same index: the next loop iteration retries this step.
                        }
                        RecoveryProposal::AlternativeTechnique(alternative) => {
                            overrides.insert(step.id.clone(), alternative.clone());
                            self.state_machine
                                .handle(Event::AlternativeTechniqueRequested(alternative));
                        }
                        RecoveryProposal::Replan => {
                            // A full replan would call back into the planning
                            // agent and rebuild the remaining workflow; this
                            // example keeps that boundary visible without
                            // exercising it, since none of the demo scenarios
                            // need it.
                            self.state_machine.handle(Event::ReplanRequested);
                            self.state_machine.handle(Event::ReplanCompleted);
                        }
                        RecoveryProposal::ContinueDegraded => {
                            self.state_machine.handle(Event::ContinueDegradedRequested);
                            println!("DEGRADED");
                            let partial = values.last().map(|(_, artifact)| artifact.clone());
                            return ExecutionOutcome::Degraded {
                                partial,
                                reason: format!("step '{}' could not recover", step.id),
                            };
                        }
                    }
                }
            }
        }

        println!("READY");
        let (_, result) = values
            .pop()
            .expect("a workflow always produces at least its input");
        ExecutionOutcome::Completed(result)
    }

    /// Only an explicit shutdown ends the runtime — see CLAUDE.md
    /// section 12.
    pub fn shutdown(&mut self) {
        self.state_machine.handle(Event::ShutdownRequested);
        println!("STOPPING");
        self.state_machine.handle(Event::ShutdownCompleted);
        println!("STOPPED");
    }
}
