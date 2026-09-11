use crate::agents::LogicalWorkflow;
use crate::domain::{Constraints, DataType};
use crate::techniques::{TechniqueId, TechniqueRegistry};

use super::ir::{InputSpec, OutputSpec, Step, StepId, ValueRef, Workflow, WorkflowId};

/// Everything that can go wrong while turning a `LogicalWorkflow` into an
/// executable `Workflow`. Structured, not a bare string — see CLAUDE.md
/// section 9, point 7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    UnknownOperation(String),
    MissingInputType {
        operation: String,
        expected: DataType,
    },
    TechniqueDenied(String),
    TooManySteps {
        limit: usize,
        actual: usize,
    },
}

/// Resolves a `LogicalWorkflow` against the `TechniqueRegistry` into an
/// executable `Workflow`. Deterministic: never calls an agent, never
/// touches runtime state — see CLAUDE.md section 22.
pub struct WorkflowBuilder;

impl WorkflowBuilder {
    pub fn build(
        logical: &LogicalWorkflow,
        registry: &TechniqueRegistry,
        constraints: &Constraints,
        input_type: DataType,
    ) -> Result<Workflow, Vec<BuildError>> {
        let mut errors = Vec::new();

        if let Some(limit) = constraints.max_steps
            && logical.steps.len() > limit
        {
            errors.push(BuildError::TooManySteps {
                limit,
                actual: logical.steps.len(),
            });
        }

        // Available values so far, seeded with the workflow's own input.
        // Resolving inputs by scanning this pool (rather than requiring
        // the planner to name exact sources) is what keeps `LogicalStep`
        // free of wiring — see CLAUDE.md section 7.
        let mut pool: Vec<(ValueRef, DataType)> = vec![(ValueRef::Input, input_type)];
        let mut steps = Vec::new();

        for (index, logical_step) in logical.steps.iter().enumerate() {
            if constraints
                .denied_techniques
                .iter()
                .any(|denied| denied == &logical_step.operation)
            {
                errors.push(BuildError::TechniqueDenied(logical_step.operation.clone()));
                continue;
            }

            let technique_id = TechniqueId::new(logical_step.operation.clone());
            let Some(technique) = registry.get(&technique_id) else {
                errors.push(BuildError::UnknownOperation(logical_step.operation.clone()));
                continue;
            };

            let mut inputs = Vec::new();
            let mut satisfied = true;
            for required in &technique.input_types {
                match pool
                    .iter()
                    .rev()
                    .find(|(_, data_type)| data_type == required)
                {
                    Some((value_ref, _)) => inputs.push(value_ref.clone()),
                    None => {
                        satisfied = false;
                        errors.push(BuildError::MissingInputType {
                            operation: logical_step.operation.clone(),
                            expected: *required,
                        });
                    }
                }
            }
            if !satisfied {
                continue;
            }

            let step_id = StepId::new(format!("step-{index}-{}", logical_step.operation));
            let output = ValueRef::StepOutput(step_id.clone());
            let output_type = *technique
                .output_types
                .first()
                .expect("a technique contract always declares at least one output type");

            pool.push((output.clone(), output_type));
            steps.push(Step {
                id: step_id,
                operation: technique_id,
                inputs,
                output,
            });
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let (output_value, output_type) = pool
            .last()
            .cloned()
            .expect("the workflow input is always in the pool");

        let id = WorkflowId::new(format!(
            "wf-{}",
            logical
                .steps
                .iter()
                .map(|step| step.operation.as_str())
                .collect::<Vec<_>>()
                .join("-")
        ));

        Ok(Workflow {
            id,
            input: InputSpec {
                data_type: input_type,
            },
            steps,
            output: OutputSpec {
                value: output_value,
                data_type: output_type,
            },
        })
    }
}
