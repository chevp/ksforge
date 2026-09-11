use crate::domain::{Artifact, DataType};

use super::registry::{ParameterSpec, Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Image + Mask -> Image. Simulated: fabricates an image artifact whose
/// description records which mask it was cut with.
pub struct BackgroundRemoval;

impl TechniqueExecutor for BackgroundRemoval {
    fn execute(&self, inputs: &[Artifact], _attempt: u32) -> Result<Artifact, String> {
        let image = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Image)
            .ok_or_else(|| "background_removal requires an Image input".to_string())?;
        let mask = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Mask)
            .ok_or_else(|| "background_removal requires a Mask input".to_string())?;
        Ok(Artifact::new(
            format!("{}-nobg", image.id),
            DataType::Image,
            format!("{} with background removed using {}", image.id, mask.id),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("background_removal"),
        input_types: vec![DataType::Image, DataType::Mask],
        output_types: vec![DataType::Image],
        parameters: vec![ParameterSpec {
            name: "feather_px".into(),
            description: "softness of the cutout edge (unused by the mock executor)".into(),
        }],
        metadata: TechniqueMetadata {
            cost: 3,
            latency_ms: 150,
            capabilities: vec!["background-removal".into()],
            constraints: vec![],
        },
    }
}
