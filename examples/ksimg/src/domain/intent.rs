use super::types::{Constraints, DataType};

/// What the `IntentAgent` produces: a goal, not a plan. Nothing here names
/// a technique or a step order — see CLAUDE.md section 6 ("Intent Agent").
#[derive(Debug, Clone)]
pub struct ProcessingIntent {
    pub input_type: DataType,
    pub output_type: DataType,
    pub goal: String,
    pub constraints: Constraints,
}
