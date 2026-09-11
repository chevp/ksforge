use crate::domain::{Artifact, DataType};

use super::registry::{ParameterSpec, Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Text -> Image. Simulated: no model runs, an `Image` artifact is
/// fabricated from the text's description.
pub struct Txt2Img;

impl TechniqueExecutor for Txt2Img {
    fn execute(&self, inputs: &[Artifact], _attempt: u32) -> Result<Artifact, String> {
        let text = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Text)
            .ok_or_else(|| "txt2img requires a Text input".to_string())?;
        Ok(Artifact::new(
            format!("{}-img", text.id),
            DataType::Image,
            format!("image generated from text: {}", text.description),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("txt2img"),
        input_types: vec![DataType::Text],
        output_types: vec![DataType::Image],
        parameters: vec![ParameterSpec {
            name: "style".into(),
            description: "optional visual style hint (unused by the mock executor)".into(),
        }],
        metadata: TechniqueMetadata {
            cost: 5,
            latency_ms: 500,
            capabilities: vec!["image-generation".into()],
            constraints: vec![],
        },
    }
}
