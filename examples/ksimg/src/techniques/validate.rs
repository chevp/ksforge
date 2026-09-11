use crate::domain::{Artifact, DataType};

use super::registry::{Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Image -> ValidatedImage. This is a business-level technique, not the
/// static graph check in `workflow::validator` — it runs at execution
/// time like any other step and can fail like any other step.
pub struct Validate;

impl TechniqueExecutor for Validate {
    fn execute(&self, inputs: &[Artifact], _attempt: u32) -> Result<Artifact, String> {
        let image = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Image)
            .ok_or_else(|| "validate requires an Image input".to_string())?;
        Ok(Artifact::new(
            format!("{}-validated", image.id),
            DataType::ValidatedImage,
            format!("validated {}", image.id),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("validate"),
        input_types: vec![DataType::Image],
        output_types: vec![DataType::ValidatedImage],
        parameters: vec![],
        metadata: TechniqueMetadata {
            cost: 1,
            latency_ms: 30,
            capabilities: vec!["quality-check".into()],
            constraints: vec![],
        },
    }
}
