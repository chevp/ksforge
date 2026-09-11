use std::collections::HashMap;
use std::collections::HashSet;

use crate::domain::{Constraints, DataType};
use crate::techniques::TechniqueRegistry;

use super::ir::{StepId, ValueRef, Workflow};

/// Everything that can be wrong with an already-built `Workflow`. Only a
/// workflow that validates with no errors may be executed — see CLAUDE.md
/// section 10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    UnknownTechnique(String),
    InvalidInputType {
        step: String,
        expected: DataType,
        got: DataType,
    },
    MissingInput {
        step: String,
    },
    InvalidReference {
        step: String,
    },
    CycleDetected(String),
    OutputNotReachable,
    ConstraintViolated(String),
}

pub struct WorkflowValidator;

impl WorkflowValidator {
    pub fn validate(
        workflow: &Workflow,
        registry: &TechniqueRegistry,
        constraints: &Constraints,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let Some(limit) = constraints.max_steps
            && workflow.steps.len() > limit
        {
            errors.push(ValidationError::ConstraintViolated(format!(
                "workflow has {} steps, limit is {limit}",
                workflow.steps.len()
            )));
        }

        if let Some(cycle) = detect_cycle(workflow) {
            errors.push(cycle);
        }

        // Built up strictly in step order: a reference to a step that
        // hasn't run yet can never resolve, which is what rules out
        // forward references without a separate graph walk.
        let mut available: Vec<(ValueRef, DataType)> =
            vec![(ValueRef::Input, workflow.input.data_type)];

        for step in &workflow.steps {
            if constraints
                .denied_techniques
                .iter()
                .any(|denied| denied == &step.operation.0)
            {
                errors.push(ValidationError::ConstraintViolated(format!(
                    "technique '{}' is denied",
                    step.operation
                )));
            }

            let Some(technique) = registry.get(&step.operation) else {
                errors.push(ValidationError::UnknownTechnique(
                    step.operation.to_string(),
                ));
                continue;
            };

            if step.inputs.len() != technique.input_types.len() {
                errors.push(ValidationError::MissingInput {
                    step: step.id.to_string(),
                });
            }

            for (value_ref, expected) in step.inputs.iter().zip(&technique.input_types) {
                match available
                    .iter()
                    .find(|(available_ref, _)| available_ref == value_ref)
                {
                    Some((_, actual)) if actual == expected => {}
                    Some((_, actual)) => errors.push(ValidationError::InvalidInputType {
                        step: step.id.to_string(),
                        expected: *expected,
                        got: *actual,
                    }),
                    None => errors.push(ValidationError::InvalidReference {
                        step: step.id.to_string(),
                    }),
                }
            }

            if let ValueRef::StepOutput(id) = &step.output
                && id != &step.id
            {
                errors.push(ValidationError::InvalidReference {
                    step: step.id.to_string(),
                });
            }

            let output_type = technique
                .output_types
                .first()
                .copied()
                .unwrap_or(workflow.input.data_type);
            available.push((step.output.clone(), output_type));
        }

        match available
            .iter()
            .find(|(value_ref, _)| *value_ref == workflow.output.value)
        {
            Some((_, data_type)) if *data_type == workflow.output.data_type => {}
            _ => errors.push(ValidationError::OutputNotReachable),
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Walks the dependency graph implied by `ValueRef::StepOutput`
/// references and reports the first step found on a cycle. The builder
/// never produces one (see `workflow::builder`), but a hand-assembled or
/// future non-linear `Workflow` could, so the check is real rather than
/// assumed away.
fn detect_cycle(workflow: &Workflow) -> Option<ValidationError> {
    let depends_on: HashMap<&StepId, Vec<&StepId>> = workflow
        .steps
        .iter()
        .map(|step| {
            let deps = step
                .inputs
                .iter()
                .filter_map(|value_ref| match value_ref {
                    ValueRef::StepOutput(id) => Some(id),
                    ValueRef::Input => None,
                })
                .collect();
            (&step.id, deps)
        })
        .collect();

    let mut visited: HashSet<&StepId> = HashSet::new();

    for step in &workflow.steps {
        let mut in_stack: HashSet<&StepId> = HashSet::new();
        if visit(&step.id, &depends_on, &mut visited, &mut in_stack) {
            return Some(ValidationError::CycleDetected(step.id.to_string()));
        }
    }
    None
}

fn visit<'a>(
    node: &'a StepId,
    depends_on: &HashMap<&'a StepId, Vec<&'a StepId>>,
    visited: &mut HashSet<&'a StepId>,
    in_stack: &mut HashSet<&'a StepId>,
) -> bool {
    if in_stack.contains(node) {
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    visited.insert(node);
    in_stack.insert(node);
    if let Some(deps) = depends_on.get(node) {
        for dep in deps {
            if visit(dep, depends_on, visited, in_stack) {
                return true;
            }
        }
    }
    in_stack.remove(node);
    false
}
