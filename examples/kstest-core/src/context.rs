use std::collections::BTreeMap;

/// Identifies one run of the runtime. Purely for technical tracing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExecutionId(pub String);

/// Ties events and log lines belonging to the same logical operation
/// together, independent of `ExecutionId` (one execution can span many
/// correlated operations over its lifetime).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub String);

pub type Metadata = BTreeMap<String, String>;

/// Technical context carried alongside an execution. Deliberately holds
/// only identifiers and free-form key/value metadata — never workflow
/// state, conversation history, agent state, or domain data. Those belong
/// to whatever capability or intelligence layer runs on top of the core.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub execution_id: ExecutionId,
    pub correlation_id: CorrelationId,
    pub metadata: Metadata,
}

impl ExecutionContext {
    pub fn new(execution_id: impl Into<String>, correlation_id: impl Into<String>) -> Self {
        Self {
            execution_id: ExecutionId(execution_id.into()),
            correlation_id: CorrelationId(correlation_id.into()),
            metadata: Metadata::new(),
        }
    }
}
