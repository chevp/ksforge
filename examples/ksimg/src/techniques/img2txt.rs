use crate::domain::{Artifact, DataType};

use super::registry::{Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Image -> Text. Simulated: fabricates a caption from the image's id.
pub struct Img2Txt;

impl TechniqueExecutor for Img2Txt {
    fn execute(&self, inputs: &[Artifact], _attempt: u32) -> Result<Artifact, String> {
        let image = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Image)
            .ok_or_else(|| "img2txt requires an Image input".to_string())?;
        Ok(Artifact::new(
            format!("{}-caption", image.id),
            DataType::Text,
            format!("text describing {}", image.id),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("img2txt"),
        input_types: vec![DataType::Image],
        output_types: vec![DataType::Text],
        parameters: vec![],
        metadata: TechniqueMetadata {
            cost: 3,
            latency_ms: 250,
            capabilities: vec!["image-captioning".into()],
            constraints: vec![],
        },
    }
}
