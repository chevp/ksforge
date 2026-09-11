use crate::domain::{Constraints, DataType, ProcessingIntent};

/// Turns a free-form user goal into a `ProcessingIntent`. It never
/// produces a workflow, a technique name, or a step order — see CLAUDE.md
/// section 6.
pub trait IntentAgent {
    fn interpret(&self, input: &str) -> ProcessingIntent;
}

/// Deterministic keyword rules standing in for an LLM, so the whole
/// example stays local and testable — see CLAUDE.md section 15.
pub struct MockIntentAgent;

impl IntentAgent for MockIntentAgent {
    fn interpret(&self, input: &str) -> ProcessingIntent {
        let text = input.to_lowercase();

        if text.contains("background") {
            return ProcessingIntent {
                input_type: DataType::Image,
                output_type: DataType::Image,
                goal: "Remove background".into(),
                constraints: Constraints::default(),
            };
        }
        if text.contains("generate") && text.contains("image") {
            return ProcessingIntent {
                input_type: DataType::Text,
                output_type: DataType::Image,
                goal: "Generate image".into(),
                constraints: Constraints::default(),
            };
        }
        if text.contains("improve") && text.contains("prompt") {
            return ProcessingIntent {
                input_type: DataType::Text,
                output_type: DataType::Text,
                goal: "Improve prompt for image generation".into(),
                constraints: Constraints::default(),
            };
        }
        if text.contains("describe") || text.contains("caption") {
            return ProcessingIntent {
                input_type: DataType::Image,
                output_type: DataType::Text,
                goal: "Describe image".into(),
                constraints: Constraints::default(),
            };
        }
        if text.contains("transform") && text.contains("image") {
            return ProcessingIntent {
                input_type: DataType::Image,
                output_type: DataType::Image,
                goal: "Transform image into another image".into(),
                constraints: Constraints::default(),
            };
        }

        ProcessingIntent {
            input_type: DataType::Text,
            output_type: DataType::Text,
            goal: "Transform text".into(),
            constraints: Constraints::default(),
        }
    }
}
