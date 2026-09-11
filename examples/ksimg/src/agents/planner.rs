use crate::domain::{DataType, ProcessingIntent};

/// One named operation and why the plan calls for it. No IDs, no
/// references, no wiring — just what the planner believes needs to
/// happen, and in what order. See CLAUDE.md section 7.
#[derive(Debug, Clone)]
pub struct LogicalStep {
    pub operation: String,
    pub purpose: String,
}

/// The planner's whole output: an ordered list of logical steps. This is
/// deliberately not a graph — turning it into one is the
/// `workflow::builder`'s job, not the planner's.
#[derive(Debug, Clone)]
pub struct LogicalWorkflow {
    pub steps: Vec<LogicalStep>,
}

pub trait PlanningAgent {
    fn plan(&self, intent: &ProcessingIntent) -> LogicalWorkflow;
}

/// Deterministic rules standing in for an LLM planner — see CLAUDE.md
/// section 15. Maps a goal's shape onto a fixed operation sequence.
pub struct MockPlanningAgent;

impl PlanningAgent for MockPlanningAgent {
    fn plan(&self, intent: &ProcessingIntent) -> LogicalWorkflow {
        let goal = intent.goal.to_lowercase();

        let steps = if goal.contains("background") {
            vec![
                LogicalStep {
                    operation: "segmentation".into(),
                    purpose: "detect foreground".into(),
                },
                LogicalStep {
                    operation: "background_removal".into(),
                    purpose: "remove background".into(),
                },
                LogicalStep {
                    operation: "validate".into(),
                    purpose: "validate resulting image".into(),
                },
            ]
        } else if goal.contains("prompt") {
            vec![LogicalStep {
                operation: "txt2txt".into(),
                purpose: "improve prompt for image generation".into(),
            }]
        } else if intent.input_type == DataType::Text && intent.output_type == DataType::Image {
            vec![
                LogicalStep {
                    operation: "txt2txt".into(),
                    purpose: "improve prompt for image generation".into(),
                },
                LogicalStep {
                    operation: "txt2img".into(),
                    purpose: "generate image from text".into(),
                },
                LogicalStep {
                    operation: "validate".into(),
                    purpose: "validate resulting image".into(),
                },
            ]
        } else if intent.input_type == DataType::Image && intent.output_type == DataType::Text {
            vec![LogicalStep {
                operation: "img2txt".into(),
                purpose: "describe image".into(),
            }]
        } else if intent.input_type == DataType::Image && intent.output_type == DataType::Image {
            vec![
                LogicalStep {
                    operation: "img2img".into(),
                    purpose: "transform image".into(),
                },
                LogicalStep {
                    operation: "validate".into(),
                    purpose: "validate resulting image".into(),
                },
            ]
        } else {
            vec![LogicalStep {
                operation: "txt2txt".into(),
                purpose: "transform text".into(),
            }]
        };

        LogicalWorkflow { steps }
    }
}
