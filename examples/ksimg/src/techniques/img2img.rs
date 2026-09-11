use crate::domain::{Artifact, DataType};

use super::registry::{Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Image -> Image. Simulated: fabricates a transformed image artifact.
pub struct Img2Img;

impl TechniqueExecutor for Img2Img {
    fn execute(&self, inputs: &[Artifact], _attempt: u32) -> Result<Artifact, String> {
        let image = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Image)
            .ok_or_else(|| "img2img requires an Image input".to_string())?;
        Ok(Artifact::new(
            format!("{}-transformed", image.id),
            DataType::Image,
            format!("{} transformed into a new image", image.id),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("img2img"),
        input_types: vec![DataType::Image],
        output_types: vec![DataType::Image],
        parameters: vec![],
        metadata: TechniqueMetadata {
            cost: 5,
            latency_ms: 400,
            capabilities: vec!["image-transformation".into()],
            constraints: vec![],
        },
    }
}
