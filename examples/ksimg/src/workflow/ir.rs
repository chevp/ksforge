use std::fmt;

use crate::domain::DataType;
use crate::techniques::TechniqueId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkflowId(pub String);

impl WorkflowId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for WorkflowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StepId(pub String);

impl StepId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for StepId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A reference to a value produced somewhere in the workflow: either the
/// workflow's own input, or a previous step's output. Only the builder
/// creates these — see `workflow::builder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueRef {
    Input,
    StepOutput(StepId),
}

#[derive(Debug, Clone)]
pub struct InputSpec {
    pub data_type: DataType,
}

#[derive(Debug, Clone)]
pub struct OutputSpec {
    pub value: ValueRef,
    pub data_type: DataType,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: StepId,
    pub operation: TechniqueId,
    pub inputs: Vec<ValueRef>,
    pub output: ValueRef,
}

/// The technical, declarative workflow representation. Still data, not
/// code: nothing here executes anything, it only describes a graph of
/// technique calls. See CLAUDE.md section 8.
#[derive(Debug, Clone)]
pub struct Workflow {
    pub id: WorkflowId,
    pub input: InputSpec,
    pub steps: Vec<Step>,
    pub output: OutputSpec,
}
