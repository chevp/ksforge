use crate::domain::{Artifact, DataType};

use super::registry::{Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Text -> Text. Simulated: rewrites the description, touches nothing
/// resembling a real NLP model.
pub struct Txt2Txt;

impl TechniqueExecutor for Txt2Txt {
    fn execute(&self, inputs: &[Artifact], _attempt: u32) -> Result<Artifact, String> {
        let text = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Text)
            .ok_or_else(|| "txt2txt requires a Text input".to_string())?;
        Ok(Artifact::new(
            format!("{}-txt", text.id),
            DataType::Text,
            format!("transformed text: {}", text.description),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("txt2txt"),
        input_types: vec![DataType::Text],
        output_types: vec![DataType::Text],
        parameters: vec![],
        metadata: TechniqueMetadata {
            cost: 1,
            latency_ms: 20,
            capabilities: vec!["text-rewrite".into()],
            constraints: vec![],
        },
    }
}
