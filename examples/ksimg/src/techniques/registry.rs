use std::collections::HashMap;
use std::fmt;

use crate::domain::{Artifact, DataType};

use super::TechniqueExecutor;

/// Identifies a technique. A newtype rather than a bare `String` so a
/// typo'd operation name is a type error at the call sites that matter
/// (the registry, the builder), not a silent no-op.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TechniqueId(pub String);

impl TechniqueId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for TechniqueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for TechniqueId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for TechniqueId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// One declared parameter. `description` is the whole schema for this
/// example; see `schemas/*.json` and the README for how a real system
/// would attach a full JSON Schema here instead.
#[derive(Debug, Clone)]
pub struct ParameterSpec {
    pub name: String,
    pub description: String,
}

pub type ParameterSchema = Vec<ParameterSpec>;

/// Non-functional facts about a technique — the kind a planner or builder
/// needs to choose between alternatives without knowing how any of them
/// is actually implemented.
#[derive(Debug, Clone)]
pub struct TechniqueMetadata {
    pub cost: u32,
    pub latency_ms: u32,
    pub capabilities: Vec<String>,
    pub constraints: Vec<String>,
}

/// The contract a technique publishes to the rest of the system. The
/// builder and validator reason about this — never about the executor
/// behind it. This is the JSON-Schema-shaped idea from CLAUDE.md section 3
/// expressed as a plain Rust struct.
#[derive(Debug, Clone)]
pub struct Technique {
    pub id: TechniqueId,
    pub input_types: Vec<DataType>,
    pub output_types: Vec<DataType>,
    pub parameters: ParameterSchema,
    pub metadata: TechniqueMetadata,
}

/// A technique's contract paired with the mock that carries it out.
struct RegisteredTechnique {
    contract: Technique,
    executor: Box<dyn TechniqueExecutor>,
}

/// The source of technical truth about what techniques exist and what
/// they need and produce. Agents propose operations by name; only the
/// registry knows whether that name resolves to something real — see
/// CLAUDE.md section 5.
#[derive(Default)]
pub struct TechniqueRegistry {
    entries: HashMap<TechniqueId, RegisteredTechnique>,
}

impl TechniqueRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, contract: Technique, executor: Box<dyn TechniqueExecutor>) {
        self.entries.insert(
            contract.id.clone(),
            RegisteredTechnique { contract, executor },
        );
    }

    pub fn get(&self, id: &TechniqueId) -> Option<&Technique> {
        self.entries.get(id).map(|entry| &entry.contract)
    }

    pub fn find_by_input_output(&self, input: DataType, output: DataType) -> Vec<&Technique> {
        self.entries
            .values()
            .map(|entry| &entry.contract)
            .filter(|technique| {
                technique.input_types.contains(&input) && technique.output_types.contains(&output)
            })
            .collect()
    }

    pub fn list(&self) -> Vec<&Technique> {
        self.entries.values().map(|entry| &entry.contract).collect()
    }

    /// Runs the technique's mock executor. This is the only place inputs
    /// actually reach an executor — builder and validator never call this.
    pub fn execute(
        &self,
        id: &TechniqueId,
        inputs: &[Artifact],
        attempt: u32,
    ) -> Result<Artifact, String> {
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| format!("unknown technique: {id}"))?;
        entry.executor.execute(inputs, attempt)
    }
}
